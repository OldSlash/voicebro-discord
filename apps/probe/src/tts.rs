//! Voice probes of change `whisper-mode` (task 6.7): one sentence through a voice, or the
//! fixed sentence list with 2, 4 and 8 threads: load time, time to the first audio, the
//! real-time factor, WAVs for listening in `target\tts-listen\`, and how much of each output
//! Whisper reads back (the intelligibility check).

use crate::whisper::{assets_dir, ensure, flag, positional};
use crate::Outcome;
use anyhow::{anyhow, bail};
use std::path::{Path, PathBuf};
use std::time::Instant;
use voicebro_core::settings::phrase_words;
use voicebro_core::stt::SpeechToText;
use voicebro_core::tts::catalog;
use voicebro_core::tts::manager::{Loader, RealLoader};
use voicebro_core::tts::wav_bytes;

/// The fixed sentences: neutral places only.
const ENGLISH: &[&str] = &[
    "It is sunny in Bangkok today, around thirty three degrees.",
    "In London it is cloudy, with some rain in the evening.",
    "The flight from London to Bangkok takes about eleven hours.",
    "Sure, let me check that for you.",
];
const RUSSIAN: &[&str] = &[
    "Сегодня в Бангкоке солнечно, около тридцати трёх градусов.",
    "В Лондоне облачно, вечером возможен дождь.",
    "Перелёт из Лондона в Бангкок занимает около одиннадцати часов.",
    "Хорошо, сейчас посмотрю.",
];

fn install(voice: &str, dir: &Path, details: &mut Vec<String>) -> anyhow::Result<()> {
    if catalog::windows_voice(voice).is_some() {
        return Ok(());
    }
    let entry = catalog::find(voice).ok_or_else(|| anyhow!("{voice} is not a voice"))?;
    for asset in catalog::assets_for(entry) {
        ensure(&asset, dir, details)?;
    }
    Ok(())
}

fn languages(voice: &str) -> Vec<(&'static str, &'static [&'static str])> {
    let language = catalog::find(voice).map_or("", |entry| entry.language);
    if language == "multi" {
        vec![("en", ENGLISH), ("ru", RUSSIAN)]
    } else if language.starts_with("ru") || voice.contains("ruRU") || voice.contains("Irina") {
        vec![("ru", RUSSIAN)]
    } else {
        vec![("en", ENGLISH)]
    }
}

/// The share of the source's words Whisper wrote back, in order-free matching.
fn word_score(source: &str, heard: &str) -> f64 {
    let wanted = phrase_words(source);
    let mut got = phrase_words(heard);
    let mut hits = 0;
    for word in &wanted {
        if let Some(index) = got.iter().position(|found| found == word) {
            got.remove(index);
            hits += 1;
        }
    }
    hits as f64 / wanted.len().max(1) as f64
}

/// `probe tts <voice id> "<text>" <out.wav> [--threads n] [--steps n] [--assets dir]
/// [--rate hz]`, or `probe tts <voice id> --bench [--whisper-model id] [--whisper-device gpu]`.
/// `--rate` writes the WAV at that rate instead of the voice's own (change
/// `chatgpt-account-notice`: the old wake corpus reads 16 or 48 kHz).
pub fn run(args: &[String]) -> anyhow::Result<Outcome> {
    let rest = positional(
        args,
        &[
            "--threads",
            "--steps",
            "--assets",
            "--whisper-model",
            "--whisper-device",
            "--language",
            "--rate",
        ],
        &["--bench"],
    );
    let voice = rest
        .first()
        .ok_or_else(|| anyhow!("usage: probe tts <voice id> \"<text>\" <out.wav> [--threads n] | probe tts <voice id> --bench"))?
        .clone();
    let dir = assets_dir(args);
    let mut details = Vec::new();
    install(&voice, &dir, &mut details)?;
    let steps: i32 = flag(args, "--steps")
        .map(str::parse)
        .transpose()?
        .unwrap_or(5);
    if !args.iter().any(|arg| arg == "--bench") {
        let (Some(text), Some(out)) = (rest.get(1), rest.get(2)) else {
            bail!("usage: probe tts <voice id> \"<text>\" <out.wav>");
        };
        let threads: Option<i32> = flag(args, "--threads").map(str::parse).transpose()?;
        let rate: Option<u32> = flag(args, "--rate").map(str::parse).transpose()?;
        if rate.is_some_and(|rate| !(8_000..=192_000).contains(&rate)) {
            bail!("--rate takes 8000 to 192000 Hz");
        }
        let started = Instant::now();
        let mut synth = RealLoader {
            threads,
            supertonic_steps: steps,
        }
        .load(&voice, &dir)
        .map_err(|error| anyhow!("{error}"))?;
        details.push(format!(
            "{voice} loaded in {:.2} s",
            started.elapsed().as_secs_f64()
        ));
        let language = flag(args, "--language");
        let started = Instant::now();
        let audio = synth
            .synthesize(text, language, 1.0, &|| false)
            .map_err(|error| anyhow!("{error}"))?
            .unwrap_or_default();
        let seconds = started.elapsed().as_secs_f64();
        let length = audio.len() as f64 / f64::from(synth.sample_rate());
        let (written, written_rate) = match rate {
            Some(rate) if rate != synth.sample_rate() => (
                voicebro_core::audio::dsp::Resampler::convert(&audio, synth.sample_rate(), rate),
                rate,
            ),
            _ => (audio, synth.sample_rate()),
        };
        std::fs::write(out, wav_bytes(&written, written_rate))?;
        details.push(format!(
            "{length:.2} s of audio at {} Hz in {seconds:.3} s (RTF {:.3}); wrote {out} at {written_rate} Hz",
            synth.sample_rate(),
            seconds / length.max(1e-6)
        ));
        if let Some(path) = voicebro_core::tts::sherpa::loaded_onnxruntime() {
            details.push(format!("onnxruntime loaded from {}", path.display()));
        }
        return Ok(Outcome {
            pass: length > 0.3,
            details,
        });
    }

    // The bench: the sentences with 2, 4 and 8 threads, then Whisper reads the 4-thread WAVs.
    let listen = PathBuf::from("target").join("tts-listen");
    std::fs::create_dir_all(&listen)?;
    let safe = voice.replace([':', '\\', '/'], "_");
    let mut written = Vec::new();
    for threads in [2, 4, 8] {
        let started = Instant::now();
        let mut synth = RealLoader {
            threads: Some(threads),
            supertonic_steps: steps,
        }
        .load(&voice, &dir)
        .map_err(|error| anyhow!("{error}"))?;
        let load = started.elapsed().as_secs_f64();
        let warm = Instant::now();
        synth
            .synthesize("Hello.", Some("en"), 1.0, &|| false)
            .map_err(|error| anyhow!("{error}"))?;
        let warm = warm.elapsed().as_secs_f64();
        let (mut busy, mut length, mut first) = (0.0, 0.0, None);
        for (language, sentences) in languages(&voice) {
            for (index, sentence) in sentences.iter().enumerate() {
                let started = Instant::now();
                let audio = synth
                    .synthesize(sentence, Some(language), 1.0, &|| false)
                    .map_err(|error| anyhow!("{error}"))?
                    .unwrap_or_default();
                let took = started.elapsed().as_secs_f64();
                first.get_or_insert(took);
                busy += took;
                length += audio.len() as f64 / f64::from(synth.sample_rate());
                if threads == 4 {
                    let path = listen.join(format!("{safe}-{language}-{index}.wav"));
                    std::fs::write(&path, wav_bytes(&audio, synth.sample_rate()))?;
                    written.push((path, *sentence, language, audio, synth.sample_rate()));
                }
            }
        }
        details.push(format!(
            "{voice} threads {threads}: load {load:.2} s, warm-up {warm:.2} s, first sentence {:.0} ms, RTF {:.3} ({length:.1} s of audio in {busy:.2} s)",
            first.unwrap_or_default() * 1000.0,
            busy / length.max(1e-6)
        ));
    }
    // Intelligibility: Whisper (the most accurate model at hand) reads the 4-thread WAVs.
    let mut whisper_args = vec![
        "--model".to_owned(),
        flag(args, "--whisper-model")
            .unwrap_or("whisper-large-v3-turbo-q8_0")
            .to_owned(),
        "--device".to_owned(),
        flag(args, "--whisper-device").unwrap_or("gpu").to_owned(),
        "--assets".to_owned(),
        dir.display().to_string(),
    ];
    whisper_args.extend(["--threads".to_owned(), "8".to_owned()]);
    let (mut model, _, model_id) = crate::whisper::load(&whisper_args, &mut details)?;
    let mut scores = Vec::new();
    for (path, sentence, language, audio, rate) in &written {
        let clip = voicebro_core::audio::dsp::Resampler::convert(audio, *rate, 16_000);
        let raw = model
            .transcribe(
                &clip,
                &voicebro_core::stt::DecodeOptions {
                    language: (*language).to_owned(),
                    threads: 8,
                    prompt: None,
                },
            )
            .map_err(|error| anyhow!("{error}"))?;
        let heard: String = raw
            .segments
            .iter()
            .map(|segment| segment.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        let score = word_score(sentence, &heard);
        scores.push(score);
        details.push(format!(
            "{:.0} % {} «{heard}»",
            score * 100.0,
            path.file_name().unwrap().to_string_lossy()
        ));
    }
    let mean = scores.iter().sum::<f64>() / scores.len().max(1) as f64;
    details.push(format!(
        "{voice}: Whisper {model_id} read back {:.0} % of the words",
        mean * 100.0
    ));
    Ok(Outcome {
        pass: mean >= 0.8,
        details,
    })
}
