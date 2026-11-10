//! Safe wrappers over the handful of `libvosk` functions the engine uses. Raw pointers
//! never leave this module; every handle frees itself on drop.

use super::super::WakeError;
use std::ffi::{c_char, c_float, c_int, c_short, c_void, CStr, CString};
use std::path::Path;
use std::sync::Arc;

type SetLogLevel = unsafe extern "C" fn(c_int);
type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type ModelFree = unsafe extern "C" fn(*mut c_void);
type ModelFindWord = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type RecognizerNew = unsafe extern "C" fn(*mut c_void, c_float) -> *mut c_void;
type RecognizerFlag = unsafe extern "C" fn(*mut c_void, c_int);
type AcceptWaveformS = unsafe extern "C" fn(*mut c_void, *const c_short, c_int) -> c_int;
type RecognizerText = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type RecognizerVoid = unsafe extern "C" fn(*mut c_void);

pub struct VoskApi {
    _library: libloading::os::windows::Library,
    set_log_level: SetLogLevel,
    model_new: ModelNew,
    model_free: ModelFree,
    model_find_word: ModelFindWord,
    recognizer_new: RecognizerNew,
    set_words: RecognizerFlag,
    accept_waveform_s: AcceptWaveformS,
    partial_result: RecognizerText,
    result: RecognizerText,
    recognizer_free: RecognizerVoid,
}

// SAFETY: the function pointers are plain C entry points of a library that stays loaded for
// the life of `VoskApi`; Vosk documents its model as shareable between threads.
unsafe impl Send for VoskApi {}
unsafe impl Sync for VoskApi {}

impl VoskApi {
    pub fn load(path: &Path) -> Result<Self, WakeError> {
        // SAFETY: loading the pinned, hash-verified Vosk release. The altered search path
        // lets Windows resolve its MinGW runtime DLLs from the same directory.
        unsafe {
            let library = libloading::os::windows::Library::load_with_flags(
                path,
                libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
            )
            .map_err(|error| {
                WakeError::Load(format!("could not load {}: {error}", path.display()))
            })?;
            macro_rules! symbol {
                ($name:literal) => {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|error| {
                            WakeError::Load(format!("libvosk is missing {}: {error}", $name))
                        })?
                };
            }
            Ok(Self {
                set_log_level: symbol!("vosk_set_log_level"),
                model_new: symbol!("vosk_model_new"),
                model_free: symbol!("vosk_model_free"),
                model_find_word: symbol!("vosk_model_find_word"),
                recognizer_new: symbol!("vosk_recognizer_new"),
                set_words: symbol!("vosk_recognizer_set_words"),
                accept_waveform_s: symbol!("vosk_recognizer_accept_waveform_s"),
                partial_result: symbol!("vosk_recognizer_partial_result"),
                result: symbol!("vosk_recognizer_result"),
                recognizer_free: symbol!("vosk_recognizer_free"),
                _library: library,
            })
        }
    }

    pub fn set_log_level(&self, level: i32) {
        // SAFETY: plain FFI call with an integer argument.
        unsafe { (self.set_log_level)(level) }
    }
}

pub struct Model {
    api: Arc<VoskApi>,
    pointer: *mut c_void,
}

// SAFETY: Vosk's model is read-only after loading and documented as shareable between
// recognizers on different threads.
unsafe impl Send for Model {}
unsafe impl Sync for Model {}

impl Model {
    pub fn load(api: Arc<VoskApi>, path: &Path) -> Result<Self, WakeError> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| WakeError::Load("the model path contains a NUL byte".to_owned()))?;
        // SAFETY: valid NUL-terminated path; the returned pointer is checked.
        let pointer = unsafe { (api.model_new)(c_path.as_ptr()) };
        if pointer.is_null() {
            return Err(WakeError::Load(format!(
                "Vosk could not load the model at {}",
                path.display()
            )));
        }
        Ok(Self { api, pointer })
    }

    pub fn has_word(&self, word: &str) -> bool {
        let Ok(word) = CString::new(word) else {
            return false;
        };
        // SAFETY: the model pointer is alive for &self; the word is NUL-terminated.
        unsafe { (self.api.model_find_word)(self.pointer, word.as_ptr()) >= 0 }
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        // SAFETY: the pointer came from vosk_model_new and is released once.
        unsafe { (self.api.model_free)(self.pointer) }
    }
}

/// An open-vocabulary recognizer bound to one model.
pub struct Recognizer {
    api: Arc<VoskApi>,
    pointer: *mut c_void,
}

// SAFETY: a recognizer is used from one thread at a time (the worker); moving it between
// threads is fine.
unsafe impl Send for Recognizer {}

impl Recognizer {
    pub fn new(model: &Model, sample_rate: f32) -> Result<Self, WakeError> {
        // SAFETY: the model is alive; the returned pointer is checked before use.
        let pointer = unsafe { (model.api.recognizer_new)(model.pointer, sample_rate) };
        if pointer.is_null() {
            return Err(WakeError::Load(
                "Vosk could not create a recognizer".to_owned(),
            ));
        }
        // SAFETY: the recognizer was just created. Word timings are requested for final
        // results only; partial word timings delay intermediate results by seconds.
        unsafe { (model.api.set_words)(pointer, 1) };
        Ok(Self {
            api: Arc::clone(&model.api),
            pointer,
        })
    }

    /// Feeds 16 kHz mono audio; returns `(is_final, json)`.
    pub fn accept(&mut self, samples: &[i16]) -> Option<(bool, String)> {
        // SAFETY: the recognizer is alive; the slice outlives the call.
        let is_final = unsafe {
            (self.api.accept_waveform_s)(self.pointer, samples.as_ptr(), samples.len() as c_int)
        } == 1;
        let getter = if is_final {
            self.api.result
        } else {
            self.api.partial_result
        };
        // SAFETY: Vosk returns a NUL-terminated string owned by the recognizer, valid until
        // the next call on it; it is copied immediately.
        let json = unsafe {
            let pointer = getter(self.pointer);
            if pointer.is_null() {
                return None;
            }
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        };
        Some((is_final, json))
    }
}

impl Drop for Recognizer {
    fn drop(&mut self) {
        // SAFETY: the pointer came from vosk_recognizer_new and is released once.
        unsafe { (self.api.recognizer_free)(self.pointer) }
    }
}
