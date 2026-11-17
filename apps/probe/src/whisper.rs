//! Whisper probes of change `whisper-mode`: one WAV through the app's whisper.cpp runtime,
//! the timings of 2, 5 and 10 s clips, and the wake corpus through Whisper's phrase
//! spotting.
//!
//! The files come from an assets folder (`--assets`, `VOICEBRO_WHISPER_DIR`, or
//! `target\whisper-test\assets`) through the app's asset store: a file put into the folder
//! by hand is hash-checked and installed; a missing one is downloaded from its pinned URL.

use crate::Outcome;
use anyhow::{anyhow, bail, Context};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use voicebro_core::assets::{self, Asset};
use voicebro_core::settings::{SessionMode, WhisperDevice};
use voicebro_core::stt::catalog::{self, CPU_RUNTIME, GPU_RUNTIME, VAD_MODEL};
use voicebro_core::stt::phrases::{decide, prompt, Decision};
use voicebro_core::stt::transcriber::{clean, Cleaned};
use voicebro_core::stt::whisper::{self, WhisperModel, NO_SPEECH_THRESHOLD};
use voicebro_core::stt::{read_wav_16k, DecodeOptions, SpeechToText};
use voicebro_core::wake::PhraseSet;

/// The value after `name`, if given.
pub fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

/// `args` without the flags in `with_value` (and their values) and `alone`.
pub fn positional(args: &[String], with_value: &[&str], alone: &[&str]) -> Vec<String> {
    let mut rest = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if with_value.contains(&arg.as_str()) {
            skip = true;
        } else if !alone.contains(&arg.as_str()) {
            rest.push(arg.clone());
        }
    }
    rest
}

pub fn assets_dir(args: &[String]) -> PathBuf {
    flag(args, "--assets")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("VOICEBRO_WHISPER_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("target").join("whisper-test").join("assets"))
}

/// Installs `asset` into `dir`, printing what happened.
pub fn ensure(asset: &Asset, dir: &Path, details: &mut Vec<String>) -> anyhow::Result<()> {
    if assets::is_installed(dir, asset) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let by_hand = assets::archive_path(dir, asset).is_file();
    let started = Instant::now();
    assets::ensure(asset, dir, &|_| {}).map_err(|error| anyhow!("{}: {error}", asset.label))?;
    details.push(format!(
        "{} {} in {:.1} s",
        asset.label,
        if by_hand {
            "installed from the folder (hash checked)"
        } else {
            "downloaded and installed"
        },
        started.elapsed().as_secs_f64()
    ));
    Ok(())
}

/// The model's file: a catalog id (installed through the asset store) or a path.
fn model_file(id: &str, dir: &Path, details: &mut Vec<String>) -> anyhow::Result<PathBuf> {
    if let Some(entry) = catalog::find(id) {
        ensure(&entry.asset, dir, details)?;
        return Ok(dir.join(entry.asset.directory).join(entry.asset.marker));
    }
    let path = PathBuf::from(id);
    if path.is_file() {
        Ok(path)
    } else {
        bail!("{id} is neither a catalog model nor a file")
    }
}

fn device_arg(args: &[String]) -> anyhow::Result<WhisperDevice> {
    match flag(args, "--device").unwrap_or("cpu") {
        "cpu" => Ok(WhisperDevice::Cpu),
        "gpu" => Ok(WhisperDevice::Gpu),
        other => bail!("--device takes cpu or gpu, not {other}"),
    }
}

/// Prints the app's log lines at info and above (whisper.cpp's buffer sizes among them).
struct PrintLogger;

impl log::Log for PrintLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info && !metadata.target().starts_with("detail::")
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("  log: {}", record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: PrintLogger = PrintLogger;

/// Loads the runtime and the model as the app does; returns the model and its load time.
pub fn load(
    args: &[String],
    details: &mut Vec<String>,
) -> anyhow::Result<(WhisperModel, f64, String)> {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let dir = assets_dir(args);
    let device = device_arg(args)?;
    ensure(
        if device == WhisperDevice::Gpu {
            &GPU_RUNTIME
        } else {
            &CPU_RUNTIME
        },
        &dir,
        details,
    )?;
    ensure(&VAD_MODEL, &dir, details)?;
    let model_id = flag(args, "--model").unwrap_or(catalog::DEFAULT_MODEL);
    let model = model_file(model_id, &dir, details)?;
    let started = Instant::now();
    let runtime = whisper::runtime(&dir).map_err(|error| anyhow!("{error}"))?;
    let runtime_seconds = started.elapsed().as_secs_f64();
    let choice = whisper::choose_device(&runtime, device, &dir);
    if let Some(line) = choice.fallback_line() {
        details.push(line);
    }
    let started = Instant::now();
    let loaded = WhisperModel::load(
        Arc::clone(&runtime),
        &model,
        &dir.join(VAD_MODEL.directory).join(VAD_MODEL.marker),
        choice.used == WhisperDevice::Gpu,
    )
    .map_err(|error| anyhow!("{error}"))?;
    let load_seconds = started.elapsed().as_secs_f64();
    details.push(format!(
        "runtime whisper.cpp {} ({}) in {runtime_seconds:.2} s; {model_id} loaded on {} in {load_seconds:.2} s",
        runtime.version,
        runtime.folder.display(),
        choice.backend
    ));
    Ok((loaded, load_seconds, model_id.to_owned()))
}

fn options(args: &[String], prompt: Option<String>) -> anyhow::Result<DecodeOptions> {
    Ok(DecodeOptions {
        language: flag(args, "--language").unwrap_or("auto").to_owned(),
        threads: flag(args, "--threads")
            .map(str::parse)
            .transpose()
            .context("--threads takes a number")?
            .unwrap_or(0),
        prompt,
    })
}

/// `probe whisper <wav> [--model id|path] [--device cpu|gpu] [--language xx] [--threads n]
/// [--prompt "..."] [--bench] [--assets dir]`.
pub fn run(args: &[String]) -> anyhow::Result<Outcome> {
    let wav = positional(
        args,
        &[
            "--model",
            "--device",
            "--language",
            "--threads",
            "--prompt",
            "--assets",
        ],
        &["--bench"],
    )
    .into_iter()
    .next()
    .ok_or_else(|| anyhow!("usage: probe whisper <wav> [--model id] [--device cpu|gpu] [--language xx] [--threads n] [--bench]"))?;
    let audio = read_wav_16k(Path::new(&wav)).map_err(|error| anyhow!(error))?;
    let mut details = Vec::new();
    let (mut model, _, _) = load(args, &mut details)?;
    let options = options(args, flag(args, "--prompt").map(str::to_owned))?;
    let threads = if options.threads == 0 {
        voicebro_core::stt::auto_threads()
    } else {
        options.threads
    };
    let started = Instant::now();
    let raw = model
        .transcribe(&audio, &options)
        .map_err(|error| anyhow!("{error}"))?;
    let seconds = started.elapsed().as_secs_f64();
    details.push(format!(
        "{:.1} s of audio transcribed in {:.0} ms with {threads} threads; language {}",
        audio.len() as f64 / 16_000.0,
        seconds * 1000.0,
        raw.language.as_deref().unwrap_or("?")
    ));
    for segment in &raw.segments {
        details.push(format!(
            "segment (no speech {:.2}): {}",
            segment.no_speech_prob,
            segment.text.trim()
        ));
    }
    let text = match clean(&raw, NO_SPEECH_THRESHOLD, audio.len() as f64 / 16_000.0) {
        Cleaned::Text(text) => text,
        Cleaned::NoSpeech => String::new(),
        Cleaned::Hallucination { text, repeats } => {
            format!("(dropped as a hallucination: one word {repeats} times) {text}")
        }
    };
    details.push(format!("text: {text}"));
    if args.iter().any(|arg| arg == "--bench") {
        // The clip repeated to 2, 5 and 10 s; the median of three runs each.
        for length in [2.0, 5.0, 10.0] {
            let samples = (length * 16_000.0) as usize;
            let clip: Vec<f32> = audio.iter().copied().cycle().take(samples).collect();
            let mut runs = Vec::new();
            for _ in 0..3 {
                let started = Instant::now();
                model
                    .transcribe(&clip, &options)
                    .map_err(|error| anyhow!("{error}"))?;
                runs.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            runs.sort_by(f64::total_cmp);
            details.push(format!(
                "bench {length:>4.1} s clip: {:.0} ms (runs {:.0}, {:.0}, {:.0})",
                runs[1], runs[0], runs[1], runs[2]
            ));
        }
    }
    Ok(Outcome {
        pass: !text.is_empty(),
        details,
    })
}

/// The cutter's padding (design D3): 300 ms of silence after the speech, and at least 1 s
/// in all, so a corpus file reaches Whisper as an utterance of the app does.
fn pad_like_the_app(mut audio: Vec<f32>) -> Vec<f32> {
    use voicebro_core::stt::cutter::{MIN_CLIP_SECONDS, TRAILING_SILENCE_SECONDS};
    audio.extend(std::iter::repeat_n(
        0.0,
        (TRAILING_SILENCE_SECONDS * 16_000.0) as usize,
    ));
    let minimum = (MIN_CLIP_SECONDS * 16_000.0) as usize;
    if audio.len() < minimum {
        audio.resize(minimum, 0.0);
    }
    audio
}

/// What a corpus file must do: activate, deactivate, or nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Expect {
    Activate,
    Deactivate,
    Nothing,
}

/// The expected kind of a corpus file, from its name as `scripts\make-wake-corpus.ps1`
/// writes it and `scripts\run-wake-corpus.ps1` judges it: `goodbye` and `peace` files
/// deactivate; `plain`, `negative`, `brother` and `lookalike` files do nothing; the rest
/// activate.
fn expect_of(name: &str) -> Expect {
    let name = name.to_lowercase();
    if ["plain", "negative", "brother", "lookalike"]
        .iter()
        .any(|part| name.contains(part))
    {
        Expect::Nothing
    } else if ["goodbye", "peace"].iter().any(|part| name.contains(part)) {
        Expect::Deactivate
    } else {
        Expect::Activate
    }
}

/// Noise-only clips the probe makes (design D3): 3 s of silence, steady noise of peak 300,
/// and a game-like mix (a low hum, gunshot-like bursts and a rising tone).
fn noise_clips() -> Vec<(String, Vec<f32>)> {
    let samples = 3 * 16_000;
    let mut seed = 0x2545_f491_u32;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let steady: Vec<f32> = (0..samples).map(|_| random() * 300.0 / 32_768.0).collect();
    let game: Vec<f32> = (0..samples)
        .map(|index| {
            let t = index as f32 / 16_000.0;
            let hum = (t * 2.0 * std::f32::consts::PI * 70.0).sin() * 0.05;
            let burst = if (index / 4_000) % 3 == 0 && index % 4_000 < 800 {
                random() * 0.6 * (1.0 - (index % 4_000) as f32 / 800.0)
            } else {
                0.0
            };
            let sweep = (t * 2.0 * std::f32::consts::PI * (300.0 + 200.0 * t)).sin() * 0.03;
            hum + burst + sweep + random() * 0.01
        })
        .collect();
    vec![
        ("noise-silence".to_owned(), vec![0.0; samples]),
        ("noise-steady-300".to_owned(), steady),
        ("noise-game".to_owned(), game),
    ]
}

/// `probe whisper-corpus [--languages en,ru] [--no-prompt] [--model id] [--device cpu|gpu]
/// [--threads n] [--corpus dir]`: every WAV of `target\wake-corpus\<lang>\` with that
/// language's default phrases, plus the noise-only clips, one line each and a total.
pub fn corpus(args: &[String]) -> anyhow::Result<Outcome> {
    let root = flag(args, "--corpus")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("wake-corpus"));
    let languages: Vec<String> = match flag(args, "--languages") {
        Some(list) => list.split(',').map(str::to_owned).collect(),
        None => voicebro_core::wake::catalog::entries()
            .iter()
            .map(|entry| {
                entry
                    .language
                    .split('-')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect(),
    };
    let use_prompt = !args.iter().any(|arg| arg == "--no-prompt");
    let mut details = Vec::new();
    let (mut model, _, model_id) = load(args, &mut details)?;
    let (mut good, mut total, mut false_hits) = (0, 0, 0);
    let mut seconds = Vec::new();
    let mut every_phrase = Vec::new();
    for language in &languages {
        let Some(entry) = voicebro_core::wake::catalog::entry_for_language(language) else {
            details.push(format!("{language}: no catalog entry"));
            continue;
        };
        let owned = |list: &[&str]| {
            list.iter()
                .map(|text| (*text).to_owned())
                .collect::<Vec<_>>()
        };
        let phrases = PhraseSet::new(
            &owned(entry.default_activation),
            &owned(entry.default_deactivation),
        );
        every_phrase.extend(owned(entry.default_activation));
        every_phrase.extend(owned(entry.default_deactivation));
        let folder = root.join(language);
        let Ok(files) = std::fs::read_dir(&folder) else {
            details.push(format!("{language}: no folder {}", folder.display()));
            continue;
        };
        let mut files: Vec<PathBuf> = files
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
            // The Russian deactivation set has phrases of its own (`-Deactivation`).
            .filter(|path| !path.to_string_lossy().contains("-deact-"))
            .collect();
        files.sort();
        let options = options(args, use_prompt.then(|| prompt(&phrases)).flatten())?;
        let (mut language_good, mut language_total) = (0, 0);
        for file in files {
            let name = file.file_stem().unwrap().to_string_lossy().into_owned();
            let speech = read_wav_16k(&file).map_err(|error| anyhow!(error))?;
            let speech_seconds = speech.len() as f64 / 16_000.0;
            let audio = pad_like_the_app(speech);
            let started = Instant::now();
            let raw = model
                .transcribe(&audio, &options)
                .map_err(|error| anyhow!("{error}"))?;
            seconds.push(started.elapsed().as_secs_f64());
            let text = match clean(&raw, NO_SPEECH_THRESHOLD, speech_seconds) {
                Cleaned::Text(text) => text,
                _ => String::new(),
            };
            let expect = expect_of(&name);
            let outcome = match expect {
                Expect::Activate | Expect::Nothing => {
                    decide(&text, SessionMode::WakeWord, false, &phrases)
                }
                Expect::Deactivate => decide(&text, SessionMode::WakeWord, true, &phrases),
            };
            let ok = match (expect, &outcome) {
                (Expect::Activate, Decision::Activate { .. }) => true,
                (Expect::Deactivate, Decision::Deactivate { .. }) => true,
                (Expect::Nothing, Decision::Ignore) => true,
                (Expect::Nothing, _) => {
                    false_hits += 1;
                    false
                }
                _ => false,
            };
            language_total += 1;
            if ok {
                language_good += 1;
            }
            details.push(format!(
                "{} {language}/{name} [{expect:?}] {:.0} ms: «{text}» -> {}",
                if ok { "ok  " } else { "MISS" },
                seconds.last().unwrap() * 1000.0,
                match outcome {
                    Decision::Activate { phrase, question } =>
                        format!("activate «{phrase}», question {question:?}"),
                    Decision::Deactivate { phrase } => format!("deactivate «{phrase}»"),
                    Decision::Question(_) => "question".to_owned(),
                    Decision::Ignore => "nothing".to_owned(),
                }
            ));
        }
        details.push(format!("{language}: {language_good} of {language_total}"));
        good += language_good;
        total += language_total;
    }
    // The noise clips, with every language's phrases in the prompt: none may give a phrase.
    let all = PhraseSet::new(&every_phrase, &[]);
    let options = options(args, use_prompt.then(|| prompt(&all)).flatten())?;
    let mut noise_hits = 0;
    for (name, clip) in noise_clips() {
        let raw = model
            .transcribe(&clip, &options)
            .map_err(|error| anyhow!("{error}"))?;
        let cleaned = clean(&raw, NO_SPEECH_THRESHOLD, clip.len() as f64 / 16_000.0);
        let text = match &cleaned {
            Cleaned::Text(text) => text.clone(),
            _ => String::new(),
        };
        let decision = decide(&text, SessionMode::WakeWord, false, &all);
        let hit = decision != Decision::Ignore;
        if hit {
            noise_hits += 1;
        }
        details.push(format!(
            "{} {name}: {cleaned:?}",
            if hit { "HIT " } else { "ok  " }
        ));
    }
    seconds.sort_by(f64::total_cmp);
    let median = seconds.get(seconds.len() / 2).copied().unwrap_or_default();
    details.push(format!(
        "total {model_id} {}: {good} of {total}, false hits on speech files {false_hits}, noise clips giving a phrase {noise_hits} of 3, median {:.0} ms per file",
        if use_prompt { "with the prompt" } else { "without the prompt" },
        median * 1000.0
    ));
    Ok(Outcome {
        pass: total > 0 && noise_hits == 0,
        details,
    })
}
