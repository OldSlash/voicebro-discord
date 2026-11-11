//! Vosk behind the wake-word traits. The library is loaded at runtime, so the app starts
//! without it; open-vocabulary recognizers only (grammar-restricted recognition squeezes
//! ordinary speech onto the wake words); no partial word timestamps (they delay partials).

pub mod ffi;

use super::catalog::Asset;
use super::{
    LoadedModel, ModelSource, Recognition, SpeechRecognizer, WakeEngine, WakeError, Word,
    RECOGNIZER_SAMPLE_RATE,
};
use ffi::{Model, Recognizer, VoskApi};
use std::path::Path;
use std::sync::Arc;

/// The Vosk 0.3.45 Windows build (libvosk.dll and its MinGW runtime DLLs).
pub const LIBRARY: Asset = Asset {
    label: "Vosk library",
    url: "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-win64-0.3.45.zip",
    sha256: "f1dcc9cca460630f81ea8f71794f69c80bed6556d2a4e6237b5785e1d2dff34b",
    directory: "vosk-win64-0.3.45",
    marker: "libvosk.dll",
    size: 14_900_000,
};

pub struct VoskEngine;

impl WakeEngine for VoskEngine {
    fn id(&self) -> &'static str {
        super::catalog::VOSK_ENGINE
    }

    fn assets(&self, model: &ModelSource) -> Vec<Asset> {
        match model {
            ModelSource::Catalog(entry) => vec![LIBRARY, entry.asset],
            ModelSource::Local { .. } => vec![LIBRARY],
        }
    }

    fn load(&self, assets_dir: &Path, model_dir: &Path) -> Result<Box<dyn LoadedModel>, WakeError> {
        let library = assets_dir.join(LIBRARY.directory).join(LIBRARY.marker);
        let api = Arc::new(VoskApi::load(&library)?);
        api.set_log_level(-1);
        let model = Model::load(api, model_dir)?;
        Ok(Box::new(VoskModel { model }))
    }
}

pub struct VoskModel {
    model: Model,
}

impl LoadedModel for VoskModel {
    fn new_recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, WakeError> {
        let recognizer = Recognizer::new(&self.model, RECOGNIZER_SAMPLE_RATE as f32)?;
        Ok(Box::new(VoskRecognizer { recognizer }))
    }

    fn knows_word(&self, word: &str) -> Option<bool> {
        Some(self.model.has_word(word))
    }
}

pub struct VoskRecognizer {
    recognizer: Recognizer,
}

impl SpeechRecognizer for VoskRecognizer {
    fn accept(&mut self, pcm_16k_mono: &[i16]) -> Option<Recognition> {
        let (is_final, json) = self.recognizer.accept(pcm_16k_mono)?;
        Some(parse_recognition(&json, is_final))
    }
}

/// Vosk's result JSON: `partial`/`partial_result` for intermediate results, `text`/`result`
/// for final ones.
pub fn parse_recognition(json: &str, is_final: bool) -> Recognition {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Recognition {
            is_final,
            ..Recognition::default()
        };
    };
    let (text_key, words_key) = if is_final {
        ("text", "result")
    } else {
        ("partial", "partial_result")
    };
    let words = value
        .get(words_key)
        .and_then(|words| words.as_array())
        .map(|words| {
            words
                .iter()
                .filter_map(|word| {
                    Some(Word {
                        word: word.get("word")?.as_str()?.to_owned(),
                        end: word.get("end")?.as_f64()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Recognition {
        is_final,
        text: value
            .get(text_key)
            .and_then(|text| text.as_str())
            .unwrap_or_default()
            .to_owned(),
        words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::spotter::PhraseSpotter;
    use crate::wake::worker::SpeakerState;
    use crate::wake::{PhraseKind, PhraseSet};
    use std::path::PathBuf;

    #[test]
    fn recognition_json_is_parsed_for_partial_and_final_results() {
        let partial = parse_recognition(
            r#"{"partial":"hey luna","partial_result":[{"conf":1.0,"end":0.3,"start":0.1,"word":"hey"},{"conf":1.0,"end":0.8,"start":0.4,"word":"luna"}]}"#,
            false,
        );
        assert_eq!(partial.text, "hey luna");
        assert_eq!(partial.words.len(), 2);
        assert_eq!(partial.words[1].end, 0.8);
        let finished = parse_recognition(
            r#"{"text":"luna","result":[{"end":0.5,"word":"luna"}]}"#,
            true,
        );
        assert!(finished.is_final);
        assert_eq!(finished.words[0].word, "luna");
        assert!(parse_recognition("garbage", true).text.is_empty());
    }

    fn wav_samples(path: &str) -> Vec<i16> {
        let bytes = std::fs::read(path).expect("wav readable");
        let data = bytes
            .windows(4)
            .position(|window| window == b"data")
            .map(|index| index + 8)
            .expect("wav has a data chunk");
        bytes[data..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect()
    }

    fn phrase_set_from_env() -> PhraseSet {
        // "activation one, activation two | deactivation" in VOICEBRO_WAKE_PHRASES.
        let spec = std::env::var("VOICEBRO_WAKE_PHRASES").unwrap_or_default();
        let (activation, deactivation) = spec.split_once('|').unwrap_or((spec.as_str(), ""));
        let list = |text: &str| -> Vec<String> {
            text.split(',')
                .map(|p| p.trim().to_lowercase())
                .filter(|p| !p.is_empty())
                .collect()
        };
        PhraseSet {
            activation: list(activation),
            deactivation: list(deactivation),
        }
    }

    /// Loads the real library and model when `VOICEBRO_VOSK_DIR` points at the assets
    /// directory. `VOICEBRO_WAKE_PHRASES` gives the phrases ("a, b | c").
    /// `VOICEBRO_WAKE_WAV` (16 kHz mono 16-bit PCM) must trigger an activation, or with
    /// `VOICEBRO_WAKE_EXPECT=none` must not. `VOICEBRO_WAKE_SEQUENCES="a.wav=Deactivate;b.wav=Activate"`
    /// checks the last kind confirmed per file and that no activation follows a
    /// deactivation. `VOICEBRO_WAKE_NOISE=<amplitude>` adds steady noise with no trailing
    /// silence.
    #[test]
    fn vosk_corpus_when_assets_are_provided() {
        let Ok(base) = std::env::var("VOICEBRO_VOSK_DIR") else {
            return;
        };
        let base = PathBuf::from(base);
        let model_folder = std::env::var("VOICEBRO_WAKE_MODEL")
            .ok()
            .filter(|folder| !folder.trim().is_empty())
            .unwrap_or_else(|| {
                crate::wake::catalog::default_entry()
                    .asset
                    .directory
                    .to_owned()
            });
        eprintln!("corpus model: {model_folder}");
        let model = VoskEngine
            .load(&base, &base.join(&model_folder))
            .expect("model loads");
        let phrases = phrase_set_from_env();
        assert!(
            !phrases.is_empty(),
            "VOICEBRO_WAKE_PHRASES is required with VOICEBRO_VOSK_DIR"
        );
        for phrase in phrases.activation.iter().chain(&phrases.deactivation) {
            for word in phrase.split_whitespace() {
                assert_eq!(
                    model.knows_word(word),
                    Some(true),
                    "word {word} unknown to the model"
                );
            }
        }
        let trailing = 75; // 1.5 s of 20 ms chunks
        let silence = vec![0i16; 320];

        if let Ok(list) = std::env::var("VOICEBRO_WAKE_SEQUENCES") {
            for item in list.split(';').filter(|e| !e.is_empty()) {
                let (path, expected) = item.split_once('=').unwrap_or((item, ""));
                let mut samples = wav_samples(path);
                let noise = std::env::var("VOICEBRO_WAKE_NOISE")
                    .ok()
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0);
                let tail = if noise > 0 { 3 * 16_000 } else { 0 };
                samples.extend(std::iter::repeat_n(0, tail));
                let mut seed = 0x2545_f491_u32;
                if noise > 0 {
                    for sample in samples.iter_mut() {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let value = (seed % (2 * noise as u32 + 1)) as i32 - noise;
                        *sample = (*sample as i32 + value).clamp(-32_768, 32_767) as i16;
                    }
                }
                let extra_silence = if noise > 0 { 0 } else { trailing };
                let mut speaker =
                    SpeakerState::new(model.new_recognizer().unwrap(), PhraseSpotter::new());
                let mut kinds = Vec::new();
                for chunk in samples
                    .chunks(320)
                    .map(<[i16]>::to_vec)
                    .chain(std::iter::repeat_n(silence.clone(), extra_silence))
                {
                    for (kind, _, _) in speaker.accept_mono(&chunk, &phrases) {
                        kinds.push(format!("{kind:?}"));
                    }
                }
                eprintln!(
                    "sequence {path}: heard {:?} -> {kinds:?}",
                    speaker.take_heard()
                );
                assert_eq!(
                    kinds.last().map(String::as_str).unwrap_or(""),
                    expected,
                    "{path}: {kinds:?}"
                );
                if let Some(goodbye) = kinds.iter().position(|k| k == "Deactivate") {
                    assert!(
                        !kinds[goodbye..].iter().any(|k| k == "Activate"),
                        "{path}: an activation after the goodbye: {kinds:?}"
                    );
                }
            }
        }

        let Ok(wav) = std::env::var("VOICEBRO_WAKE_WAV") else {
            return;
        };
        let samples = wav_samples(&wav);
        let mut speaker = SpeakerState::new(model.new_recognizer().unwrap(), PhraseSpotter::new());
        let mut heard = None;
        for chunk in samples
            .chunks(320)
            .map(<[i16]>::to_vec)
            .chain(std::iter::repeat_n(silence.clone(), trailing))
        {
            if let Some(hit) = speaker
                .accept_mono(&chunk, &phrases)
                .into_iter()
                .find(|(kind, _, _)| *kind == PhraseKind::Activate)
            {
                heard = Some(hit);
                break;
            }
        }
        eprintln!(
            "wake phrase heard: {heard:?}; recognizer said {:?}",
            speaker.take_heard()
        );
        if std::env::var("VOICEBRO_WAKE_EXPECT").ok().as_deref() == Some("none") {
            assert!(heard.is_none(), "false trigger on {heard:?}");
        } else {
            assert!(
                heard.is_some(),
                "the wake phrase in the WAV file was not recognized"
            );
        }
    }
}
