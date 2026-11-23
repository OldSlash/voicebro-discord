//! whisper.cpp behind [`SpeechToText`] (change `whisper-mode`, designs D5, D8 and D11).
//!
//! The runtime's libraries are loaded once per process and stay loaded: ggml keeps a global
//! backend registry, and CUDA's libraries are not made to be unloaded. A model is loaded at
//! a join and freed after the last leave; between joins no model is in memory, and after a
//! model on the GPU the CUDA context is reset too (change `whisper-config-layout`, D9).

pub mod ffi;

use super::catalog::{CPU_RUNTIME, GPU_RUNTIME};
use super::{AbortFlag, DecodeOptions, RawSegment, RawTranscript, SpeechToText, SttError};
use crate::settings::WhisperDevice;
use ffi::{Context, WhisperRuntime};
use std::ffi::{c_void, CString};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static RUNTIME: Mutex<Option<Arc<WhisperRuntime>>> = Mutex::new(None);

/// The process's Whisper runtime, loaded on first use from the GPU runtime's folder when it
/// is on disk (it holds the CPU set too), else from the CPU runtime's.
pub fn runtime(assets_dir: &Path) -> Result<Arc<WhisperRuntime>, SttError> {
    let mut slot = RUNTIME.lock().expect("whisper runtime lock");
    if let Some(runtime) = slot.as_ref() {
        return Ok(Arc::clone(runtime));
    }
    let folder = [GPU_RUNTIME, CPU_RUNTIME]
        .into_iter()
        .find(|asset| crate::assets::is_installed(assets_dir, asset))
        .map(|asset| assets_dir.join(asset.directory))
        .ok_or_else(|| SttError::Load("the Whisper runtime is not on disk".to_owned()))?;
    let runtime = Arc::new(WhisperRuntime::load(&folder)?);
    log::info!(
        "Whisper runtime: whisper.cpp {} from {}, CPU backend {}",
        runtime.version,
        folder.display(),
        runtime.cpu_variant
    );
    *slot = Some(Arc::clone(&runtime));
    Ok(runtime)
}

/// The runtime if this process loaded one.
pub fn loaded_runtime() -> Option<Arc<WhisperRuntime>> {
    RUNTIME.lock().expect("whisper runtime lock").clone()
}

/// Which device a model runs on, and why when it is not the one asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice {
    pub requested: WhisperDevice,
    pub used: WhisperDevice,
    pub reason: Option<String>,
    /// "CUDA0: NVIDIA GeForce RTX 5090 Laptop GPU" or "CPU ggml-cpu-alderlake".
    pub backend: String,
}

impl DeviceChoice {
    /// The log line of a fallback, as the spec words it.
    pub fn fallback_line(&self) -> Option<String> {
        self.reason.as_ref().map(|reason| {
            format!(
                "Whisper device: {} requested, {} used: {reason}",
                self.requested.name(),
                self.used.name()
            )
        })
    }
}

/// Chooses the device (design D11): the GPU when asked for and a CUDA device is there,
/// else the CPU, with the reason.
pub fn choose_device(
    runtime: &WhisperRuntime,
    requested: WhisperDevice,
    assets_dir: &Path,
) -> DeviceChoice {
    let cpu = |reason: Option<String>| DeviceChoice {
        requested,
        used: WhisperDevice::Cpu,
        reason,
        backend: format!("CPU {}", runtime.cpu_variant),
    };
    if requested == WhisperDevice::Cpu {
        return cpu(None);
    }
    if !super::catalog::HAS_GPU_RUNTIME {
        return cpu(Some("Linux has no CUDA runtime".to_owned()));
    }
    match runtime.enable_cuda() {
        Ok(true) => match runtime.gpu() {
            Some(gpu) => DeviceChoice {
                requested,
                used: WhisperDevice::Gpu,
                reason: None,
                backend: format!("{}: {}", gpu.name, gpu.description),
            },
            None => cpu(Some("no CUDA device".to_owned())),
        },
        Ok(false) if crate::assets::is_installed(assets_dir, &GPU_RUNTIME) => cpu(Some(
            "the CPU runtime was loaded before the GPU runtime was downloaded; restart VoiceBro to use the GPU"
                .to_owned(),
        )),
        Ok(false) => cpu(Some("the GPU runtime is not on disk".to_owned())),
        Err(error) => cpu(Some(format!("CUDA did not load: {error}"))),
    }
}

/// The no-speech threshold of design D5: a segment above it is dropped.
pub const NO_SPEECH_THRESHOLD: f32 = 0.6;

/// A loaded Whisper model with the VAD model's path.
pub struct WhisperModel {
    context: Context,
    vad_model: CString,
    use_gpu: bool,
}

impl WhisperModel {
    /// Loads `model` (flash attention on the GPU, as whisper.cpp recommends).
    pub fn load(
        runtime: Arc<WhisperRuntime>,
        model: &Path,
        vad_model: &Path,
        use_gpu: bool,
    ) -> Result<Self, SttError> {
        let vad_model = CString::new(vad_model.to_string_lossy().as_bytes())
            .map_err(|_| SttError::Load("the VAD model's path has a NUL".to_owned()))?;
        let context = Context::load(runtime, model, use_gpu, use_gpu)?;
        Ok(Self {
            context,
            vad_model,
            use_gpu,
        })
    }

    pub fn uses_gpu(&self) -> bool {
        self.use_gpu
    }
}

/// whisper.cpp's abort callback, checked after each encoder pass and decoder step: true once
/// the transcription's [`AbortFlag`] is raised.
unsafe extern "C" fn abort_raised(data: *mut c_void) -> bool {
    // SAFETY: `data` is the `AtomicBool` of the `AbortFlag` given to `transcribe_until`,
    // which outlives the `whisper_full` call that calls this.
    unsafe { &*(data as *const AtomicBool) }.load(Ordering::Acquire)
}

/// whisper.cpp's encoder-begin callback, checked before each encoder pass after the language
/// detection's: false (stop) once the flag is raised.
unsafe extern "C" fn encoder_may_begin(_: *mut c_void, _: *mut c_void, data: *mut c_void) -> bool {
    // SAFETY: as for `abort_raised`.
    !unsafe { abort_raised(data) }
}

/// The decoding parameters of design D5 on top of the library's greedy defaults, with the
/// abort of change `whisper-config-layout` (design D8) reading `abort`.
pub fn decode_params(
    defaults: ffi::WhisperFullParams,
    options: &DecodeOptions,
    language: &CString,
    prompt: Option<&CString>,
    vad_model: &CString,
    abort: &AtomicBool,
) -> ffi::WhisperFullParams {
    let mut params = defaults;
    params.n_threads = if options.threads == 0 {
        super::auto_threads()
    } else {
        options.threads
    } as i32;
    params.no_context = true;
    params.single_segment = false;
    params.print_special = false;
    params.print_progress = false;
    params.print_realtime = false;
    params.print_timestamps = false;
    params.suppress_nst = true;
    params.no_speech_thold = NO_SPEECH_THRESHOLD;
    params.greedy.best_of = 1;
    params.temperature = 0.0;
    params.language = language.as_ptr();
    params.detect_language = false;
    params.initial_prompt = prompt.map_or(std::ptr::null(), |prompt| prompt.as_ptr());
    params.vad = true;
    params.vad_model_path = vad_model.as_ptr();
    let abort = abort as *const AtomicBool as *mut c_void;
    params.abort_callback = abort_raised as *const c_void;
    params.abort_callback_user_data = abort;
    params.encoder_begin_callback = encoder_may_begin as *const c_void;
    params.encoder_begin_callback_user_data = abort;
    params
}

impl SpeechToText for WhisperModel {
    fn transcribe(
        &mut self,
        audio: &[f32],
        options: &DecodeOptions,
    ) -> Result<RawTranscript, SttError> {
        self.transcribe_until(audio, options, &AbortFlag::default())
    }

    fn transcribe_until(
        &mut self,
        audio: &[f32],
        options: &DecodeOptions,
        abort: &AbortFlag,
    ) -> Result<RawTranscript, SttError> {
        let language = CString::new(options.language.as_bytes())
            .map_err(|_| SttError::Transcribe("the language has a NUL".to_owned()))?;
        let prompt = options
            .prompt
            .as_ref()
            .and_then(|prompt| CString::new(prompt.as_bytes()).ok());
        let params = decode_params(
            self.context.runtime().full_default_params(),
            options,
            &language,
            prompt.as_ref(),
            &self.vad_model,
            abort.as_atomic(),
        );
        self.context.full(params, audio)?;
        Ok(RawTranscript {
            segments: self
                .context
                .segments()
                .into_iter()
                .map(|segment| RawSegment {
                    text: segment.text,
                    no_speech_prob: segment.no_speech_prob,
                })
                .collect(),
            language: self.context.language(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 3.2 of change `whisper-mode`, gated by `VOICEBRO_WHISPER_DIR` (an assets folder
    /// with the CPU runtime, the VAD model and `whisper-base-q8_0` installed): loads the
    /// library, compares the greedy and context defaults with the v1.9.5 header's field by
    /// field (a wrong struct layout fails here, not in a call), and transcribes an Irina and
    /// a David WAV of the wake corpus.
    #[test]
    fn whisper_runtime_when_enabled() {
        let Ok(assets) = std::env::var("VOICEBRO_WHISPER_DIR") else {
            return;
        };
        let assets = std::path::PathBuf::from(assets);
        let started = std::time::Instant::now();
        let runtime = Arc::new(
            WhisperRuntime::load(&assets.join(CPU_RUNTIME.directory)).expect("runtime loads"),
        );
        eprintln!(
            "runtime {} loaded in {:?}, CPU backend {}, devices {:?}",
            runtime.version,
            started.elapsed(),
            runtime.cpu_variant,
            runtime.devices()
        );
        let defaults = runtime.full_default_params();
        let hardware = std::thread::available_parallelism().unwrap().get() as i32;
        assert_eq!(defaults.strategy, 0);
        assert_eq!(defaults.n_threads, hardware.min(4));
        assert_eq!(defaults.n_max_text_ctx, 16_384);
        assert_eq!((defaults.offset_ms, defaults.duration_ms), (0, 0));
        assert!(!defaults.translate && defaults.no_context && !defaults.no_timestamps);
        assert!(!defaults.single_segment && !defaults.print_special);
        assert!(defaults.print_progress && !defaults.print_realtime && defaults.print_timestamps);
        assert!(!defaults.token_timestamps);
        assert_eq!((defaults.thold_pt, defaults.thold_ptsum), (0.01, 0.01));
        assert_eq!((defaults.max_len, defaults.max_tokens), (0, 0));
        assert!(!defaults.split_on_word && !defaults.debug_mode && !defaults.tdrz_enable);
        assert_eq!(defaults.audio_ctx, 0);
        assert!(defaults.suppress_regex.is_null() && defaults.initial_prompt.is_null());
        assert!(!defaults.carry_initial_prompt && defaults.prompt_tokens.is_null());
        assert_eq!(defaults.prompt_n_tokens, 0);
        // SAFETY: the default language is a static "en".
        let language = unsafe { std::ffi::CStr::from_ptr(defaults.language) };
        assert_eq!(language.to_str().unwrap(), "en");
        assert!(!defaults.detect_language && defaults.suppress_blank && !defaults.suppress_nst);
        assert_eq!(
            (
                defaults.temperature,
                defaults.max_initial_ts,
                defaults.length_penalty
            ),
            (0.0, 1.0, -1.0)
        );
        assert_eq!(
            (
                defaults.temperature_inc,
                defaults.entropy_thold,
                defaults.logprob_thold,
                defaults.no_speech_thold
            ),
            (0.2, 2.4, -1.0, 0.6)
        );
        assert_eq!(defaults.greedy.best_of, 5);
        assert_eq!(
            (
                defaults.beam_search.beam_size,
                defaults.beam_search.patience
            ),
            (-1, -1.0)
        );
        assert!(defaults.new_segment_callback.is_null() && defaults.abort_callback.is_null());
        assert!(defaults.grammar_rules.is_null());
        assert_eq!((defaults.n_grammar_rules, defaults.i_start_rule), (0, 0));
        assert_eq!(defaults.grammar_penalty, 100.0);
        assert!(!defaults.vad && defaults.vad_model_path.is_null());
        assert_eq!(
            defaults.vad_params,
            ffi::WhisperVadParams {
                threshold: 0.5,
                min_speech_duration_ms: 250,
                min_silence_duration_ms: 100,
                max_speech_duration_s: f32::MAX,
                speech_pad_ms: 30,
                samples_overlap: 0.1,
            }
        );
        let context = runtime.context_default_params();
        assert!(context.use_gpu && context.flash_attn && !context.dtw_token_timestamps);
        assert_eq!(
            (
                context.gpu_device,
                context.dtw_aheads_preset,
                context.dtw_n_top
            ),
            (0, 0, -1)
        );
        assert_eq!(context.dtw_aheads.n_heads, 0);
        assert!(context.dtw_aheads.heads.is_null());
        assert_eq!(context.dtw_mem_size, 128 * 1024 * 1024);

        let model_path = assets.join("whisper-base-q8_0").join("ggml-base-q8_0.bin");
        let vad = assets
            .join(super::super::catalog::VAD_MODEL.directory)
            .join(super::super::catalog::VAD_MODEL.marker);
        let started = std::time::Instant::now();
        let mut model =
            WhisperModel::load(Arc::clone(&runtime), &model_path, &vad, false).expect("model");
        eprintln!("model loaded in {:?}", started.elapsed());
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("wake-corpus");
        for (wav, language) in [
            (corpus.join("ru").join("ru-positive.wav"), "ru"),
            (corpus.join("en").join("david-en-hey-bro.wav"), "auto"),
        ] {
            let audio = crate::stt::read_wav_16k(&wav).expect("wav");
            let started = std::time::Instant::now();
            let transcript = model
                .transcribe(
                    &audio,
                    &DecodeOptions {
                        language: language.to_owned(),
                        threads: 0,
                        prompt: None,
                    },
                )
                .expect("transcribed");
            eprintln!(
                "{}: {:.1} s of audio in {:?}: {:?}",
                wav.display(),
                audio.len() as f64 / 16_000.0,
                started.elapsed(),
                transcript
            );
            assert!(!transcript.segments.is_empty(), "{}", wav.display());
        }
    }
}
