//! The voice corpus test (change `voice-corpus-tests`): recorded speech in
//! `testdata/voice-corpus/`, played as Discord delivers a speaker through the app's Vosk wake
//! word and Whisper phrase spotting, with a report of whether and how each file activated
//! the assistant. Test code only.
//!
//! - D1: the corpus lives in the repository; `VOICEBRO_VOICE_CORPUS` points elsewhere.
//! - D3: each folder's `expect.txt` says what its files must do.
//! - D4: any common WAV is read and played at 48 kHz stereo.
//! - D5: a frame is sent while it is audible and for 5 frames after; 2 s of ticks follow.
//! - D6, D7: Vosk on the wake worker's path, Whisper on the cutter's.
//! - D8: the gate's rules of state, with its `REACTIVATION_GUARD`. A change of the gate's
//!   rules (`gate.rs`) must change [`Rules`].
//! - D9, D11: one gated test, `voice_corpus_when_enabled`, with one report and one verdict.

use crate::audio::dsp::Resampler;
use crate::audio::peak;
use crate::stt::cutter::{Cut, Cutter};
use crate::stt::phrases::{decide, Decision};
use crate::stt::transcriber::{clean, Cleaned};
use crate::wake::{PhraseKind, PhraseSet, REACTIVATION_GUARD};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The language this test covers (task scope: English only).
const LANGUAGE: &str = "en";
/// A 20 ms frame of 48 kHz stereo, interleaved.
const FRAME: usize = 1920;
/// The peak the app treats as audible when it decides to send toward Discord.
const AUDIBLE_PEAK: i32 = 256;
/// Frames sent after the last audible one, as at the end of speech.
const HANG_FRAMES: usize = 5;
/// Receive ticks after the file: Vosk's 1.5 s of trailing silence and the cutter's 700 ms.
const AFTER_TICKS: usize = 100;
/// The old corpus test's steady noise.
const NOISE_PEAK: i32 = 300;
const NOISE_SEED: u32 = 0x2545_f491;
/// 3 s more of noise after the file in the noise run.
const NOISE_TAIL_FRAMES: usize = 150;
const TICK_SECONDS: f64 = 0.02;

// ---------- D4: reading a WAV ----------

/// A file's samples, mono in -1..1, at its own rate.
#[derive(Debug, Clone, PartialEq)]
struct Wav {
    mono: Vec<f32>,
    rate: u32,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Integer PCM of 16 or 24 bits and float of 32 bits, `WAVE_FORMAT_EXTENSIBLE` included;
/// channels averaged to mono. Anything else is refused with its format.
fn read_wav(bytes: &[u8]) -> Result<Wav, String> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a RIFF WAVE file".to_owned());
    }
    let mut at = 12;
    let mut format = None;
    let mut data = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4).unwrap_or(0) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " => format = Some(&bytes[body..end]),
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        at = body.saturating_add(size).saturating_add(size % 2);
    }
    let format = format.ok_or("no fmt chunk")?;
    let tag = u16_at(format, 0).ok_or("a short fmt chunk")?;
    let channels = u16_at(format, 2).ok_or("a short fmt chunk")?;
    let rate = u32_at(format, 4).ok_or("a short fmt chunk")?;
    let bits = u16_at(format, 14).ok_or("a short fmt chunk")?;
    // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of the subformat GUID.
    let kind = if tag == 0xFFFE {
        u16_at(format, 24).ok_or("a short extensible fmt chunk")?
    } else {
        tag
    };
    let describe = || format!("format {tag:#06x}, {bits}-bit, {channels} channels");
    if channels == 0 || rate == 0 {
        return Err(describe());
    }
    let data = data.ok_or("no data chunk")?;
    let sample = |raw: &[u8]| -> Option<f32> {
        match (kind, bits) {
            (1, 16) => Some(f32::from(i16::from_le_bytes([raw[0], raw[1]])) / 32_768.0),
            (1, 24) => {
                let value = i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) >> 8;
                Some(value as f32 / 8_388_608.0)
            }
            (3, 32) => Some(f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
            _ => None,
        }
    };
    let width = usize::from(bits / 8);
    if sample(&[0; 4]).is_none() || width == 0 {
        return Err(format!(
            "{}: only 16-bit or 24-bit PCM and 32-bit float are read",
            describe()
        ));
    }
    let block = width * usize::from(channels);
    let mono = data
        .chunks_exact(block)
        .map(|frame| {
            let sum: f32 = frame
                .chunks_exact(width)
                .map(|raw| sample(raw).unwrap_or(0.0))
                .sum();
            sum / f32::from(channels)
        })
        .collect();
    Ok(Wav { mono, rate })
}

/// The file at 48 kHz as stereo frames of 20 ms; a last partial frame is padded with zeros.
fn frames_48k(wav: &Wav) -> Vec<Vec<i16>> {
    let audio = Resampler::convert(&wav.mono, wav.rate, 48_000);
    audio
        .chunks(FRAME / 2)
        .map(|chunk| {
            let mut frame: Vec<i16> = chunk
                .iter()
                .flat_map(|&value| {
                    let sample = (value * 32_767.0).clamp(-32_768.0, 32_767.0) as i16;
                    [sample, sample]
                })
                .collect();
            frame.resize(FRAME, 0);
            frame
        })
        .collect()
}

// ---------- D1, D3: the corpus tree and `expect.txt` ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Activate,
    Deactivate,
}

impl Change {
    fn name(self) -> &'static str {
        match self {
            Change::Activate => "activate",
            Change::Deactivate => "deactivate",
        }
    }

    fn of(kind: PhraseKind) -> Self {
        match kind {
            PhraseKind::Activate => Change::Activate,
            PhraseKind::Deactivate => Change::Deactivate,
        }
    }
}

fn changes_text(changes: &[Change]) -> String {
    if changes.is_empty() {
        "none".to_owned()
    } else {
        changes
            .iter()
            .map(|change| change.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// What a file must do: the state changes in order (none when empty), and the words.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Expect {
    changes: Vec<Change>,
    words: Option<String>,
}

impl Expect {
    /// A list that begins with a deactivation starts with a session active.
    fn starts_active(&self) -> bool {
        self.changes.first() == Some(&Change::Deactivate)
    }
}

/// The phrases a folder is checked against, as the settings fields write them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Lists {
    activation: Vec<String>,
    deactivation: Vec<String>,
}

impl Lists {
    fn parse(text: &str) -> Self {
        let (activation, deactivation) = text.split_once('|').unwrap_or((text, ""));
        let list = |part: &str| -> Vec<String> {
            part.split(',')
                .map(str::trim)
                .filter(|phrase| !phrase.is_empty())
                .map(str::to_owned)
                .collect()
        };
        Self {
            activation: list(activation),
            deactivation: list(deactivation),
        }
    }

    /// The language's default phrases: the catalog model's, which the Whisper mode's equal.
    fn defaults() -> Self {
        let entry = crate::wake::catalog::entry_for_language(LANGUAGE)
            .expect("the catalog has an English model");
        let own = |list: &[&str]| list.iter().map(|phrase| (*phrase).to_owned()).collect();
        Self {
            activation: own(entry.default_activation),
            deactivation: own(entry.default_deactivation),
        }
    }

    fn set(&self) -> PhraseSet {
        PhraseSet::new(&self.activation, &self.deactivation)
    }

    fn text(&self) -> String {
        format!(
            "{} | {}",
            self.activation.join(", "),
            self.deactivation.join(", ")
        )
    }
}

/// One file of the corpus.
#[derive(Debug, Clone)]
struct Case {
    /// The path below the corpus root, with `/`.
    rel: String,
    path: PathBuf,
    /// `None`: unlabeled, run and reported but not judged.
    expect: Option<Expect>,
    /// The folder whose phrases apply, for the Vosk word check.
    folder: String,
    phrases: Lists,
}

#[derive(Debug, Default)]
struct Corpus {
    cases: Vec<Case>,
    /// Broken lines and missing files, each a failure.
    errors: Vec<String>,
    /// Language folders this test does not cover.
    not_covered: Vec<String>,
}

/// One parsed `expect.txt`: its phrases, its file lines (with their line numbers), its errors.
#[derive(Debug, Default)]
struct ExpectFile {
    phrases: Option<Lists>,
    files: BTreeMap<String, (Expect, usize)>,
    errors: Vec<(usize, String)>,
}

fn parse_expect(text: &str) -> ExpectFile {
    let mut file = ExpectFile::default();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = line.split_once('=') else {
            file.errors
                .push((number, format!("«{line}» is no `name = ...` line")));
            continue;
        };
        let name = name.trim();
        if name == "phrases" {
            file.phrases = Some(Lists::parse(rest));
            continue;
        }
        let (changes, words) = match rest.split_once(';') {
            Some((changes, words)) => (changes, Some(words.trim().to_owned())),
            None => (rest, None),
        };
        let parts: Vec<String> = changes
            .split(',')
            .map(|part| part.trim().to_ascii_lowercase())
            .collect();
        let parsed: Result<Vec<Change>, String> = if parts.len() == 1 && parts[0] == "none" {
            Ok(Vec::new())
        } else {
            parts
                .iter()
                .map(|part| match part.as_str() {
                    "activate" => Ok(Change::Activate),
                    "deactivate" => Ok(Change::Deactivate),
                    other => Err(format!(
                        "«{other}» is not activate, deactivate or none (alone)"
                    )),
                })
                .collect()
        };
        match parsed {
            Ok(changes) if !name.is_empty() => {
                file.files.insert(
                    name.to_owned(),
                    (
                        Expect {
                            changes,
                            words: words.filter(|words| !words.is_empty()),
                        },
                        number,
                    ),
                );
            }
            Ok(_) => file
                .errors
                .push((number, "a line without a file name".to_owned())),
            Err(message) => file.errors.push((number, message)),
        }
    }
    file
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    entries.sort();
    entries
}

fn walk(dir: &Path, rel: &str, inherited: &Lists, corpus: &mut Corpus) {
    let expect_path = dir.join("expect.txt");
    let mut expect = match std::fs::read_to_string(&expect_path) {
        Ok(text) => parse_expect(&text),
        Err(_) => ExpectFile::default(),
    };
    for (line, message) in &expect.errors {
        corpus
            .errors
            .push(format!("{rel}/expect.txt line {line}: {message}"));
    }
    let phrases = expect.phrases.clone().unwrap_or_else(|| inherited.clone());
    let entries = sorted_entries(dir);
    for path in entries.iter().filter(|path| path.is_file() && is_wav(path)) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        corpus.cases.push(Case {
            rel: format!("{rel}/{name}"),
            path: path.clone(),
            expect: expect.files.remove(&name).map(|(expect, _)| expect),
            folder: rel.to_owned(),
            phrases: phrases.clone(),
        });
    }
    for (name, (_, line)) in &expect.files {
        corpus.errors.push(format!(
            "{rel}/expect.txt line {line}: {name} does not exist in {rel}"
        ));
    }
    for path in entries.iter().filter(|path| path.is_dir()) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        walk(path, &format!("{rel}/{name}"), &phrases, corpus);
    }
}

/// The corpus under `root`: the English folder walked, every other folder named as not
/// covered.
fn read_corpus(root: &Path) -> Corpus {
    let mut corpus = Corpus::default();
    for path in sorted_entries(root)
        .into_iter()
        .filter(|path| path.is_dir())
    {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == LANGUAGE {
            walk(&path, &name, &Lists::defaults(), &mut corpus);
        } else {
            corpus.not_covered.push(name);
        }
    }
    corpus
}

// ---------- D5: playing a file as Discord delivers a speaker ----------

/// One slot per receive tick: the frame sent in it, or nothing.
fn schedule(frames: &[Vec<i16>], noise: bool) -> Vec<Option<Vec<i16>>> {
    let mut slots: Vec<Option<Vec<i16>>> = if noise {
        let mut seed = NOISE_SEED;
        frames
            .iter()
            .cloned()
            .chain(std::iter::repeat_n(vec![0i16; FRAME], NOISE_TAIL_FRAMES))
            .map(|mut frame| {
                for pair in frame.as_chunks_mut::<2>().0 {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let value = (seed % (2 * NOISE_PEAK as u32 + 1)) as i32 - NOISE_PEAK;
                    for sample in pair {
                        *sample = (i32::from(*sample) + value).clamp(-32_768, 32_767) as i16;
                    }
                }
                Some(frame)
            })
            .collect()
    } else {
        let mut last_audible: Option<usize> = None;
        frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                if peak(frame) >= AUDIBLE_PEAK {
                    last_audible = Some(index);
                    Some(frame.clone())
                } else if last_audible.is_some_and(|last| index - last <= HANG_FRAMES) {
                    Some(frame.clone())
                } else {
                    None
                }
            })
            .collect()
    };
    slots.extend(std::iter::repeat_n(None, AFTER_TICKS));
    slots
}

// ---------- D8: the gate's rules of state ----------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Applied {
    Changed,
    Ignored(&'static str),
}

/// The gate's rules: activation only while waiting, deactivation only while active, no
/// activation within [`REACTIVATION_GUARD`] of a deactivation, in the file's time.
#[derive(Debug, Clone)]
struct Rules {
    active: bool,
    deactivated_at: Option<f64>,
}

impl Rules {
    fn new(active: bool) -> Self {
        Self {
            active,
            deactivated_at: None,
        }
    }

    fn apply(&mut self, change: Change, at: f64) -> Applied {
        match change {
            Change::Activate if self.active => Applied::Ignored("the session is already active"),
            Change::Deactivate if !self.active => Applied::Ignored("no session is active"),
            Change::Activate
                if self
                    .deactivated_at
                    .is_some_and(|then| at - then < REACTIVATION_GUARD.as_secs_f64()) =>
            {
                Applied::Ignored("within 1.5 s of a deactivation")
            }
            Change::Activate => {
                self.active = true;
                Applied::Changed
            }
            Change::Deactivate => {
                self.active = false;
                self.deactivated_at = Some(at);
                Applied::Changed
            }
        }
    }
}

// ---------- D6, D7: the runs ----------

/// What one run of one file through one engine gave.
#[derive(Debug, Default)]
struct RunResult {
    changes: Vec<Change>,
    detail: String,
    /// Whisper's transcription times in ms.
    times: Vec<f64>,
    error: Option<String>,
}

fn run_vosk(
    model: &dyn crate::wake::LoadedModel,
    slots: &[Option<Vec<i16>>],
    phrases: &PhraseSet,
    start_active: bool,
) -> RunResult {
    use crate::wake::spotter::PhraseSpotter;
    use crate::wake::worker::SpeakerState;
    let recognizer = match model.new_recognizer() {
        Ok(recognizer) => recognizer,
        Err(error) => {
            return RunResult {
                error: Some(format!("no recognizer: {error}")),
                ..RunResult::default()
            }
        }
    };
    let mut speaker = SpeakerState::new(recognizer, PhraseSpotter::new());
    let mut rules = Rules::new(start_active);
    let mut result = RunResult::default();
    let mut events = Vec::new();
    let mut heard = Vec::new();
    let mut near = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        let tick = index as u64 + 1;
        let mut hits = Vec::new();
        if let Some(frame) = slot {
            hits.extend(speaker.accept_discord(frame, phrases, tick, std::time::Instant::now()));
        }
        hits.extend(speaker.on_tick(tick, phrases));
        for (kind, phrase, since_end) in hits {
            let at = tick as f64 * TICK_SECONDS;
            let change = Change::of(kind);
            match rules.apply(change, at) {
                Applied::Changed => {
                    result.changes.push(change);
                    events.push(format!(
                        "{} «{phrase}» confirmed at {at:.2} s, {since_end:.2} s after the phrase ended",
                        change.name()
                    ));
                }
                Applied::Ignored(why) => events.push(format!(
                    "{} «{phrase}» at {at:.2} s ignored: {why}",
                    change.name()
                )),
            }
        }
        if let Some(text) = speaker.take_heard() {
            heard.push(text);
        }
        if let Some((phrase, partials)) = speaker.take_near_miss() {
            near.push(format!(
                "deactivation «{phrase}» seen in {partials} partials, never confirmed"
            ));
        }
    }
    let mut parts = events;
    parts.extend(near);
    parts.push(if heard.is_empty() {
        "heard nothing".to_owned()
    } else {
        format!(
            "heard {}",
            heard
                .iter()
                .map(|text| format!("\"{text}\""))
                .collect::<Vec<_>>()
                .join(" / ")
        )
    });
    result.detail = parts.join("; ");
    result
}

fn run_whisper(
    model: &mut dyn crate::stt::SpeechToText,
    options: &crate::stt::DecodeOptions,
    slots: &[Option<Vec<i16>>],
    phrases: &PhraseSet,
    start_active: bool,
) -> RunResult {
    let mut cutter = Cutter::new();
    let mut rules = Rules::new(start_active);
    let mut result = RunResult::default();
    let mut parts = Vec::new();
    let mut first: Option<u64> = None;
    let mut last = 0u64;
    let mut handle = |cut: Cut,
                      tick: u64,
                      first: &mut Option<u64>,
                      last: u64,
                      rules: &mut Rules,
                      result: &mut RunResult,
                      parts: &mut Vec<String>| {
        let start = (first.take().unwrap_or(tick).saturating_sub(1)) as f64 * TICK_SECONDS;
        let end = last as f64 * TICK_SECONDS;
        let span = format!("{start:.2}–{end:.2} s");
        match cut {
            Cut::Noise { voiced_frames, .. } => parts.push(format!(
                "{span}: dropped as noise ({voiced_frames} voiced frames)"
            )),
            Cut::Utterance(utterance) => {
                let started = std::time::Instant::now();
                let raw = match model.transcribe(&utterance.audio, options) {
                    Ok(raw) => raw,
                    Err(error) => {
                        result.error = Some(format!("{span}: transcription failed: {error}"));
                        return;
                    }
                };
                let ms = started.elapsed().as_secs_f64() * 1_000.0;
                result.times.push(ms);
                let delay = tick.saturating_sub(last) as f64 * TICK_SECONDS + ms / 1_000.0;
                let at = tick as f64 * TICK_SECONDS;
                let what = match clean(
                    &raw,
                    crate::stt::whisper::NO_SPEECH_THRESHOLD,
                    utterance.speech_seconds,
                ) {
                    Cleaned::Text(text) => {
                        let decision = decide(
                            &text,
                            crate::settings::SessionMode::WakeWord,
                            rules.active,
                            phrases,
                        );
                        let outcome = match decision {
                            Decision::Activate { phrase, question } => {
                                let question = question
                                    .map(|question| format!("question \"{question}\""))
                                    .unwrap_or_else(|| "no question".to_owned());
                                match rules.apply(Change::Activate, at) {
                                    Applied::Changed => {
                                        result.changes.push(Change::Activate);
                                        format!("activate «{phrase}», {question}")
                                    }
                                    Applied::Ignored(why) => {
                                        format!("activate «{phrase}» ignored: {why}")
                                    }
                                }
                            }
                            Decision::Deactivate { phrase } => {
                                match rules.apply(Change::Deactivate, at) {
                                    Applied::Changed => {
                                        result.changes.push(Change::Deactivate);
                                        format!("deactivate «{phrase}»")
                                    }
                                    Applied::Ignored(why) => {
                                        format!("deactivate «{phrase}» ignored: {why}")
                                    }
                                }
                            }
                            Decision::Question(question) => format!("question \"{question}\""),
                            Decision::Ignore => "nothing".to_owned(),
                        };
                        format!("\"{text}\" ({ms:.0} ms) -> {outcome}; {delay:.2} s after the speech")
                    }
                    Cleaned::NoSpeech => format!("no text: no speech ({ms:.0} ms)"),
                    Cleaned::Hallucination { text, repeats } => format!(
                        "no text: a hallucination, one word {repeats} times in \"{text}\" ({ms:.0} ms)"
                    ),
                };
                parts.push(format!("{span}: {what}"));
            }
        }
    };
    for (index, slot) in slots.iter().enumerate() {
        let tick = index as u64 + 1;
        if let Some(frame) = slot {
            if first.is_none() {
                first = Some(tick);
            }
            last = tick;
            if let Some(cut) = cutter.on_frame(1, 1, tick, std::time::Instant::now(), frame) {
                handle(
                    cut,
                    tick,
                    &mut first,
                    last,
                    &mut rules,
                    &mut result,
                    &mut parts,
                );
            }
        }
        for cut in cutter.on_tick(1, tick) {
            handle(
                cut,
                tick,
                &mut first,
                last,
                &mut rules,
                &mut result,
                &mut parts,
            );
        }
    }
    result.detail = if parts.is_empty() {
        "no utterance".to_owned()
    } else {
        parts.join("; ")
    };
    result
}

// ---------- D9, D11: the gated test and its report ----------

/// One engine and run's counts for the summary.
#[derive(Debug, Default)]
struct Tally {
    labeled: usize,
    right: usize,
    false_activations: usize,
    unlabeled: usize,
    errors: usize,
    times: Vec<f64>,
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(sorted[sorted.len() / 2])
}

/// The loaded Whisper model and its options' language.
struct WhisperSetup {
    model: crate::stt::whisper::WhisperModel,
    description: String,
}

/// Loads Whisper from `dir` without downloading: the runtime of the device, the VAD model and
/// the model must be installed; otherwise the error names each missing file.
fn load_whisper(dir: &Path) -> Result<WhisperSetup, String> {
    use crate::settings::WhisperDevice;
    use crate::stt::catalog;
    let device = match std::env::var("VOICEBRO_WHISPER_DEVICE").as_deref() {
        Ok("gpu") => WhisperDevice::Gpu,
        _ => WhisperDevice::Cpu,
    };
    let model_id = std::env::var("VOICEBRO_WHISPER_MODEL")
        .unwrap_or_else(|_| catalog::DEFAULT_MODEL.to_owned());
    let entry = catalog::find(&model_id)
        .ok_or_else(|| format!("VOICEBRO_WHISPER_MODEL «{model_id}» is not a catalog model"))?;
    let runtime_asset = catalog::runtime_for(device, dir);
    let missing: Vec<String> = [runtime_asset, catalog::VAD_MODEL, entry.asset]
        .iter()
        .filter(|asset| !crate::assets::is_installed(dir, asset))
        .map(|asset| {
            format!(
                "{} ({})",
                dir.join(asset.directory).join(asset.marker).display(),
                asset.label
            )
        })
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "the Whisper assets in {} miss {}",
            dir.display(),
            missing.join(", ")
        ));
    }
    let runtime = crate::stt::whisper::runtime(dir).map_err(|error| error.to_string())?;
    let choice = crate::stt::whisper::choose_device(&runtime, device, dir);
    let model = crate::stt::whisper::WhisperModel::load(
        std::sync::Arc::clone(&runtime),
        &dir.join(entry.asset.directory).join(entry.asset.marker),
        &dir.join(catalog::VAD_MODEL.directory)
            .join(catalog::VAD_MODEL.marker),
        choice.used == WhisperDevice::Gpu,
    )
    .map_err(|error| error.to_string())?;
    Ok(WhisperSetup {
        model,
        description: format!(
            "Whisper {model_id} on {}{}",
            choice.backend,
            choice
                .fallback_line()
                .map(|line| format!(" ({line})"))
                .unwrap_or_default()
        ),
    })
}

/// Loads the English catalog Vosk model from `dir` without downloading.
fn load_vosk(dir: &Path) -> Result<(Box<dyn crate::wake::LoadedModel>, String), String> {
    use crate::wake::WakeEngine;
    let entry = crate::wake::catalog::entry_for_language(LANGUAGE)
        .ok_or("the catalog has no English model")?;
    let missing: Vec<String> = [crate::wake::vosk::LIBRARY, entry.asset]
        .iter()
        .filter(|asset| !crate::assets::is_installed(dir, asset))
        .map(|asset| {
            format!(
                "{} ({})",
                dir.join(asset.directory).join(asset.marker).display(),
                asset.label
            )
        })
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "the Vosk assets in {} miss {}",
            dir.display(),
            missing.join(", ")
        ));
    }
    let model = crate::wake::vosk::VoskEngine
        .load(dir, &dir.join(entry.asset.directory))
        .map_err(|error| error.to_string())?;
    Ok((model, format!("Vosk {}", entry.id)))
}

/// Runs the corpus through Vosk when `VOICEBRO_VOSK_DIR` is set and Whisper when
/// `VOICEBRO_WHISPER_DIR` is set; with neither it returns at once. Every file is reported;
/// the test fails at the end on a wrong, missing or unreadable labeled file.
#[test]
fn voice_corpus_when_enabled() {
    let vosk_dir = std::env::var_os("VOICEBRO_VOSK_DIR").map(PathBuf::from);
    let whisper_dir = std::env::var_os("VOICEBRO_WHISPER_DIR").map(PathBuf::from);
    if vosk_dir.is_none() && whisper_dir.is_none() {
        return;
    }
    let root = std::env::var_os("VOICEBRO_VOICE_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("testdata")
                .join("voice-corpus")
        });
    let corpus = read_corpus(&root);
    let mut failures: Vec<String> = corpus.errors.clone();
    eprintln!(
        "== voice corpus: {} ({} files)",
        root.display(),
        corpus.cases.len()
    );
    for language in &corpus.not_covered {
        eprintln!("not covered: the language folder «{language}» (only {LANGUAGE} is run)");
    }
    for error in &corpus.errors {
        eprintln!("ERROR {error}");
    }

    let vosk = match &vosk_dir {
        None => {
            eprintln!("Vosk not run: VOICEBRO_VOSK_DIR is not set");
            None
        }
        Some(dir) => match load_vosk(dir) {
            Ok((model, description)) => {
                eprintln!("{description} from {}", dir.display());
                Some(model)
            }
            Err(error) => {
                eprintln!("ERROR {error}");
                failures.push(error);
                None
            }
        },
    };
    let mut whisper = match &whisper_dir {
        None => {
            eprintln!("Whisper not run: VOICEBRO_WHISPER_DIR is not set");
            None
        }
        Some(dir) => match load_whisper(dir) {
            Ok(setup) => {
                eprintln!("{} from {}", setup.description, dir.display());
                Some(setup)
            }
            Err(error) => {
                eprintln!("ERROR {error}");
                failures.push(error);
                None
            }
        },
    };

    // The words the Vosk model does not know, per folder: its labeled files are errors.
    let mut unknown: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(model) = &vosk {
        for case in &corpus.cases {
            if unknown.contains_key(&case.folder) {
                continue;
            }
            let set = case.phrases.set();
            let all: Vec<_> = set
                .activation
                .iter()
                .chain(&set.deactivation)
                .cloned()
                .collect();
            let (_, words) = crate::wake::worker::usable_phrases(model.as_ref(), &all);
            if !words.is_empty() {
                eprintln!(
                    "ERROR {}: the Vosk model does not know {} (phrases {})",
                    case.folder,
                    words
                        .iter()
                        .map(|word| format!("«{word}»"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    case.phrases.text()
                );
            }
            unknown.insert(case.folder.clone(), words);
        }
    }

    let mut tallies: BTreeMap<(&'static str, &'static str), Tally> = BTreeMap::new();
    for case in &corpus.cases {
        let want = case
            .expect
            .as_ref()
            .map(|expect| changes_text(&expect.changes))
            .unwrap_or_else(|| "unlabeled".to_owned());
        let said = case
            .expect
            .as_ref()
            .and_then(|expect| expect.words.as_ref())
            .map(|words| format!("  said: \"{words}\""))
            .unwrap_or_default();
        eprintln!("{}  want: {want}{said}", case.rel);
        let frames = match std::fs::read(&case.path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| read_wav(&bytes))
        {
            Ok(wav) => frames_48k(&wav),
            Err(error) => {
                eprintln!("  not run: {error}");
                if case.expect.is_some() {
                    failures.push(format!("{}: {error}", case.rel));
                }
                let engines = [("vosk", vosk.is_some()), ("whisper", whisper.is_some())];
                for engine in engines
                    .iter()
                    .filter(|(_, running)| *running)
                    .map(|(engine, _)| *engine)
                {
                    for run in ["clean", "noise"] {
                        let tally = tallies.entry((engine, run)).or_default();
                        if case.expect.is_some() {
                            tally.errors += 1;
                        } else {
                            tally.unlabeled += 1;
                        }
                    }
                }
                continue;
            }
        };
        let start_active = case.expect.as_ref().is_some_and(Expect::starts_active);
        let phrases = case.phrases.set();
        for (run, noise) in [("clean", false), ("noise", true)] {
            let slots = schedule(&frames, noise);
            let mut results: Vec<(&'static str, RunResult)> = Vec::new();
            if let Some(model) = &vosk {
                let words = unknown.get(&case.folder).cloned().unwrap_or_default();
                let result = if words.is_empty() {
                    run_vosk(model.as_ref(), &slots, &phrases, start_active)
                } else {
                    RunResult {
                        error: Some(format!(
                            "the model does not know {}",
                            words
                                .iter()
                                .map(|word| format!("«{word}»"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                        ..RunResult::default()
                    }
                };
                results.push(("vosk", result));
            }
            if let Some(setup) = whisper.as_mut() {
                let options = crate::stt::DecodeOptions {
                    language: LANGUAGE.to_owned(),
                    threads: 0,
                    prompt: crate::stt::phrases::prompt(&phrases),
                };
                results.push((
                    "whisper",
                    run_whisper(&mut setup.model, &options, &slots, &phrases, start_active),
                ));
            }
            for (engine, result) in results {
                let tally = tallies.entry((engine, run)).or_default();
                tally.times.extend(&result.times);
                let verdict = match (&case.expect, &result.error) {
                    (Some(_), Some(error)) => {
                        tally.errors += 1;
                        failures.push(format!("{} ({engine}, {run}): {error}", case.rel));
                        "ERROR"
                    }
                    (None, _) => {
                        tally.unlabeled += 1;
                        "--"
                    }
                    (Some(expect), None) => {
                        tally.labeled += 1;
                        if result.changes.contains(&Change::Activate)
                            && !expect.changes.contains(&Change::Activate)
                        {
                            tally.false_activations += 1;
                        }
                        if result.changes == expect.changes {
                            tally.right += 1;
                            "ok"
                        } else {
                            failures.push(format!(
                                "{} ({engine}, {run}): wanted {}, got {}",
                                case.rel,
                                changes_text(&expect.changes),
                                changes_text(&result.changes)
                            ));
                            "WRONG"
                        }
                    }
                };
                let detail = result
                    .error
                    .as_deref()
                    .map(|error| format!("error: {error}"))
                    .unwrap_or_else(|| result.detail.clone());
                eprintln!(
                    "  {engine:<8} {run:<6} {verdict:<6} {}: {detail}",
                    changes_text(&result.changes)
                );
            }
        }
    }

    eprintln!("== summary");
    for ((engine, run), tally) in &tallies {
        let time = median(&tally.times)
            .map(|ms| format!(", median transcription {ms:.0} ms"))
            .unwrap_or_default();
        eprintln!(
            "  {engine:<8} {run:<6} right {} of {} labeled, false activations {}, unlabeled {}, errors {}{time}",
            tally.right, tally.labeled, tally.false_activations, tally.unlabeled, tally.errors
        );
    }
    if failures.is_empty() {
        eprintln!("== every labeled file is right");
    } else {
        eprintln!("== {} failures:", failures.len());
        for failure in &failures {
            eprintln!("  {failure}");
        }
    }
    assert!(
        failures.is_empty(),
        "the voice corpus has {} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV of `tag` (with `sub` as the extensible subformat), `channels`, `rate`, `bits`
    /// and the raw sample bytes.
    fn wav_bytes(
        tag: u16,
        sub: Option<u16>,
        channels: u16,
        rate: u32,
        bits: u16,
        data: &[u8],
    ) -> Vec<u8> {
        let mut format = Vec::new();
        format.extend(tag.to_le_bytes());
        format.extend(channels.to_le_bytes());
        format.extend(rate.to_le_bytes());
        let block = channels * bits / 8;
        format.extend((rate * u32::from(block)).to_le_bytes());
        format.extend(block.to_le_bytes());
        format.extend(bits.to_le_bytes());
        if let Some(sub) = sub {
            format.extend(22u16.to_le_bytes());
            format.extend(bits.to_le_bytes());
            format.extend(0u32.to_le_bytes());
            format.extend(sub.to_le_bytes());
            format.extend([0u8; 14]);
        }
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(b"WAVE");
        bytes.extend(b"fmt ");
        bytes.extend((format.len() as u32).to_le_bytes());
        bytes.extend(&format);
        bytes.extend(b"data");
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
        bytes
    }

    #[test]
    fn common_wav_formats_are_read() {
        let pcm16: Vec<u8> = [16_384i16, -16_384]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let wav = read_wav(&wav_bytes(1, None, 1, 16_000, 16, &pcm16)).unwrap();
        assert_eq!(wav.rate, 16_000);
        assert_eq!(wav.mono, vec![0.5, -0.5]);

        // 24-bit stereo at 44.1 kHz: left half, right zero, averaged.
        let pcm24: Vec<u8> = [0x40_0000i32, 0]
            .iter()
            .flat_map(|s| s.to_le_bytes()[..3].to_vec())
            .collect();
        let wav = read_wav(&wav_bytes(1, None, 2, 44_100, 24, &pcm24)).unwrap();
        assert_eq!(wav.rate, 44_100);
        assert!((wav.mono[0] - 0.25).abs() < 1e-6, "{:?}", wav.mono);

        let float: Vec<u8> = [0.25f32, -1.0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let wav = read_wav(&wav_bytes(3, None, 1, 48_000, 32, &float)).unwrap();
        assert_eq!(wav.mono, vec![0.25, -1.0]);

        let wav = read_wav(&wav_bytes(0xFFFE, Some(1), 1, 16_000, 16, &pcm16)).unwrap();
        assert_eq!(wav.mono, vec![0.5, -0.5], "extensible PCM");
        let wav = read_wav(&wav_bytes(0xFFFE, Some(3), 1, 48_000, 32, &float)).unwrap();
        assert_eq!(wav.mono, vec![0.25, -1.0], "extensible float");
    }

    #[test]
    fn other_wavs_are_refused_with_their_format() {
        let error = read_wav(&wav_bytes(1, None, 1, 8_000, 8, &[128, 129])).unwrap_err();
        assert!(
            error.contains("8-bit") && error.contains("0x0001"),
            "{error}"
        );
        let mut no_data = wav_bytes(1, None, 1, 16_000, 16, &[]);
        no_data.truncate(no_data.len() - 8);
        assert_eq!(read_wav(&no_data).unwrap_err(), "no data chunk");
        assert!(read_wav(b"not a wav").is_err());
    }

    #[test]
    fn a_partial_last_frame_is_padded_at_48_khz_stereo() {
        let wav = Wav {
            mono: vec![0.5; 16_000 / 50 + 10],
            rate: 16_000,
        };
        let frames = frames_48k(&wav);
        assert!(frames.len() >= 2);
        assert!(frames.iter().all(|frame| frame.len() == FRAME));
        let last = frames.last().unwrap();
        assert_eq!(last[FRAME - 1], 0, "padded with zeros");
        assert_eq!(frames[0][100], frames[0][101], "both channels alike");
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn expect_lines_are_read_with_their_errors() {
        let file = parse_expect(
            "# a comment\n\
             phrases = hey bro, bro | goodbye\n\
             a.wav = activate ; Bro, what time is it?\n\
             b.wav = activate, deactivate\n\
             c.wav = none\n\
             d.wav = sometimes\n\
             nonsense\n",
        );
        assert_eq!(
            file.phrases,
            Some(Lists {
                activation: vec!["hey bro".to_owned(), "bro".to_owned()],
                deactivation: vec!["goodbye".to_owned()],
            })
        );
        assert_eq!(
            file.files["a.wav"].0,
            Expect {
                changes: vec![Change::Activate],
                words: Some("Bro, what time is it?".to_owned()),
            }
        );
        assert_eq!(
            file.files["b.wav"].0.changes,
            vec![Change::Activate, Change::Deactivate]
        );
        assert!(file.files["c.wav"].0.changes.is_empty());
        assert_eq!(file.errors.len(), 2);
        assert_eq!(file.errors[0].0, 6);
        assert_eq!(file.errors[1].0, 7);
    }

    #[test]
    fn the_corpus_tree_takes_phrases_down_and_reports_what_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("en/plain.wav"), "x");
        write(
            &root.join("en/mine/expect.txt"),
            "phrases = hey bro | goodbye\nmine.wav = activate\nghost.wav = none\n",
        );
        write(&root.join("en/mine/mine.wav"), "x");
        write(&root.join("en/mine/sub/deep.wav"), "x");
        write(&root.join("en/mine/local/private.wav"), "x");
        write(&root.join("de/hallo.wav"), "x");
        let corpus = read_corpus(root);
        let rels: Vec<&str> = corpus.cases.iter().map(|case| case.rel.as_str()).collect();
        assert_eq!(
            rels,
            vec![
                "en/plain.wav",
                "en/mine/mine.wav",
                "en/mine/local/private.wav",
                "en/mine/sub/deep.wav"
            ]
        );
        let case = |rel: &str| corpus.cases.iter().find(|case| case.rel == rel).unwrap();
        assert_eq!(case("en/plain.wav").expect, None, "unlabeled");
        assert_eq!(case("en/plain.wav").phrases, Lists::defaults());
        assert_eq!(Lists::defaults().activation, vec!["bro".to_owned()]);
        assert_eq!(Lists::defaults().deactivation, vec!["peace out".to_owned()]);
        assert_eq!(
            case("en/mine/mine.wav").expect.as_ref().unwrap().changes,
            vec![Change::Activate]
        );
        assert_eq!(
            case("en/mine/sub/deep.wav").phrases.deactivation,
            vec!["goodbye".to_owned()]
        );
        assert_eq!(
            case("en/mine/local/private.wav").phrases.activation,
            vec!["hey bro".to_owned()]
        );
        assert_eq!(corpus.not_covered, vec!["de".to_owned()]);
        assert_eq!(corpus.errors.len(), 1);
        assert!(
            corpus.errors[0].contains("line 3") && corpus.errors[0].contains("ghost.wav"),
            "{:?}",
            corpus.errors
        );
    }

    #[test]
    fn a_file_is_played_as_discord_sends_a_speaker() {
        let loud = vec![1_000i16; FRAME];
        let quiet = vec![0i16; FRAME];
        // 10 loud frames, a 1.5 s pause (75 frames), 10 loud frames.
        let mut frames = vec![loud.clone(); 10];
        frames.extend(vec![quiet.clone(); 75]);
        frames.extend(vec![loud.clone(); 10]);
        let slots = schedule(&frames, false);
        assert_eq!(slots.len(), frames.len() + AFTER_TICKS, "100 ticks follow");
        assert!(slots[..10].iter().all(Option::is_some));
        assert!(
            slots[10..15].iter().all(Option::is_some),
            "5 frames after the speech"
        );
        assert!(
            slots[15..85].iter().all(Option::is_none),
            "nothing during the pause"
        );
        assert!(slots[85..95].iter().all(Option::is_some));
        assert!(
            slots[95..].iter().all(Option::is_none),
            "nothing after the file but ticks"
        );

        let noisy = schedule(&frames, true);
        assert_eq!(noisy.len(), frames.len() + NOISE_TAIL_FRAMES + AFTER_TICKS);
        assert!(
            noisy[..frames.len() + NOISE_TAIL_FRAMES]
                .iter()
                .all(Option::is_some),
            "every frame of the noise run is sent, with 3 s more"
        );
        assert!(noisy[20]
            .as_ref()
            .is_some_and(|frame| peak(frame) <= NOISE_PEAK && peak(frame) > 0));
    }

    #[test]
    fn the_rules_of_state_are_the_gates() {
        let mut rules = Rules::new(false);
        assert_eq!(
            rules.apply(Change::Deactivate, 0.5),
            Applied::Ignored("no session is active")
        );
        assert_eq!(rules.apply(Change::Activate, 1.0), Applied::Changed);
        assert_eq!(
            rules.apply(Change::Activate, 1.2),
            Applied::Ignored("the session is already active")
        );
        assert_eq!(rules.apply(Change::Deactivate, 2.0), Applied::Changed);
        assert_eq!(
            rules.apply(Change::Activate, 3.0),
            Applied::Ignored("within 1.5 s of a deactivation")
        );
        assert_eq!(rules.apply(Change::Activate, 3.6), Applied::Changed);
        let starts = Expect {
            changes: vec![Change::Deactivate],
            words: None,
        };
        assert!(starts.starts_active());
        assert!(!Expect {
            changes: vec![],
            words: None
        }
        .starts_active());
    }
}
