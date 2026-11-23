//! sherpa-onnx v2.0.1 through `libloading` (change `whisper-mode`, design D9b): Supertonic 3,
//! Piper and Kokoro on the CPU.
//!
//! - `onnxruntime.dll` is loaded first by full path: Windows 11 has its own, older
//!   `System32\onnxruntime.dll`, which must not be the one `sherpa-onnx-c-api.dll` binds to.
//! - The structs mirror the v2.0.1 `c-api.h` field by field; the pinned archive hash makes
//!   the mirror valid. Raw pointers stay in this module.
//! - Only `SherpaOnnxOfflineTtsGenerateWithConfig` is used; its progress callback returns 0
//!   to stop a synthesis a flush made useless.

use super::phonemize::Phonemizer;
use super::{Synth, TtsError};
use libloading::Library;
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Clone, Copy)]
struct VitsConfig {
    model: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    noise_scale_w: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MatchaConfig {
    acoustic_model: *const c_char,
    vocoder: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KokoroConfig {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
    dict_dir: *const c_char,
    lexicon: *const c_char,
    lang: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KittenConfig {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
    lexicon: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ZipvoiceConfig {
    tokens: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    vocoder: *const c_char,
    data_dir: *const c_char,
    lexicon: *const c_char,
    feat_scale: f32,
    t_shift: f32,
    target_rms: f32,
    guidance_scale: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PocketConfig {
    lm_flow: *const c_char,
    lm_main: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    text_conditioner: *const c_char,
    vocab_json: *const c_char,
    token_scores_json: *const c_char,
    voice_embedding_cache_capacity: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SupertonicConfig {
    duration_predictor: *const c_char,
    text_encoder: *const c_char,
    vector_estimator: *const c_char,
    vocoder: *const c_char,
    tts_json: *const c_char,
    unicode_indexer: *const c_char,
    voice_style: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ModelConfig {
    vits: VitsConfig,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    matcha: MatchaConfig,
    kokoro: KokoroConfig,
    kitten: KittenConfig,
    zipvoice: ZipvoiceConfig,
    pocket: PocketConfig,
    supertonic: SupertonicConfig,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TtsConfig {
    model: ModelConfig,
    rule_fsts: *const c_char,
    max_num_sentences: i32,
    rule_fars: *const c_char,
    silence_scale: f32,
}

#[repr(C)]
struct GeneratedAudio {
    samples: *const f32,
    n: i32,
    sample_rate: i32,
}

#[repr(C)]
struct GenerationConfig {
    silence_scale: f32,
    speed: f32,
    sid: i32,
    reference_audio: *const f32,
    reference_audio_len: i32,
    reference_sample_rate: i32,
    reference_text: *const c_char,
    num_steps: i32,
    extra: *const c_char,
    phoneme_codepoints: *const i32,
    phoneme_codepoints_lens: *const i32,
    phoneme_codepoints_num_sentences: i32,
    tokens: *const *const c_char,
    tokens_lens: *const i32,
    tokens_num_sentences: i32,
}

type Engine = c_void;
type ProgressFn = unsafe extern "C" fn(*const f32, i32, f32, *mut c_void) -> i32;
type CreateFn = unsafe extern "C" fn(*const TtsConfig) -> *const Engine;
type DestroyFn = unsafe extern "C" fn(*const Engine);
type SampleRateFn = unsafe extern "C" fn(*const Engine) -> i32;
type GenerateFn = unsafe extern "C" fn(
    *const Engine,
    *const c_char,
    *const GenerationConfig,
    Option<ProgressFn>,
    *mut c_void,
) -> *const GeneratedAudio;
type DestroyAudioFn = unsafe extern "C" fn(*const GeneratedAudio);

/// Which model a [`SherpaVoice`] runs, with its files.
#[derive(Debug, Clone, PartialEq)]
pub enum Model {
    /// Supertonic 3: speaker 0..9 of `voice.bin`, the language per sentence.
    Supertonic { folder: PathBuf, num_steps: i32 },
    /// A Piper voice: the `.onnx` file and its espeak voice (from the `.onnx.json`).
    Piper { folder: PathBuf, model: String },
    /// Kokoro English: phonemized for `en-us`.
    Kokoro { folder: PathBuf },
}

/// Loads `path` explicitly, resolving bundled imports from its folder on Windows.
pub(crate) fn load_library(path: &Path) -> Result<Library, TtsError> {
    // SAFETY: loading a library pinned by hash; its initializers are the vendor's.
    unsafe { crate::native_library::load(path) }
        .map_err(|error| TtsError::Load(format!("{}: {error}", path.display())))
}

fn c_path(path: &Path) -> Result<CString, TtsError> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| TtsError::Load(format!("{} has a NUL", path.display())))
}

/// The engine's library, loaded once per process and kept: onnxruntime is not made to be
/// unloaded.
struct Runtime {
    _onnxruntime: Library,
    _api: Library,
    create: CreateFn,
    destroy: DestroyFn,
    sample_rate: SampleRateFn,
    generate: GenerateFn,
    destroy_audio: DestroyAudioFn,
    onnxruntime_path: PathBuf,
}

// SAFETY: the function pointers and libraries are plain data; the engine handles are used
// from one thread at a time ([`SherpaVoice`] is `Send`, not `Sync`).
unsafe impl Send for Runtime {}
unsafe impl Sync for Runtime {}

static RUNTIME: std::sync::OnceLock<Result<Runtime, TtsError>> = std::sync::OnceLock::new();

fn runtime(folder: &Path) -> Result<&'static Runtime, TtsError> {
    RUNTIME
        .get_or_init(|| {
            let onnxruntime_path = folder.join(super::catalog::files::ONNXRUNTIME);
            let onnxruntime = load_library(&onnxruntime_path)?;
            let api = load_library(&folder.join(super::catalog::files::SHERPA))?;
            // SAFETY: symbols of the pinned sherpa-onnx v2.0.1 C API with the header's
            // signatures; the libraries live in the static.
            unsafe {
                macro_rules! symbol {
                    ($name:literal) => {
                        *api.get(concat!($name, "\0").as_bytes()).map_err(|error| {
                            TtsError::Load(format!("sherpa-onnx is missing {}: {error}", $name))
                        })?
                    };
                }
                let runtime = Runtime {
                    create: symbol!("SherpaOnnxCreateOfflineTts"),
                    destroy: symbol!("SherpaOnnxDestroyOfflineTts"),
                    sample_rate: symbol!("SherpaOnnxOfflineTtsSampleRate"),
                    generate: symbol!("SherpaOnnxOfflineTtsGenerateWithConfig"),
                    destroy_audio: symbol!("SherpaOnnxDestroyOfflineTtsGeneratedAudio"),
                    _onnxruntime: onnxruntime,
                    _api: api,
                    onnxruntime_path,
                };
                log::info!(
                    "voice engine: sherpa-onnx from {}, onnxruntime {}",
                    folder.display(),
                    runtime.onnxruntime_path.display()
                );
                Ok(runtime)
            }
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The `onnxruntime.dll` the engine loaded, once it did.
pub fn loaded_onnxruntime() -> Option<PathBuf> {
    RUNTIME
        .get()
        .and_then(|runtime| runtime.as_ref().ok())
        .map(|runtime| runtime.onnxruntime_path.clone())
}

/// A loaded sherpa-onnx voice.
pub struct SherpaVoice {
    runtime: &'static Runtime,
    engine: *const Engine,
    sample_rate: u32,
    model: Model,
    speaker: i32,
    phonemizer: Option<Phonemizer>,
    /// The espeak voice of a Piper or Kokoro model.
    espeak_voice: String,
}

// SAFETY: the engine is used from the thread that owns the voice only.
unsafe impl Send for SherpaVoice {}

impl SherpaVoice {
    /// Loads `model` with the engine of `engine_folder`; `phonemizer_folder` for Piper and
    /// Kokoro.
    pub fn load(
        engine_folder: &Path,
        phonemizer_folder: Option<&Path>,
        model: Model,
        speaker: i32,
        threads: i32,
    ) -> Result<Self, TtsError> {
        let runtime = runtime(engine_folder)?;
        let mut strings: Vec<CString> = Vec::new();
        let mut keep = |path: &Path| -> Result<*const c_char, TtsError> {
            let value = c_path(path)?;
            let pointer = value.as_ptr();
            strings.push(value);
            Ok(pointer)
        };
        // SAFETY: an all-zero config is the C API's "unset" (null strings, zero numbers).
        let mut config: TtsConfig = unsafe { std::mem::zeroed() };
        config.model.num_threads = threads.max(1);
        config.model.provider = c"cpu".as_ptr();
        config.max_num_sentences = 1;
        config.silence_scale = 0.2;
        let mut espeak_voice = String::new();
        let mut data_dir = None;
        match &model {
            Model::Supertonic { folder, .. } => {
                let s = &mut config.model.supertonic;
                s.duration_predictor = keep(&folder.join("duration_predictor.int8.onnx"))?;
                s.text_encoder = keep(&folder.join("text_encoder.int8.onnx"))?;
                s.vector_estimator = keep(&folder.join("vector_estimator.int8.onnx"))?;
                s.vocoder = keep(&folder.join("vocoder.int8.onnx"))?;
                s.tts_json = keep(&folder.join("tts.json"))?;
                s.unicode_indexer = keep(&folder.join("unicode_indexer.bin"))?;
                s.voice_style = keep(&folder.join("voice.bin"))?;
            }
            Model::Piper { folder, model } => {
                let v = &mut config.model.vits;
                v.model = keep(&folder.join(model))?;
                v.tokens = keep(&folder.join("tokens.txt"))?;
                v.noise_scale = 0.667;
                v.noise_scale_w = 0.8;
                v.length_scale = 1.0;
                let json = std::fs::read_to_string(folder.join(format!("{model}.json")))
                    .map_err(|error| TtsError::Load(format!("{model}.json: {error}")))?;
                let json: serde_json::Value = serde_json::from_str(&json)
                    .map_err(|error| TtsError::Load(format!("{model}.json: {error}")))?;
                espeak_voice = json["espeak"]["voice"]
                    .as_str()
                    .unwrap_or("en-us")
                    .to_owned();
                data_dir = Some(folder.join("espeak-ng-data"));
            }
            Model::Kokoro { folder } => {
                let k = &mut config.model.kokoro;
                k.model = keep(&folder.join("model.onnx"))?;
                k.voices = keep(&folder.join("voices.bin"))?;
                k.tokens = keep(&folder.join("tokens.txt"))?;
                k.length_scale = 1.0;
                espeak_voice = "en-us".to_owned();
                data_dir = Some(folder.join("espeak-ng-data"));
            }
        }
        let phonemizer = match (data_dir, phonemizer_folder) {
            (Some(data), Some(folder)) => Some(Phonemizer::load(folder, &data)?),
            (Some(_), None) => {
                return Err(TtsError::Load(
                    "this voice needs the phonemizer, which is not on disk".to_owned(),
                ))
            }
            (None, _) => None,
        };
        // SAFETY: the config's strings live in `strings` for the call.
        let engine = unsafe { (runtime.create)(&config) };
        drop(strings);
        if engine.is_null() {
            return Err(TtsError::Load(
                "sherpa-onnx could not create the voice (see the log)".to_owned(),
            ));
        }
        // SAFETY: a live engine.
        let sample_rate = unsafe { (runtime.sample_rate)(engine) } as u32;
        Ok(Self {
            runtime,
            engine,
            sample_rate,
            model,
            speaker,
            phonemizer,
            espeak_voice,
        })
    }
}

impl Drop for SherpaVoice {
    fn drop(&mut self) {
        // SAFETY: created by `SherpaOnnxCreateOfflineTts`, destroyed once.
        unsafe { (self.runtime.destroy)(self.engine) };
    }
}

/// The stop check the progress callback asks.
struct Progress<'a> {
    stop: &'a dyn Fn() -> bool,
    stopped: bool,
}

unsafe extern "C" fn progress(_: *const f32, _: i32, _: f32, arg: *mut c_void) -> i32 {
    // SAFETY: `arg` is the `Progress` of the running call.
    let progress = unsafe { &mut *arg.cast::<Progress>() };
    if (progress.stop)() {
        progress.stopped = true;
        0
    } else {
        1
    }
}

impl Synth for SherpaVoice {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn synthesize(
        &mut self,
        text: &str,
        language: Option<&str>,
        speed: f32,
        stop: &dyn Fn() -> bool,
    ) -> Result<Option<Vec<f32>>, TtsError> {
        let text_c = CString::new(text.replace('\0', " "))
            .map_err(|_| TtsError::Synthesize("the text has a NUL".to_owned()))?;
        // SAFETY: zeroed is the C API's "unset" for every field.
        let mut config: GenerationConfig = unsafe { std::mem::zeroed() };
        config.silence_scale = 0.2;
        config.speed = speed;
        config.sid = self.speaker;
        let extra;
        let (codepoints, lengths): (Vec<i32>, Vec<i32>);
        match &self.model {
            Model::Supertonic { num_steps, .. } => {
                config.num_steps = *num_steps;
                let language = language
                    .filter(|code| super::catalog::SUPERTONIC_LANGUAGES.contains(code))
                    .unwrap_or("na");
                extra = CString::new(format!("{{\"lang\":\"{language}\"}}")).expect("no NUL");
                config.extra = extra.as_ptr();
            }
            Model::Piper { .. } | Model::Kokoro { .. } => {
                let phonemizer = self
                    .phonemizer
                    .as_ref()
                    .ok_or_else(|| TtsError::Synthesize("no phonemizer".to_owned()))?;
                let sentences = phonemizer.phonemize(text, &self.espeak_voice)?;
                lengths = sentences
                    .iter()
                    .map(|sentence| sentence.len() as i32)
                    .collect();
                codepoints = sentences.concat();
                if codepoints.is_empty() {
                    return Ok(Some(Vec::new()));
                }
                config.phoneme_codepoints = codepoints.as_ptr();
                config.phoneme_codepoints_lens = lengths.as_ptr();
                config.phoneme_codepoints_num_sentences = lengths.len() as i32;
            }
        }
        let mut progress_state = Progress {
            stop,
            stopped: false,
        };
        // SAFETY: the engine is live; the config's pointers outlive the call; the callback
        // gets `progress_state`, which outlives the call.
        let audio = unsafe {
            (self.runtime.generate)(
                self.engine,
                text_c.as_ptr(),
                &config,
                Some(progress),
                (&raw mut progress_state).cast(),
            )
        };
        if progress_state.stopped {
            if !audio.is_null() {
                // SAFETY: returned by the generate call.
                unsafe { (self.runtime.destroy_audio)(audio) };
            }
            return Ok(None);
        }
        if audio.is_null() {
            return Err(TtsError::Synthesize(
                "sherpa-onnx returned no audio (see the log)".to_owned(),
            ));
        }
        // SAFETY: `samples` holds `n` floats until the audio is destroyed.
        let samples = unsafe {
            let audio_ref = &*audio;
            let samples = if audio_ref.samples.is_null() || audio_ref.n <= 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(audio_ref.samples, audio_ref.n as usize).to_vec()
            };
            (self.runtime.destroy_audio)(audio);
            samples
        };
        Ok(Some(samples))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The struct sizes of the v2.0.1 header on x64, counted by hand: a wrong mirror fails
    /// here before any call.
    #[test]
    fn the_mirrors_have_the_header_sizes() {
        assert_eq!(std::mem::size_of::<VitsConfig>(), 4 * 8 + 3 * 4 + 4 + 8);
        assert_eq!(std::mem::size_of::<SupertonicConfig>(), 7 * 8);
        assert_eq!(std::mem::size_of::<GeneratedAudio>(), 16);
        assert_eq!(std::mem::size_of::<GenerationConfig>(), 104);
        assert_eq!(std::mem::offset_of!(GenerationConfig, num_steps), 40);
        assert_eq!(std::mem::offset_of!(GenerationConfig, extra), 48);
        assert_eq!(std::mem::offset_of!(ModelConfig, provider), 64);
        assert_eq!(
            std::mem::offset_of!(TtsConfig, rule_fsts),
            std::mem::size_of::<ModelConfig>()
        );
    }
}
