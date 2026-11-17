//! piper-phonemize v1.5.9 (GPL-3.0, a separate download) for the Piper and Kokoro voices
//! (change `whisper-mode`, design D9b): text to espeak-ng phoneme codepoints, per sentence.
//!
//! espeak-ng keeps global state, so the library is initialized once per process with the
//! first voice's `espeak-ng-data` (every voice archive carries the same data) and is
//! called only from the voice thread.

use super::sherpa::load_library;
use super::TtsError;
use libloading::Library;
use std::ffi::{c_char, c_void, CString};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

type InitializeFn = unsafe extern "C" fn(*const c_char) -> i32;
type TextFn = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void;
type CountFn = unsafe extern "C" fn(*const c_void) -> i32;
type PhonemeCountFn = unsafe extern "C" fn(*const c_void, i32) -> i32;
type PhonemesFn = unsafe extern "C" fn(*const c_void, i32) -> *const i32;
type DestroyFn = unsafe extern "C" fn(*mut c_void);

struct Library_ {
    _core: Library,
    _api: Option<Library>,
    text: TextFn,
    sentences: CountFn,
    phoneme_count: PhonemeCountFn,
    phonemes: PhonemesFn,
    destroy: DestroyFn,
}

// SAFETY: function pointers into a library that stays loaded; calls are serialized by
// `CALLS`.
unsafe impl Send for Library_ {}
unsafe impl Sync for Library_ {}

static LIBRARY: OnceLock<Result<Library_, TtsError>> = OnceLock::new();
/// espeak-ng is not thread-safe: one call at a time in the process.
static CALLS: Mutex<()> = Mutex::new(());

/// A handle on the process's phonemizer.
#[derive(Clone, Copy)]
pub struct Phonemizer {
    library: &'static Library_,
}

impl Phonemizer {
    /// Loads the library from `folder` and initializes espeak-ng with `data` once.
    pub fn load(folder: &Path, data: &Path) -> Result<Self, TtsError> {
        let library = LIBRARY
            .get_or_init(|| {
                let core = load_library(&folder.join("piper_phonemize_core.dll"))?;
                let api = load_library(&folder.join("piper_phonemize_cxx_api.dll")).ok();
                // SAFETY: symbols of the pinned piper-phonemize v1.5.9 C API with its
                // header's signatures; the libraries live in the static.
                unsafe {
                    macro_rules! symbol {
                        ($name:literal) => {{
                            let name = concat!($name, "\0").as_bytes();
                            match core.get(name) {
                                Ok(symbol) => *symbol,
                                Err(error) => match api.as_ref().map(|api| api.get(name)) {
                                    Some(Ok(symbol)) => *symbol,
                                    _ => {
                                        return Err(TtsError::Load(format!(
                                            "piper-phonemize is missing {}: {error}",
                                            $name
                                        )))
                                    }
                                },
                            }
                        }};
                    }
                    let initialize: InitializeFn = symbol!("PiperPhonemizeInitialize");
                    let data_c = CString::new(data.to_string_lossy().as_bytes())
                        .map_err(|_| TtsError::Load("the data path has a NUL".to_owned()))?;
                    // espeak_Initialize's answer: the sample rate (22050) on success, a
                    // negative error code otherwise.
                    let status = initialize(data_c.as_ptr());
                    if status <= 0 {
                        return Err(TtsError::Load(format!(
                            "piper-phonemize could not initialize espeak-ng with {} (status {status})",
                            data.display()
                        )));
                    }
                    log::info!(
                        "phonemizer: piper-phonemize from {}, espeak-ng data {}",
                        folder.display(),
                        data.display()
                    );
                    Ok(Library_ {
                        text: symbol!("PiperPhonemizeText"),
                        sentences: symbol!("PiperPhonemizeResultGetNumSentences"),
                        phoneme_count: symbol!("PiperPhonemizeResultGetNumPhonemes"),
                        phonemes: symbol!("PiperPhonemizeResultGetPhonemes"),
                        destroy: symbol!("PiperPhonemizeDestroyResult"),
                        _core: core,
                        _api: api,
                    })
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(Self { library })
    }

    /// The phoneme codepoints of each sentence of `text` for the espeak `voice` (`ru`,
    /// `en-us`).
    pub fn phonemize(&self, text: &str, voice: &str) -> Result<Vec<Vec<i32>>, TtsError> {
        let text_c = CString::new(text.replace('\0', " "))
            .map_err(|_| TtsError::Synthesize("the text has a NUL".to_owned()))?;
        let voice_c = CString::new(voice)
            .map_err(|_| TtsError::Synthesize("the voice has a NUL".to_owned()))?;
        let _one_at_a_time = CALLS.lock().expect("phonemizer lock");
        let library = self.library;
        // SAFETY: the result is read with the library's accessors and destroyed once.
        unsafe {
            let result = (library.text)(text_c.as_ptr(), voice_c.as_ptr());
            if result.is_null() {
                return Err(TtsError::Synthesize(format!(
                    "piper-phonemize could not read the text with the voice {voice}"
                )));
            }
            let mut sentences = Vec::new();
            for sentence in 0..(library.sentences)(result) {
                let count = (library.phoneme_count)(result, sentence);
                let phonemes = (library.phonemes)(result, sentence);
                if count > 0 && !phonemes.is_null() {
                    sentences.push(std::slice::from_raw_parts(phonemes, count as usize).to_vec());
                }
            }
            (library.destroy)(result);
            Ok(sentences)
        }
    }
}
