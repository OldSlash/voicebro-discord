//! whisper.cpp v1.9.5 and its ggml libraries, loaded at run time (change `whisper-mode`,
//! design D8). The structs are mirrored field by field from the v1.9.5 `whisper.h`; the
//! pinned SHA-256 of the runtime and the `whisper_version()` check together make the mirror
//! valid, and a gated test compares the library's defaults with the header's. Raw pointers
//! never leave this module; every handle frees itself on drop.

use super::super::SttError;
use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};
use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `enum whisper_sampling_strategy`: greedy.
pub const WHISPER_SAMPLING_GREEDY: c_int = 0;

/// `struct whisper_aheads`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WhisperAheads {
    pub n_heads: usize,
    pub heads: *const c_void,
}

/// `struct whisper_context_params`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WhisperContextParams {
    pub use_gpu: bool,
    pub flash_attn: bool,
    pub gpu_device: c_int,
    pub dtw_token_timestamps: bool,
    pub dtw_aheads_preset: c_int,
    pub dtw_n_top: c_int,
    pub dtw_aheads: WhisperAheads,
    pub dtw_mem_size: usize,
}

/// `whisper_vad_params`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WhisperVadParams {
    pub threshold: c_float,
    pub min_speech_duration_ms: c_int,
    pub min_silence_duration_ms: c_int,
    pub max_speech_duration_s: c_float,
    pub speech_pad_ms: c_int,
    pub samples_overlap: c_float,
}

/// The `greedy` member of `whisper_full_params`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Greedy {
    pub best_of: c_int,
}

/// The `beam_search` member of `whisper_full_params`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamSearch {
    pub beam_size: c_int,
    pub patience: c_float,
}

/// `struct whisper_full_params`, in the header's order. The callbacks are never set and
/// stay opaque pointers.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WhisperFullParams {
    pub strategy: c_int,
    pub n_threads: c_int,
    pub n_max_text_ctx: c_int,
    pub offset_ms: c_int,
    pub duration_ms: c_int,
    pub translate: bool,
    pub no_context: bool,
    pub no_timestamps: bool,
    pub single_segment: bool,
    pub print_special: bool,
    pub print_progress: bool,
    pub print_realtime: bool,
    pub print_timestamps: bool,
    pub token_timestamps: bool,
    pub thold_pt: c_float,
    pub thold_ptsum: c_float,
    pub max_len: c_int,
    pub split_on_word: bool,
    pub max_tokens: c_int,
    pub debug_mode: bool,
    pub audio_ctx: c_int,
    pub tdrz_enable: bool,
    pub suppress_regex: *const c_char,
    pub initial_prompt: *const c_char,
    pub carry_initial_prompt: bool,
    pub prompt_tokens: *const i32,
    pub prompt_n_tokens: c_int,
    pub language: *const c_char,
    pub detect_language: bool,
    pub suppress_blank: bool,
    pub suppress_nst: bool,
    pub temperature: c_float,
    pub max_initial_ts: c_float,
    pub length_penalty: c_float,
    pub temperature_inc: c_float,
    pub entropy_thold: c_float,
    pub logprob_thold: c_float,
    pub no_speech_thold: c_float,
    pub greedy: Greedy,
    pub beam_search: BeamSearch,
    pub new_segment_callback: *const c_void,
    pub new_segment_callback_user_data: *mut c_void,
    pub progress_callback: *const c_void,
    pub progress_callback_user_data: *mut c_void,
    pub encoder_begin_callback: *const c_void,
    pub encoder_begin_callback_user_data: *mut c_void,
    pub abort_callback: *const c_void,
    pub abort_callback_user_data: *mut c_void,
    pub logits_filter_callback: *const c_void,
    pub logits_filter_callback_user_data: *mut c_void,
    pub grammar_rules: *const *const c_void,
    pub n_grammar_rules: usize,
    pub i_start_rule: usize,
    pub grammar_penalty: c_float,
    pub vad: bool,
    pub vad_model_path: *const c_char,
    pub vad_params: WhisperVadParams,
}

type VersionFn = unsafe extern "C" fn() -> *const c_char;
type ContextDefaultParamsFn = unsafe extern "C" fn() -> WhisperContextParams;
type InitFromFileFn = unsafe extern "C" fn(*const c_char, WhisperContextParams) -> *mut c_void;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type FullDefaultParamsFn = unsafe extern "C" fn(c_int) -> WhisperFullParams;
type FullFn = unsafe extern "C" fn(*mut c_void, WhisperFullParams, *const c_float, c_int) -> c_int;
type ContextCountFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type SegmentTextFn = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;
type SegmentFloatFn = unsafe extern "C" fn(*mut c_void, c_int) -> c_float;
type LangStrFn = unsafe extern "C" fn(c_int) -> *const c_char;
type LogCallback = unsafe extern "C" fn(c_int, *const c_char, *mut c_void);
type LogSetFn = unsafe extern "C" fn(Option<LogCallback>, *mut c_void);
type BackendLoadFn = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type DevCountFn = unsafe extern "C" fn() -> usize;
type DevGetFn = unsafe extern "C" fn(usize) -> *mut c_void;
type DevTypeFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type DevTextFn = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type ScoreFn = unsafe extern "C" fn() -> c_int;

/// `enum ggml_backend_dev_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Cpu,
    Gpu,
    IntegratedGpu,
    Accelerator,
    Other,
}

/// A device ggml registered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BackendDevice {
    pub kind: DeviceKind,
    pub name: String,
    pub description: String,
}

/// The CUDA libraries the GPU runtime's `ggml-cuda.dll` imports, loaded first by full path
/// so that ggml's plain `LoadLibraryW` binds them to the runtime folder.
const CUDA_LIBRARIES: &[&str] = &["cudart64_12.dll", "cublasLt64_12.dll", "cublas64_12.dll"];

/// whisper.cpp's and ggml's log lines go to the app's log: ggml's buffer sizes at info
/// (they tell how much memory the model holds on each device), errors and warnings as
/// warnings, the rest at debug.
unsafe extern "C" fn log_line(level: c_int, text: *const c_char, _: *mut c_void) {
    if text.is_null() {
        return;
    }
    // SAFETY: ggml passes a NUL-terminated string valid for the call.
    let line = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    let line = line.trim_end();
    if line.is_empty() {
        return;
    }
    // The sizes ("CUDA0 total size = 539 MB", "compute buffer (encode) = 52 MB") and the
    // device lines stay at info, so the log shows what Whisper holds (design D15).
    if line.contains(" MB") || line.contains("buffer size") || line.contains("device") {
        log::info!("[whisper] {line}");
    } else if level == 4 || level == 3 {
        log::warn!("[whisper] {line}");
    } else {
        log::debug!("[whisper] {line}");
    }
}

/// The loaded runtime: the libraries stay loaded as long as it lives.
pub struct WhisperRuntime {
    _preloaded: Vec<Library>,
    _ggml: Library,
    _whisper: Library,
    /// The CUDA libraries, once [`Self::enable_cuda`] loaded them.
    cuda: std::sync::Mutex<Option<Vec<Library>>>,
    pub folder: PathBuf,
    pub version: String,
    /// The CPU variant ggml runs (`ggml-cpu-alderlake`).
    pub cpu_variant: String,
    backend_load: BackendLoadFn,
    dev_count: DevCountFn,
    dev_get: DevGetFn,
    dev_type: DevTypeFn,
    dev_name: DevTextFn,
    dev_description: DevTextFn,
    context_default_params: ContextDefaultParamsFn,
    init_from_file: InitFromFileFn,
    free: FreeFn,
    full_default_params: FullDefaultParamsFn,
    full: FullFn,
    n_segments: ContextCountFn,
    segment_text: SegmentTextFn,
    segment_no_speech_prob: SegmentFloatFn,
    lang_id: ContextCountFn,
    lang_str: LangStrFn,
}

// SAFETY: the function pointers are plain C entry points of libraries that stay loaded for
// the life of the runtime; a context is only ever used by one thread at a time (`Context`).
unsafe impl Send for WhisperRuntime {}
unsafe impl Sync for WhisperRuntime {}

fn load_library(path: &Path) -> Result<Library, SttError> {
    // SAFETY: loading a library of the pinned, hash-verified runtime by its full path. The
    // altered search path resolves its own imports from the same folder.
    unsafe { Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }
        .map_err(|error| SttError::Load(format!("could not load {}: {error}", path.display())))
}

fn c_text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: the library returns a NUL-terminated string that outlives the call.
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

/// Registers one backend library with ggml.
fn load_backend(load: BackendLoadFn, path: &Path) -> Result<(), SttError> {
    let path_c = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| SttError::Load("a backend's path has a NUL".to_owned()))?;
    // SAFETY: ggml loads the library at the path and registers it; null means it failed.
    if unsafe { load(path_c.as_ptr()) }.is_null() {
        return Err(SttError::Load(format!(
            "ggml could not load {}",
            path.display()
        )));
    }
    Ok(())
}

/// The best `ggml-cpu-*.dll` variant of `folder` for this CPU, by each variant's own
/// `ggml_backend_score` (0: not supported here), as ggml chooses among them.
fn best_cpu_variant(folder: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(folder).ok()?;
    let mut best: Option<(c_int, PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("ggml-cpu-") && name.ends_with(".dll")) {
            continue;
        }
        let path = entry.path();
        let Ok(library) = load_library(&path) else {
            continue;
        };
        // SAFETY: the symbol is the variant's scoring entry point; called without arguments.
        let score = unsafe {
            library
                .get::<ScoreFn>(b"ggml_backend_score\0")
                .map(|score| score())
                .unwrap_or(0)
        };
        if score > 0 && best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((score, path));
        }
    }
    best.map(|(_, path)| path)
}

impl WhisperRuntime {
    /// Loads the runtime from `folder` (design D8): `ggml-base.dll` by full path, then
    /// `ggml.dll` and `whisper.dll`, the version check, the log, and the best CPU variant of
    /// the folder. CUDA is added only by [`Self::enable_cuda`], so the CPU device never
    /// creates a CUDA device, even from the GPU runtime's folder (design D11).
    pub fn load(folder: &Path) -> Result<Self, SttError> {
        let preloaded = vec![load_library(&folder.join("ggml-base.dll"))?];
        let ggml = load_library(&folder.join("ggml.dll"))?;
        let whisper = load_library(&folder.join("whisper.dll"))?;
        // SAFETY: symbols of the pinned whisper.cpp v1.9.5 build, with the signatures of its
        // header; the libraries stay loaded in `Self`.
        unsafe {
            macro_rules! symbol {
                ($library:expr, $name:literal) => {
                    *$library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|error| {
                            SttError::Load(format!(
                                "the Whisper runtime is missing {}: {error}",
                                $name
                            ))
                        })?
                };
            }
            // The registry functions are in ggml.dll, the device accessors in ggml-base.dll.
            macro_rules! ggml_symbol {
                ($name:literal) => {
                    match ggml.get(concat!($name, "\0").as_bytes()) {
                        Ok(symbol) => *symbol,
                        Err(_) => symbol!(preloaded[0], $name),
                    }
                };
            }
            let version_fn: VersionFn = symbol!(whisper, "whisper_version");
            let version = c_text(version_fn());
            if version != super::super::catalog::WHISPER_VERSION {
                return Err(SttError::Load(format!(
                    "{} is whisper.cpp {version}, but this build of VoiceBro needs {}; download the runtime again",
                    folder.join("whisper.dll").display(),
                    super::super::catalog::WHISPER_VERSION
                )));
            }
            let log_set: LogSetFn = symbol!(whisper, "whisper_log_set");
            log_set(Some(log_line), std::ptr::null_mut());
            let backend_load: BackendLoadFn = ggml_symbol!("ggml_backend_load");
            let variant = best_cpu_variant(folder).ok_or_else(|| {
                SttError::Load(format!(
                    "no ggml-cpu variant in {} runs on this CPU",
                    folder.display()
                ))
            })?;
            load_backend(backend_load, &variant)?;
            Ok(Self {
                context_default_params: symbol!(whisper, "whisper_context_default_params"),
                init_from_file: symbol!(whisper, "whisper_init_from_file_with_params"),
                free: symbol!(whisper, "whisper_free"),
                full_default_params: symbol!(whisper, "whisper_full_default_params"),
                full: symbol!(whisper, "whisper_full"),
                n_segments: symbol!(whisper, "whisper_full_n_segments"),
                segment_text: symbol!(whisper, "whisper_full_get_segment_text"),
                segment_no_speech_prob: symbol!(whisper, "whisper_full_get_segment_no_speech_prob"),
                lang_id: symbol!(whisper, "whisper_full_lang_id"),
                lang_str: symbol!(whisper, "whisper_lang_str"),
                backend_load,
                dev_count: ggml_symbol!("ggml_backend_dev_count"),
                dev_get: ggml_symbol!("ggml_backend_dev_get"),
                dev_type: ggml_symbol!("ggml_backend_dev_type"),
                dev_name: ggml_symbol!("ggml_backend_dev_name"),
                dev_description: ggml_symbol!("ggml_backend_dev_description"),
                cpu_variant: variant
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                cuda: std::sync::Mutex::new(None),
                _preloaded: preloaded,
                _ggml: ggml,
                _whisper: whisper,
                folder: folder.to_path_buf(),
                version,
            })
        }
    }

    /// Adds the CUDA backend of the folder: its libraries by full path, then
    /// `ggml-cuda.dll` through ggml. Once per process; `Ok(false)` when the folder has no
    /// CUDA backend (the CPU runtime).
    pub fn enable_cuda(&self) -> Result<bool, SttError> {
        let mut cuda = self.cuda.lock().expect("cuda lock");
        if cuda.is_some() {
            return Ok(true);
        }
        let backend = self.folder.join("ggml-cuda.dll");
        if !backend.is_file() {
            return Ok(false);
        }
        let mut libraries = Vec::new();
        for name in CUDA_LIBRARIES {
            libraries.push(load_library(&self.folder.join(name))?);
        }
        load_backend(self.backend_load, &backend)?;
        *cuda = Some(libraries);
        Ok(true)
    }

    /// Destroys the process's CUDA context, so its GPU memory and the driver's memory for
    /// it go back to the system (change `whisper-config-layout`, design D9):
    /// `cudaSetDevice(0)` and `cudaDeviceReset` of the CUDA runtime ggml-cuda uses. Device 0
    /// is the one a model is loaded on (`gpu_device` keeps its default). The caller makes
    /// sure no model is loaded and none is loading; the next load creates a new context.
    /// `None` when this process never loaded CUDA.
    pub fn reset_cuda(&self) -> Option<Result<(), String>> {
        type SetDeviceFn = unsafe extern "C" fn(c_int) -> c_int;
        type ResetFn = unsafe extern "C" fn() -> c_int;
        let cuda = self.cuda.lock().expect("cuda lock");
        // CUDA_LIBRARIES[0]: cudart64_12.dll.
        let cudart = cuda.as_ref()?.first()?;
        // SAFETY: entry points of the loaded CUDA runtime with their documented signatures,
        // called while no model holds the context.
        let result = unsafe {
            let set_device = cudart.get::<SetDeviceFn>(b"cudaSetDevice\0");
            let reset = cudart.get::<ResetFn>(b"cudaDeviceReset\0");
            match (set_device, reset) {
                (Ok(set_device), Ok(reset)) => match set_device(0) {
                    0 => match reset() {
                        0 => Ok(()),
                        code => Err(format!("cudaDeviceReset returned {code}")),
                    },
                    code => Err(format!("cudaSetDevice(0) returned {code}")),
                },
                (Err(error), _) | (_, Err(error)) => Err(format!("{error}")),
            }
        };
        Some(result)
    }

    /// The devices ggml has registered now.
    pub fn devices(&self) -> Vec<BackendDevice> {
        // SAFETY: registry accessors of the loaded ggml; indices below the count.
        unsafe {
            (0..(self.dev_count)())
                .map(|index| {
                    let device = (self.dev_get)(index);
                    BackendDevice {
                        kind: match (self.dev_type)(device) {
                            0 => DeviceKind::Cpu,
                            1 => DeviceKind::Gpu,
                            2 => DeviceKind::IntegratedGpu,
                            3 => DeviceKind::Accelerator,
                            _ => DeviceKind::Other,
                        },
                        name: c_text((self.dev_name)(device)),
                        description: c_text((self.dev_description)(device)),
                    }
                })
                .collect()
        }
    }

    /// The library's own context defaults.
    pub fn context_default_params(&self) -> WhisperContextParams {
        // SAFETY: returns a struct by value; no arguments.
        unsafe { (self.context_default_params)() }
    }

    /// The library's own decoding defaults for greedy decoding.
    pub fn full_default_params(&self) -> WhisperFullParams {
        // SAFETY: returns a struct by value for a valid strategy.
        unsafe { (self.full_default_params)(WHISPER_SAMPLING_GREEDY) }
    }

    /// The first dedicated GPU ggml registered.
    pub fn gpu(&self) -> Option<BackendDevice> {
        self.devices()
            .into_iter()
            .find(|device| device.kind == DeviceKind::Gpu)
    }
}

/// One segment of a transcription.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub no_speech_prob: f32,
}

/// A loaded model with its one state, the context's own: `whisper_full` applies the VAD
/// only on that state (`whisper_full_with_state` skips it). Not thread-safe (the header
/// says so): one thread owns it.
pub struct Context {
    runtime: Arc<WhisperRuntime>,
    context: *mut c_void,
}

// SAFETY: the context is used by one thread at a time; ownership moves with the value.
unsafe impl Send for Context {}

impl Context {
    pub fn load(
        runtime: Arc<WhisperRuntime>,
        model: &Path,
        use_gpu: bool,
        flash_attn: bool,
    ) -> Result<Self, SttError> {
        let path = CString::new(model.to_string_lossy().as_bytes())
            .map_err(|_| SttError::Load("the model's path has a NUL".to_owned()))?;
        let mut params = runtime.context_default_params();
        params.use_gpu = use_gpu;
        params.flash_attn = flash_attn;
        // SAFETY: a valid path and params by value; a null result is an error.
        let context = unsafe { (runtime.init_from_file)(path.as_ptr(), params) };
        if context.is_null() {
            return Err(SttError::Load(format!(
                "whisper.cpp could not load the model {}",
                model.display()
            )));
        }
        Ok(Self { runtime, context })
    }

    pub fn runtime(&self) -> &Arc<WhisperRuntime> {
        &self.runtime
    }

    /// Runs `whisper_full` on 16 kHz mono samples in -1..1. The pointers in `params`
    /// (language, prompt, VAD model path) must stay valid for the call.
    pub fn full(&mut self, params: WhisperFullParams, samples: &[f32]) -> Result<(), SttError> {
        // SAFETY: the context is live; the samples slice is valid for the call.
        let result = unsafe {
            (self.runtime.full)(
                self.context,
                params,
                samples.as_ptr(),
                samples.len() as c_int,
            )
        };
        if result != 0 {
            return Err(SttError::Transcribe(format!(
                "whisper_full returned {result}"
            )));
        }
        Ok(())
    }

    pub fn segments(&self) -> Vec<Segment> {
        // SAFETY: reading results of the last `full` from a live context.
        unsafe {
            let count = (self.runtime.n_segments)(self.context);
            (0..count)
                .map(|index| Segment {
                    text: c_text((self.runtime.segment_text)(self.context, index)),
                    no_speech_prob: (self.runtime.segment_no_speech_prob)(self.context, index),
                })
                .collect()
        }
    }

    /// The language the last `full` used or detected.
    pub fn language(&self) -> Option<String> {
        // SAFETY: reading the last result's language id, then its static code string.
        let id = unsafe { (self.runtime.lang_id)(self.context) };
        if id < 0 {
            return None;
        }
        let code = c_text(unsafe { (self.runtime.lang_str)(id) });
        (!code.is_empty()).then_some(code)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: freeing the context this value owns (with its state), once.
        unsafe { (self.runtime.free)(self.context) }
    }
}
