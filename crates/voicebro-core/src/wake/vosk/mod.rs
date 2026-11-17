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
    page: "https://github.com/alphacep/vosk-api/releases/tag/v0.3.45",
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

    /// A 16-bit mono WAV as the recognizer hears it: 16 kHz files as they are, 48 kHz files
    /// (the Codex voice's recordings) through the app's own 48 → 16 kHz downsampler, fed as
    /// stereo the way Discord audio reaches it.
    fn wav_samples(path: &str) -> Vec<i16> {
        let bytes = std::fs::read(path).expect("wav readable");
        let format = bytes
            .windows(4)
            .position(|window| window == b"fmt ")
            .map(|index| index + 8)
            .expect("wav has a fmt chunk");
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let channels = u16_at(format + 2);
        let rate = u32::from_le_bytes(bytes[format + 4..format + 8].try_into().unwrap());
        let bits = u16_at(format + 14);
        assert!(
            channels == 1 && bits == 16,
            "{path}: want 16-bit mono, got {channels} channels at {bits} bits"
        );
        let data = bytes
            .windows(4)
            .position(|window| window == b"data")
            .map(|index| index + 8)
            .expect("wav has a data chunk");
        let mono: Vec<i16> = bytes[data..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect();
        match rate {
            16_000 => mono,
            48_000 => {
                let stereo: Vec<i16> = mono.iter().flat_map(|&sample| [sample, sample]).collect();
                crate::audio::dsp::StereoDownsampler::new(3).process(&stereo)
            }
            other => panic!("{path}: want 16 or 48 kHz, got {other} Hz"),
        }
    }

    /// Phrases as the settings fields take them: "a, b | c" (activation | deactivation),
    /// comma-separated, each one or more words.
    fn phrase_set(spec: &str) -> PhraseSet {
        let (activation, deactivation) = spec.split_once('|').unwrap_or((spec, ""));
        let list = |text: &str| -> Vec<String> {
            text.split([',', '\n'])
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect()
        };
        PhraseSet::new(&list(activation), &list(deactivation))
    }

    fn phrase_set_from_env() -> PhraseSet {
        phrase_set(&std::env::var("VOICEBRO_WAKE_PHRASES").unwrap_or_default())
    }

    /// Task 4.5: loads every catalog model from `VOICEBRO_VOSK_DIR` through the real
    /// library when `VOICEBRO_WAKE_CATALOG=1` is set too, and checks that each knows every
    /// word of its default phrases (scenario "Every default is known"). Prints the load time
    /// and the unknown words of every model, then fails on any.
    #[test]
    fn catalog_models_load_when_enabled() {
        let (Ok(base), Ok("1")) = (
            std::env::var("VOICEBRO_VOSK_DIR"),
            std::env::var("VOICEBRO_WAKE_CATALOG").as_deref(),
        ) else {
            return;
        };
        let base = PathBuf::from(base);
        let mut failures = Vec::new();
        for entry in crate::wake::catalog::entries() {
            let words: Vec<&str> = entry
                .default_activation
                .iter()
                .chain(entry.default_deactivation)
                .copied()
                .collect();
            let started = std::time::Instant::now();
            let model = match VoskEngine.load(&base, &base.join(entry.asset.directory)) {
                Ok(model) => model,
                Err(error) => {
                    eprintln!("{}: does not load: {error}", entry.id);
                    failures.push(format!("{} does not load", entry.id));
                    continue;
                }
            };
            let loaded = started.elapsed();
            let unknown: Vec<&str> = words
                .iter()
                .flat_map(|phrase| phrase.split_whitespace())
                .filter(|word| model.knows_word(word) != Some(true))
                .collect();
            eprintln!(
                "{}: loaded in {} ms; unknown words: {unknown:?}",
                entry.id,
                loaded.as_millis()
            );
            if !unknown.is_empty() {
                failures.push(format!("{} does not know {unknown:?}", entry.id));
            }
            // A vocabulary that "knows" nonsense would make the check above meaningless.
            if model.knows_word("zzqxvkw") != Some(false) {
                failures.push(format!("{} claims to know a nonsense word", entry.id));
            }
            // Candidates for a default that failed the corpus, only printed:
            // `VOICEBRO_WAKE_CANDIDATES="<model id>=word, two words;<model id>=…"`.
            for spec in std::env::var("VOICEBRO_WAKE_CANDIDATES")
                .unwrap_or_default()
                .split(';')
            {
                let Some((id, list)) = spec.split_once('=') else {
                    continue;
                };
                if id.trim() != entry.id {
                    continue;
                }
                for candidate in list.split(',').map(str::trim).filter(|c| !c.is_empty()) {
                    let missing: Vec<&str> = candidate
                        .split_whitespace()
                        .filter(|word| model.knows_word(word) != Some(true))
                        .collect();
                    eprintln!(
                        "{}: candidate «{candidate}» {}",
                        entry.id,
                        if missing.is_empty() {
                            "known".to_owned()
                        } else {
                            format!("unknown {missing:?}")
                        }
                    );
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Loads the real library and model when `VOICEBRO_VOSK_DIR` points at the assets
    /// directory. `VOICEBRO_WAKE_PHRASES` gives the phrases ("a, b | c").
    /// WAVs are 16-bit mono at 16 kHz, or at 48 kHz through the app's downsampler.
    /// `VOICEBRO_WAKE_WAV` must trigger an activation, or with
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
        let (_, unknown) = crate::wake::worker::usable_phrases(
            model.as_ref(),
            &[phrases.activation.clone(), phrases.deactivation.clone()].concat(),
        );
        assert!(
            unknown.is_empty(),
            "words unknown to the model: {unknown:?}"
        );
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
                let mut confirmed = Vec::new();
                for chunk in samples
                    .chunks(320)
                    .map(<[i16]>::to_vec)
                    .chain(std::iter::repeat_n(silence.clone(), extra_silence))
                {
                    for (kind, phrase, since_end) in speaker.accept_mono(&chunk, &phrases) {
                        kinds.push(format!("{kind:?}"));
                        confirmed.push(format!("«{phrase}» {since_end:.2} s after its end"));
                    }
                }
                eprintln!(
                    "sequence {path}: heard {:?} -> {kinds:?} {confirmed:?}",
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

    /// 16 kHz mono as 48 kHz stereo Discord frames of 20 ms (each sample held for three),
    /// for the worker's real input path through its 48 → 16 kHz downsampler.
    fn discord_frames(mono: &[i16]) -> Vec<Vec<i16>> {
        mono.chunks(320)
            .map(|chunk| {
                let mut frame: Vec<i16> = chunk
                    .iter()
                    .flat_map(|&sample| std::iter::repeat_n(sample, 6))
                    .collect();
                frame.resize(1920, 0);
                frame
            })
            .collect()
    }

    /// One run of [`vosk_gaps_when_assets_are_provided`]: the confirmed phrases with the
    /// stream second of each, what Vosk heard, and the ticks from the WAV's last frame to
    /// the first deactivation (negative: before the end).
    struct GapRun {
        hits: Vec<(PhraseKind, String, f64)>,
        heard: Vec<String>,
        end_to_deactivation_ms: Option<i64>,
    }

    /// How the speaker's frames reach the recognizer.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Feeding {
        /// The worker before change `voice-recognition-rework`: zeros fed once a speaker's
        /// frames stop for 60 ms (3 ticks), one more per tick, up to 75.
        OldClock,
        /// The frames back to back, the missing ticks ignored.
        BackToBack,
        /// The worker of design D1: frames and receive ticks through `SpeakerState`.
        Ticks,
    }

    /// Plays `frames` as a speaker whose frames go missing for `hole` ticks every
    /// `every` frames (songbird's refill: the audio is delayed, not lost), then Discord's
    /// end of speech (5 zero frames, then no frames for 2 s of ticks).
    fn run_with_holes(
        model: &dyn crate::wake::LoadedModel,
        frames: &[Vec<i16>],
        phrases: &PhraseSet,
        feeding: Feeding,
        every: usize,
        phase: usize,
        hole: u64,
    ) -> GapRun {
        let mut speaker = SpeakerState::new(model.new_recognizer().unwrap(), PhraseSpotter::new());
        let mut stream = frames.to_vec();
        stream.extend(std::iter::repeat_n(vec![0i16; 1920], 5));
        let wav_end_index = frames.len() - 1;
        let mut run = GapRun {
            hits: Vec::new(),
            heard: Vec::new(),
            end_to_deactivation_ms: None,
        };
        let mut tick = 0u64;
        let mut wav_end_tick = None;
        let mut last_frame_tick = 0u64;
        let mut old_fed = 0u64;
        let now = std::time::Instant::now();
        let frames_end = frames.len() as u64;
        let record = |hits: Vec<(PhraseKind, String, f64)>,
                      tick: u64,
                      speaker: &mut SpeakerState,
                      wav_end_tick: Option<u64>,
                      run: &mut GapRun| {
            for (kind, phrase, since_end) in hits {
                if kind == PhraseKind::Deactivate && run.end_to_deactivation_ms.is_none() {
                    // Before the WAV's end, its last frame's tick is not known yet; without
                    // holes it is the frame count.
                    let end = wav_end_tick.unwrap_or(frames_end);
                    run.end_to_deactivation_ms = Some((tick as i64 - end as i64) * 20);
                }
                run.hits.push((kind, phrase, since_end));
            }
            if let Some(text) = speaker.take_heard() {
                run.heard.push(text);
            }
        };
        let old_silence = |speaker: &mut SpeakerState, quiet: u64, fed: &mut u64| {
            let mut hits = Vec::new();
            if quiet >= 3 {
                let due = quiet.min(75);
                while *fed < due {
                    *fed += 1;
                    hits.extend(speaker.accept_mono(&[0i16; 320], phrases));
                }
            }
            hits
        };
        for (index, frame) in stream.iter().enumerate() {
            if index > 0 && (index + phase).is_multiple_of(every) {
                // The hole: ticks pass, the speaker's frames do not come.
                for _ in 0..hole {
                    tick += 1;
                    let hits = match feeding {
                        Feeding::Ticks => speaker.on_tick(tick, phrases),
                        Feeding::OldClock => {
                            old_silence(&mut speaker, tick - last_frame_tick, &mut old_fed)
                        }
                        Feeding::BackToBack => Vec::new(),
                    };
                    record(hits, tick, &mut speaker, wav_end_tick, &mut run);
                }
            }
            tick += 1;
            old_fed = 0;
            last_frame_tick = tick;
            let hits = match feeding {
                Feeding::Ticks => {
                    let mut hits = speaker.accept_discord(frame, phrases, tick, now);
                    hits.extend(speaker.on_tick(tick, phrases));
                    hits
                }
                _ => speaker.accept_discord(frame, phrases, tick, now),
            };
            if index == wav_end_index {
                wav_end_tick = Some(tick);
            }
            record(hits, tick, &mut speaker, wav_end_tick, &mut run);
        }
        // Discord sends nothing more; 2 s of receive ticks pass.
        for _ in 0..100 {
            tick += 1;
            let hits = match feeding {
                Feeding::Ticks => speaker.on_tick(tick, phrases),
                Feeding::OldClock | Feeding::BackToBack => {
                    old_silence(&mut speaker, tick - last_frame_tick, &mut old_fed)
                }
            };
            record(hits, tick, &mut speaker, wav_end_tick, &mut run);
        }
        run
    }

    /// Task 1.2 / 2.3: the regression test of change `voice-recognition-rework` with the
    /// real model, gated by `VOICEBRO_VOSK_DIR`. Feeds `VOICEBRO_WAKE_GAPS_WAV` (default:
    /// the corpus' `ru-goodbye.wav` next to the assets folder; several files separated by
    /// `;`) as a speaker whose frames go missing for 6 ticks every 0.5 s, as songbird's
    /// refill does: with zeros where the frames were missing (the old worker), back to back,
    /// and through the D1 worker's ticks. Model `VOICEBRO_WAKE_GAPS_MODEL` (default the
    /// Russian one) and phrases `VOICEBRO_WAKE_GAPS_PHRASES` (default "братан | отбой").
    /// Then the end of a sentence without holes, old worker against D1: the time from the
    /// WAV's end to the deactivation. The D1 run must equal the back-to-back run, and the
    /// default file must confirm the deactivation.
    #[test]
    fn vosk_gaps_when_assets_are_provided() {
        let Ok(base) = std::env::var("VOICEBRO_VOSK_DIR") else {
            return;
        };
        let base = PathBuf::from(base);
        let default_wav = base
            .parent()
            .map(|parent| parent.join("ru").join("ru-goodbye.wav"))
            .unwrap_or_default();
        let wavs = std::env::var("VOICEBRO_WAKE_GAPS_WAV")
            .unwrap_or_else(|_| default_wav.display().to_string());
        let model_folder = std::env::var("VOICEBRO_WAKE_GAPS_MODEL")
            .unwrap_or_else(|_| crate::wake::catalog::RUSSIAN_SMALL.to_owned());
        let phrases = phrase_set(
            &std::env::var("VOICEBRO_WAKE_GAPS_PHRASES")
                .unwrap_or_else(|_| "братан | отбой".to_owned()),
        );
        let model = VoskEngine
            .load(&base, &base.join(&model_folder))
            .expect("model loads");
        for wav in wavs.split(';').filter(|path| !path.trim().is_empty()) {
            if !std::path::Path::new(wav).is_file() {
                eprintln!("gaps: {wav} is missing; skipped");
                continue;
            }
            let frames = discord_frames(&wav_samples(wav));
            eprintln!("== gaps: {wav} ({} frames)", frames.len());
            let kinds = |run: &GapRun| -> Vec<(PhraseKind, String)> {
                run.hits.iter().map(|(k, p, _)| (*k, p.clone())).collect()
            };
            let deactivated = |run: &GapRun| {
                run.hits
                    .iter()
                    .any(|(kind, _, _)| *kind == PhraseKind::Deactivate)
            };
            // Five phases of the holes against the speech, so they fall into different words.
            let mut confirmed = [0usize; 3];
            for phase in [0, 5, 10, 15, 20] {
                let mut runs = Vec::new();
                for feeding in [Feeding::OldClock, Feeding::BackToBack, Feeding::Ticks] {
                    let run =
                        run_with_holes(model.as_ref(), &frames, &phrases, feeding, 25, phase, 6);
                    eprintln!(
                        "  holes of 6 ticks every 0.5 s, phase {phase}, {feeding:?}: heard {:?}; confirmed {:?}",
                        run.heard,
                        run.hits
                            .iter()
                            .map(|(kind, phrase, since)| format!(
                                "{kind:?} «{phrase}» {since:.2} s after its end"
                            ))
                            .collect::<Vec<_>>()
                    );
                    runs.push(run);
                }
                for (count, run) in confirmed.iter_mut().zip(&runs) {
                    *count += usize::from(deactivated(run));
                }
                assert_eq!(
                    kinds(&runs[2]),
                    kinds(&runs[1]),
                    "{wav}: the D1 worker hears what the back-to-back stream does"
                );
                if wavs == default_wav.display().to_string() {
                    assert!(
                        deactivated(&runs[2]),
                        "{wav}: the D1 run confirms the deactivation"
                    );
                }
            }
            eprintln!(
                "  deactivation confirmed in 5 phases: zeros (old worker) {}, back to back {}, D1 ticks {}",
                confirmed[0], confirmed[1], confirmed[2]
            );
            // The end of a sentence as Discord sends it: the speech without the WAV's own
            // trailing silence, then 5 zero frames and nothing.
            let mut spoken = frames.clone();
            while spoken
                .last()
                .is_some_and(|frame| crate::audio::peak(frame) < 500)
            {
                spoken.pop();
            }
            for feeding in [Feeding::OldClock, Feeding::Ticks] {
                let run =
                    run_with_holes(model.as_ref(), &spoken, &phrases, feeding, usize::MAX, 0, 0);
                eprintln!(
                    "  end of the sentence, no holes, {feeding:?}: deactivation {} ms after the WAV's last frame",
                    run.end_to_deactivation_ms
                        .map(|ms| ms.to_string())
                        .unwrap_or_else(|| "never".to_owned())
                );
            }
        }
    }
}
