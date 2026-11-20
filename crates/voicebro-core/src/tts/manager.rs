//! Loads the local voice for the Whisper mode and speaks the answers (change
//! `whisper-mode`, designs D7, D10 and D12).
//!
//! - `configure` checks which files the voice needs (the engine, the phonemizer for Piper
//!   and Kokoro, the voice) and publishes `NeedsDownload` without touching the network.
//! - The voice loads at a join, warmed up with one short synthesis, and unloads 0.5 s after
//!   the last leave.
//! - One thread (`voicebro-tts`) synthesizes the queued pieces in order, at most 20 s ahead
//!   of what has played; a piece of an older epoch is skipped, and a synthesis running when
//!   its epoch ended is stopped through the engine's progress callback.

use super::catalog::{self, VoiceEngine, ENGINE, PHONEMIZER};
use super::player::SpeechPlayer;
use super::sherpa::{Model, SherpaVoice};
use super::{Synth, TtsError};
use crate::assets::{self, Asset};
use crate::audio::dsp::Resampler;
use crate::events::{AppEvent, EventBus};
use crate::settings::Settings;
use crate::wake::{megabytes, DownloadItem};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The voice synthesizes at most this far ahead of what has played.
pub const AHEAD_SECONDS: f64 = 20.0;
/// The voice unloads this long after the bot left its last voice channel.
pub const UNLOAD_AFTER: Duration = Duration::from_millis(500);
/// CPU threads of a sherpa-onnx voice, per engine, from the measurement of task 6.7 of
/// change `whisper-mode`: Supertonic was fastest with 2 (more threads made it slower),
/// Piper is fast with any count, Kokoro gained up to 8.
pub fn threads_for(engine: VoiceEngine) -> i32 {
    match engine {
        VoiceEngine::Supertonic => 2,
        VoiceEngine::Kokoro => 8,
        VoiceEngine::Piper | VoiceEngine::Windows => 4,
    }
}
/// Supertonic's denoising steps (task 6.7).
pub const SUPERTONIC_STEPS: i32 = 5;
/// What the join refuses with while the voice's files are missing.
pub const JOIN_NEEDS_DOWNLOADS: &str = crate::stt::manager::JOIN_NEEDS_DOWNLOADS;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TtsState {
    Off,
    NeedsDownload { items: Vec<DownloadItem> },
    Downloading { label: String, percent: u8 },
    Installing { label: String },
    Idle,
    Loading,
    Ready,
    Error { message: String },
}

/// `Status.tts` (design D15).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TtsStatus {
    pub state: TtsState,
    pub voice: String,
    /// `supertonic`, `piper`, `kokoro` or `windows`.
    pub engine: String,
    pub sample_rate: Option<u32>,
    pub load_ms: Option<u64>,
    /// From a piece's queueing to its audio, for the first piece of the last answer.
    pub last_first_audio_ms: Option<u64>,
    /// Pieces synthesized, and those skipped or stopped by a flush.
    pub spoken: u64,
    pub skipped: u64,
    /// Pieces the voice could not read, and the last error.
    pub failed: u64,
    pub last_error: Option<String>,
    /// The selected voice's files that are not on disk, in either mode (change
    /// `whisper-config-layout`): the window's notice and its Download button.
    pub missing: Vec<DownloadItem>,
}

impl Default for TtsStatus {
    fn default() -> Self {
        Self {
            state: TtsState::Off,
            voice: catalog::DEFAULT_VOICE.to_owned(),
            engine: String::new(),
            sample_rate: None,
            load_ms: None,
            last_first_audio_ms: None,
            spoken: 0,
            skipped: 0,
            failed: 0,
            last_error: None,
            missing: Vec::new(),
        }
    }
}

/// A voice as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceInfo {
    pub id: String,
    pub display_name: String,
    pub language: String,
    pub engine: VoiceEngine,
    pub bytes: u64,
    pub installed: bool,
    pub data_license: String,
    pub needs_phonemizer: bool,
}

/// Loads a voice by id: the real engines, or a fake in tests.
pub trait Loader: Send + Sync {
    fn load(&self, voice: &str, assets_dir: &Path) -> Result<Box<dyn Synth>, TtsError>;
}

/// sherpa-onnx and the Windows voices.
pub struct RealLoader {
    /// `None`: the engine's own count ([`threads_for`]).
    pub threads: Option<i32>,
    pub supertonic_steps: i32,
}

impl Default for RealLoader {
    fn default() -> Self {
        Self {
            threads: None,
            supertonic_steps: SUPERTONIC_STEPS,
        }
    }
}

impl Loader for RealLoader {
    fn load(&self, voice: &str, assets_dir: &Path) -> Result<Box<dyn Synth>, TtsError> {
        if let Some(token) = catalog::windows_voice(voice) {
            #[cfg(windows)]
            return Ok(Box::new(super::windows::WindowsSynth::load(token)?));
            #[cfg(not(windows))]
            return Err(TtsError::Load(format!("no Windows voice {token} here")));
        }
        let entry = catalog::find(voice)
            .ok_or_else(|| TtsError::Load(format!("«{voice}» is not a voice")))?;
        let folder = assets_dir.join(entry.pack.asset.directory);
        let model = match entry.engine {
            VoiceEngine::Supertonic => Model::Supertonic {
                folder,
                num_steps: self.supertonic_steps,
            },
            VoiceEngine::Piper => Model::Piper {
                folder,
                model: entry.pack.asset.marker.to_owned(),
            },
            VoiceEngine::Kokoro => Model::Kokoro { folder },
            VoiceEngine::Windows => unreachable!("Windows voices are not in the catalog"),
        };
        let phonemizer = entry
            .pack
            .needs_phonemizer
            .then(|| assets_dir.join(PHONEMIZER.directory));
        Ok(Box::new(SherpaVoice::load(
            &assets_dir.join(ENGINE.directory),
            phonemizer.as_deref(),
            model,
            entry.speaker,
            self.threads.unwrap_or_else(|| threads_for(entry.engine)),
        )?))
    }
}

pub(crate) type Fetch =
    dyn Fn(&Asset, &Path, &dyn Fn(assets::Progress)) -> Result<(), String> + Send + Sync;

/// One piece to speak.
struct Job {
    epoch: u64,
    text: String,
    language: Option<String>,
    player: Arc<SpeechPlayer>,
    /// The first piece of a turn: the player reports its first frame.
    turn: Option<u64>,
    queued_at: Instant,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

struct Inner {
    loader: Arc<dyn Loader>,
    fetch: Arc<Fetch>,
    assets_dir: PathBuf,
    events: EventBus,
    status: Mutex<TtsStatus>,
    settings: Mutex<Option<Settings>>,
    consent: Mutex<Option<String>>,
    generation: AtomicU64,
    busy: Mutex<()>,
    wanted: AtomicBool,
    presence: AtomicU64,
    loaded: Mutex<Option<String>>,
    voice: Arc<Mutex<Option<Box<dyn Synth>>>>,
    speed: Arc<Mutex<f32>>,
    queue: Arc<(Mutex<Queue>, Condvar)>,
    thread: Mutex<bool>,
    spoken: Arc<AtomicU64>,
    skipped: Arc<AtomicU64>,
    first_audio_ms: Arc<AtomicU64>,
    failed: Arc<Mutex<(u64, Option<String>)>>,
}

/// See the module documentation.
#[derive(Clone)]
pub struct TtsManager {
    inner: Arc<Inner>,
}

impl TtsManager {
    pub fn new(assets_dir: PathBuf, events: EventBus) -> Self {
        Self::with_parts(
            Arc::new(RealLoader::default()),
            Arc::new(
                |asset: &Asset, dir: &Path, progress: &dyn Fn(assets::Progress)| {
                    assets::ensure(asset, dir, progress)
                },
            ),
            assets_dir,
            events,
        )
    }

    pub(crate) fn with_parts(
        loader: Arc<dyn Loader>,
        fetch: Arc<Fetch>,
        assets_dir: PathBuf,
        events: EventBus,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                loader,
                fetch,
                assets_dir,
                events,
                status: Mutex::new(TtsStatus::default()),
                settings: Mutex::new(None),
                consent: Mutex::new(None),
                generation: AtomicU64::new(0),
                busy: Mutex::new(()),
                wanted: AtomicBool::new(false),
                presence: AtomicU64::new(0),
                loaded: Mutex::new(None),
                voice: Arc::new(Mutex::new(None)),
                speed: Arc::new(Mutex::new(1.0)),
                queue: Arc::new((Mutex::new(Queue::default()), Condvar::new())),
                thread: Mutex::new(false),
                spoken: Arc::new(AtomicU64::new(0)),
                skipped: Arc::new(AtomicU64::new(0)),
                first_audio_ms: Arc::new(AtomicU64::new(u64::MAX)),
                failed: Arc::new(Mutex::new((0, None))),
            }),
        }
    }

    /// The catalog with what is downloaded, then the voices Windows has.
    pub fn voices(&self) -> Vec<VoiceInfo> {
        let dir = &self.inner.assets_dir;
        let catalog = catalog::voices().iter().map(|voice| VoiceInfo {
            id: voice.id.to_owned(),
            display_name: voice.display_name.to_owned(),
            language: voice.language.to_owned(),
            engine: voice.engine,
            bytes: catalog::assets_for(voice)
                .iter()
                .filter(|asset| !assets::is_installed(dir, asset))
                .map(|asset| asset.size)
                .sum(),
            installed: catalog::assets_for(voice)
                .iter()
                .all(|asset| assets::is_installed(dir, asset)),
            data_license: voice.pack.data_license.to_owned(),
            needs_phonemizer: voice.pack.needs_phonemizer,
        });
        let windows = super::windows::voices()
            .unwrap_or_else(|error| {
                log::warn!("could not list the Windows voices: {error}");
                Vec::new()
            })
            .into_iter()
            .map(|voice| VoiceInfo {
                id: voice.id,
                display_name: voice.display_name,
                language: voice.language,
                engine: VoiceEngine::Windows,
                bytes: 0,
                installed: true,
                data_license: "built into Windows".to_owned(),
                needs_phonemizer: false,
            });
        catalog.chain(windows).collect()
    }

    pub fn status(&self) -> TtsStatus {
        let mut status = self.inner.status.lock().expect("tts status lock").clone();
        status.spoken = self.inner.spoken.load(Ordering::Relaxed);
        status.skipped = self.inner.skipped.load(Ordering::Relaxed);
        let first = self.inner.first_audio_ms.load(Ordering::Relaxed);
        status.last_first_audio_ms = (first != u64::MAX).then_some(first);
        let (failed, last_error) = self.inner.failed.lock().expect("tts failed lock").clone();
        status.failed = failed;
        status.last_error = last_error;
        status
    }

    fn update(&self, change: impl FnOnce(&mut TtsStatus)) {
        {
            let mut status = self.inner.status.lock().expect("tts status lock");
            change(&mut status);
        }
        self.inner
            .events
            .publish(AppEvent::Tts(Box::new(self.status())));
    }

    fn set_state(&self, state: TtsState) {
        self.update(|status| status.state = state);
    }

    /// The files `voice` needs that are not on disk.
    fn missing(&self, voice: &str) -> Result<Vec<Asset>, String> {
        if catalog::windows_voice(voice).is_some() {
            return Ok(Vec::new());
        }
        let entry = catalog::find(voice).ok_or_else(|| format!("«{voice}» is not a voice"))?;
        Ok(catalog::assets_for(entry)
            .into_iter()
            .filter(|asset| !assets::is_installed(&self.inner.assets_dir, asset))
            .collect())
    }

    /// What choosing `voice` would download, for the install dialog.
    pub fn requirements(&self, voice: &str) -> Result<Vec<DownloadItem>, String> {
        Ok(self.missing(voice)?.iter().map(DownloadItem::of).collect())
    }

    fn engine_name(voice: &str) -> &'static str {
        if catalog::windows_voice(voice).is_some() {
            return "windows";
        }
        match catalog::find(voice).map(|entry| entry.engine) {
            Some(VoiceEngine::Supertonic) => "supertonic",
            Some(VoiceEngine::Piper) => "piper",
            Some(VoiceEngine::Kokoro) => "kokoro",
            _ => "",
        }
    }

    /// Follows the settings; returns at once.
    pub fn configure(&self, settings: &Settings) {
        *self.inner.settings.lock().expect("tts settings lock") = Some(settings.clone());
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *self.inner.speed.lock().expect("tts speed lock") = settings.tts.speed;
        let voice = settings.tts.voice.clone();
        let missing = self.missing(&voice);
        let items: Vec<DownloadItem> = missing
            .as_ref()
            .map(|missing| missing.iter().map(DownloadItem::of).collect())
            .unwrap_or_default();
        self.update(|status| {
            status.voice = voice.clone();
            status.engine = Self::engine_name(&voice).to_owned();
            status.missing = items;
        });
        let agreed = self.inner.consent.lock().expect("consent lock").as_deref() == Some(&voice);
        if !settings.assistant.mode.speaks_locally() {
            self.unload();
            // A download agreed in the realtime or text answers mode (changes
            // `whisper-config-layout`, `text-answers-mode`) fetches the files and loads
            // nothing.
            match missing {
                Ok(missing) if !missing.is_empty() && agreed => {
                    let manager = self.clone();
                    std::thread::Builder::new()
                        .name("voicebro-tts-load".to_owned())
                        .spawn(move || manager.background(generation, voice, missing))
                        .expect("spawn the voice loader thread");
                }
                _ => self.set_state(TtsState::Off),
            }
            return;
        }
        let missing = match missing {
            Ok(missing) => missing,
            Err(message) => {
                self.set_state(TtsState::Error { message });
                return;
            }
        };
        if !missing.is_empty() && !agreed {
            let items: Vec<DownloadItem> = missing.iter().map(DownloadItem::of).collect();
            self.inner.events.log(format!(
                "voice: {voice} needs a download first ({}); waiting for the OK in the window",
                items
                    .iter()
                    .map(|item| format!(
                        "{}, {} MB from {}",
                        item.label,
                        megabytes(item.bytes),
                        item.host
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            self.set_state(TtsState::NeedsDownload { items });
            return;
        }
        let loaded = self.inner.loaded.lock().expect("loaded lock").clone();
        let wanted = self.inner.wanted.load(Ordering::Acquire);
        if missing.is_empty() && (loaded.as_deref() == Some(&voice) || !wanted) {
            self.set_state(if loaded.as_deref() == Some(&voice) {
                TtsState::Ready
            } else {
                TtsState::Idle
            });
            return;
        }
        let manager = self.clone();
        std::thread::Builder::new()
            .name("voicebro-tts-load".to_owned())
            .spawn(move || manager.background(generation, voice, missing))
            .expect("spawn the voice loader thread");
    }

    /// The download and the load of a `configure`, unless a newer one came first. In the
    /// realtime mode it downloads only.
    fn background(&self, generation: u64, voice: String, missing: Vec<Asset>) {
        let _busy = self.inner.busy.lock().expect("tts busy lock");
        if self.inner.generation.load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(message) = self.fetch_all(&missing) {
            self.set_state(TtsState::Error { message });
            return;
        }
        let still = self.missing(&voice).unwrap_or_default();
        self.update(|status| status.missing = still.iter().map(DownloadItem::of).collect());
        let whisper_mode = self
            .inner
            .settings
            .lock()
            .expect("tts settings lock")
            .as_ref()
            .is_some_and(|settings| settings.assistant.mode.speaks_locally());
        if !whisper_mode {
            self.set_state(TtsState::Off);
            return;
        }
        if !self.inner.wanted.load(Ordering::Acquire) {
            self.set_state(TtsState::Idle);
            return;
        }
        if let Err(message) = self.load(&voice) {
            self.set_state(TtsState::Error { message });
        }
    }

    fn fetch_all(&self, missing: &[Asset]) -> Result<(), String> {
        for asset in missing {
            let label = asset.label.to_owned();
            self.inner
                .events
                .log(format!("downloading the {label} (first use only)"));
            let manager = self.clone();
            let events = self.inner.events.clone();
            (self.inner.fetch)(asset, &self.inner.assets_dir, &|step| match step {
                assets::Progress::Downloaded(percent) => {
                    manager.set_state(TtsState::Downloading {
                        label: label.clone(),
                        percent,
                    });
                    events.publish(AppEvent::Assets {
                        label: label.clone(),
                        percent,
                        error: None,
                    });
                }
                assets::Progress::Unpacking => manager.set_state(TtsState::Installing {
                    label: label.clone(),
                }),
            })
            .inspect_err(|error| {
                self.inner.events.publish(AppEvent::Assets {
                    label: label.clone(),
                    percent: 0,
                    error: Some(error.clone()),
                });
            })?;
            self.inner.events.log(format!("{label} installed"));
        }
        Ok(())
    }

    /// Loads `voice`, warms it up and swaps it in; the voice in use speaks until then.
    fn load(&self, voice: &str) -> Result<(), String> {
        self.set_state(TtsState::Loading);
        let started = Instant::now();
        let mut synth = self
            .inner
            .loader
            .load(voice, &self.inner.assets_dir)
            .map_err(|error| format!("the voice {voice} could not load: {error}"))?;
        let load_ms = started.elapsed().as_millis() as u64;
        let warm = Instant::now();
        let warm_up = synth.synthesize("Hello.", Some("en"), 1.0, &|| false);
        if let Err(error) = warm_up {
            return Err(format!("the voice {voice} could not speak: {error}"));
        }
        let sample_rate = synth.sample_rate();
        self.inner.events.log(format!(
            "voice {voice} loaded in {load_ms} ms, warmed up in {} ms ({sample_rate} Hz)",
            warm.elapsed().as_millis()
        ));
        *self.inner.voice.lock().expect("tts voice lock") = Some(synth);
        *self.inner.loaded.lock().expect("loaded lock") = Some(voice.to_owned());
        self.start_thread();
        self.update(|status| {
            status.state = TtsState::Ready;
            status.sample_rate = Some(sample_rate);
            status.load_ms = Some(load_ms);
        });
        Ok(())
    }

    fn start_thread(&self) {
        let mut running = self.inner.thread.lock().expect("tts thread lock");
        if *running {
            return;
        }
        self.inner.queue.0.lock().expect("tts queue lock").closed = false;
        let (queue, voice, speed, spoken, skipped, first, failed) = (
            Arc::clone(&self.inner.queue),
            Arc::clone(&self.inner.voice),
            Arc::clone(&self.inner.speed),
            Arc::clone(&self.inner.spoken),
            Arc::clone(&self.inner.skipped),
            Arc::clone(&self.inner.first_audio_ms),
            Arc::clone(&self.inner.failed),
        );
        std::thread::Builder::new()
            .name("voicebro-tts".to_owned())
            .spawn(move || run(&queue, &voice, &speed, &spoken, &skipped, &first, &failed))
            .expect("spawn the voice thread");
        *running = true;
    }

    fn unload(&self) {
        {
            let (lock, ready) = &*self.inner.queue;
            let mut queue = lock.lock().expect("tts queue lock");
            queue.closed = true;
            queue.jobs.clear();
            ready.notify_all();
        }
        *self.inner.thread.lock().expect("tts thread lock") = false;
        *self.inner.voice.lock().expect("tts voice lock") = None;
        if let Some(voice) = self.inner.loaded.lock().expect("loaded lock").take() {
            self.inner.events.log(format!("voice {voice} unloaded"));
        }
    }

    /// A `/join` in Whisper mode: loads the voice now, or refuses while files are missing.
    pub fn prepare_join(&self) -> Result<(), String> {
        self.inner.presence.fetch_add(1, Ordering::AcqRel);
        let settings = self
            .inner
            .settings
            .lock()
            .expect("tts settings lock")
            .clone()
            .ok_or_else(|| "the voice has not been configured yet".to_owned())?;
        // Only the Whisper mode speaks; the text answers mode needs no voice for a join
        // (change `text-answers-mode`, D2).
        if !settings.assistant.mode.speaks_locally() {
            return Ok(());
        }
        self.inner.wanted.store(true, Ordering::Release);
        let voice = settings.tts.voice.clone();
        let missing = self.missing(&voice)?;
        if !missing.is_empty() {
            if !matches!(
                self.status().state,
                TtsState::Downloading { .. } | TtsState::Installing { .. }
            ) {
                self.set_state(TtsState::NeedsDownload {
                    items: missing.iter().map(DownloadItem::of).collect(),
                });
            }
            return Err(JOIN_NEEDS_DOWNLOADS.to_owned());
        }
        let _busy = self.inner.busy.lock().expect("tts busy lock");
        if self.inner.loaded.lock().expect("loaded lock").as_deref() == Some(&voice) {
            return Ok(());
        }
        self.load(&voice).inspect_err(|message| {
            self.set_state(TtsState::Error {
                message: message.clone(),
            });
        })
    }

    /// The bot left its last voice channel: unloads after [`UNLOAD_AFTER`] unless it joins
    /// again first.
    pub fn release(&self) {
        self.inner.wanted.store(false, Ordering::Release);
        let presence = self.inner.presence.fetch_add(1, Ordering::AcqRel) + 1;
        let manager = self.clone();
        std::thread::Builder::new()
            .name("voicebro-tts-unload".to_owned())
            .spawn(move || {
                std::thread::sleep(UNLOAD_AFTER);
                let _busy = manager.inner.busy.lock().expect("tts busy lock");
                if manager.inner.presence.load(Ordering::Acquire) != presence
                    || manager.inner.wanted.load(Ordering::Acquire)
                {
                    return;
                }
                manager.unload();
                if matches!(manager.status().state, TtsState::Ready) {
                    manager.set_state(TtsState::Idle);
                }
            })
            .expect("spawn the voice unload thread");
    }

    /// The user agreed to download what `voice` needs.
    pub fn download(&self, voice: &str) -> Result<(), String> {
        let settings = self
            .inner
            .settings
            .lock()
            .expect("tts settings lock")
            .clone()
            .ok_or_else(|| "the voice has not been configured yet".to_owned())?;
        if settings.tts.voice != voice {
            return Err(format!(
                "{voice} is not the selected voice ({})",
                settings.tts.voice
            ));
        }
        // In either mode (change `whisper-config-layout`): whatever the selected voice
        // misses, or a retry after a failed download.
        let waiting = !self.missing(voice)?.is_empty();
        if !waiting && !matches!(self.status().state, TtsState::Error { .. }) {
            return Err("nothing is waiting for a voice download".to_owned());
        }
        self.inner
            .events
            .log(format!("voice: download agreed for {voice}"));
        *self.inner.consent.lock().expect("consent lock") = Some(voice.to_owned());
        self.configure(&settings);
        Ok(())
    }

    /// Queues a piece of `epoch` for `player`; `turn` marks the first piece of a turn.
    pub fn speak(
        &self,
        player: &Arc<SpeechPlayer>,
        epoch: u64,
        text: String,
        language: Option<String>,
        turn: Option<u64>,
    ) {
        let (lock, ready) = &*self.inner.queue;
        let mut queue = lock.lock().expect("tts queue lock");
        if queue.closed {
            return;
        }
        if turn.is_some() {
            self.inner.first_audio_ms.store(u64::MAX, Ordering::Relaxed);
        }
        queue.jobs.push_back(Job {
            epoch,
            text,
            language,
            player: Arc::clone(player),
            turn,
            queued_at: Instant::now(),
        });
        ready.notify_one();
    }

    /// Preview: `voice` reads its sample sentence at `speed` into WAV bytes, played only in
    /// the window. A voice of its own, loaded for the call and dropped after it.
    pub fn preview(&self, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        let missing = self.missing(voice)?;
        if !missing.is_empty() {
            return Err(format!(
                "{voice} needs a download first: {}",
                missing
                    .iter()
                    .map(|asset| asset.label)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let language = if let Some(token) = catalog::windows_voice(voice) {
            super::windows::voices()
                .ok()
                .and_then(|voices| {
                    voices
                        .into_iter()
                        .find(|found| found.id.ends_with(token))
                        .map(|found| found.language)
                })
                .unwrap_or_else(|| "en".to_owned())
        } else {
            let entry = catalog::find(voice).ok_or_else(|| format!("«{voice}» is not a voice"))?;
            if entry.language == "multi" {
                "en".to_owned()
            } else {
                entry.language.to_owned()
            }
        };
        let mut synth = self
            .inner
            .loader
            .load(voice, &self.inner.assets_dir)
            .map_err(|error| error.to_string())?;
        let short = language.split('-').next().unwrap_or("en").to_owned();
        let audio = synth
            .synthesize(
                super::preview_sentence(&language),
                Some(&short),
                speed,
                &|| false,
            )
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        Ok(super::wav_bytes(&audio, synth.sample_rate()))
    }
}

/// The voice thread: the queued pieces in order, until the queue closes.
fn run(
    queue: &(Mutex<Queue>, Condvar),
    voice: &Mutex<Option<Box<dyn Synth>>>,
    speed: &Mutex<f32>,
    spoken: &AtomicU64,
    skipped: &AtomicU64,
    first_audio_ms: &AtomicU64,
    failed: &Mutex<(u64, Option<String>)>,
) {
    loop {
        let job = {
            let (lock, ready) = queue;
            let mut state = lock.lock().expect("tts queue lock");
            loop {
                if state.closed {
                    return;
                }
                if let Some(job) = state.jobs.pop_front() {
                    break job;
                }
                state = ready.wait(state).expect("tts queue wait");
            }
        };
        let current = || job.player.epoch() == job.epoch;
        // Back-pressure: at most 20 s ahead of what has played.
        while current() && job.player.held_seconds() > AHEAD_SECONDS {
            std::thread::sleep(Duration::from_millis(50));
        }
        if !current() {
            skipped.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let speed = *speed.lock().expect("tts speed lock");
        let result = {
            let mut slot = voice.lock().expect("tts voice lock");
            let Some(synth) = slot.as_mut() else {
                continue;
            };
            let rate = synth.sample_rate();
            synth
                .synthesize(&job.text, job.language.as_deref(), speed, &|| !current())
                .map(|audio| audio.map(|audio| Resampler::convert(&audio, rate, 48_000)))
        };
        match result {
            Ok(Some(audio)) => {
                if let Some(turn) = job.turn {
                    job.player.start_turn(job.epoch, turn);
                    first_audio_ms.store(
                        job.queued_at.elapsed().as_millis() as u64,
                        Ordering::Relaxed,
                    );
                }
                if job.player.push(job.epoch, &audio) {
                    job.player.end_piece(job.epoch);
                    spoken.fetch_add(1, Ordering::Relaxed);
                } else {
                    skipped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Ok(None) => {
                skipped.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                log::warn!("the voice could not read a piece: {error}");
                let mut failed = failed.lock().expect("tts failed lock");
                failed.0 += 1;
                failed.1 = Some(error.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{Frame, FrameProvider};
    use crate::settings::AssistantMode;

    /// 0.5 s of a tone per piece at 24 kHz, taking `delay`; stops when asked.
    struct FakeSynth {
        delay: Duration,
        read: Arc<Mutex<Vec<String>>>,
    }

    impl Synth for FakeSynth {
        fn sample_rate(&self) -> u32 {
            24_000
        }

        fn synthesize(
            &mut self,
            text: &str,
            _: Option<&str>,
            _: f32,
            stop: &dyn Fn() -> bool,
        ) -> Result<Option<Vec<f32>>, TtsError> {
            let started = Instant::now();
            while started.elapsed() < self.delay {
                if stop() {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            self.read.lock().unwrap().push(text.to_owned());
            Ok(Some(
                (0..12_000)
                    .map(|index| (index as f32 * 0.1).sin() * 0.5)
                    .collect(),
            ))
        }
    }

    struct FakeLoader {
        delay: Duration,
        read: Arc<Mutex<Vec<String>>>,
    }

    impl Loader for FakeLoader {
        fn load(&self, _: &str, _: &Path) -> Result<Box<dyn Synth>, TtsError> {
            Ok(Box::new(FakeSynth {
                delay: self.delay,
                read: Arc::clone(&self.read),
            }))
        }
    }

    fn manager(delay: Duration) -> (TtsManager, Arc<Mutex<Vec<String>>>) {
        let read = Arc::new(Mutex::new(Vec::new()));
        let manager = TtsManager::with_parts(
            Arc::new(FakeLoader {
                delay,
                read: Arc::clone(&read),
            }),
            Arc::new(|_: &Asset, _: &Path, _: &dyn Fn(assets::Progress)| Ok(())),
            std::env::temp_dir(),
            EventBus::default(),
        );
        let mut settings = Settings::default();
        settings.assistant.mode = AssistantMode::Whisper;
        settings.tts.voice = "windows:fake".to_owned();
        manager.configure(&settings);
        manager.prepare_join().unwrap();
        (manager, read)
    }

    fn wait(condition: impl Fn() -> bool) {
        for _ in 0..400 {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out");
    }

    #[test]
    fn pieces_play_in_order_resampled_to_48_khz() {
        let (manager, read) = manager(Duration::ZERO);
        assert_eq!(manager.status().state, TtsState::Ready);
        assert_eq!(read.lock().unwrap().as_slice(), ["Hello."], "the warm-up");
        let player = SpeechPlayer::new();
        let epoch = player.epoch();
        for piece in ["One piece.", "Two pieces."] {
            manager.speak(&player, epoch, piece.to_owned(), None, Some(1));
        }
        wait(|| manager.status().spoken == 2);
        assert_eq!(&read.lock().unwrap()[1..], ["One piece.", "Two pieces."]);
        // Each piece ends padded to a whole 20 ms frame.
        assert!(
            (player.held_seconds() - 1.0).abs() < 0.05,
            "{}",
            player.held_seconds()
        );
        assert!(manager.status().last_first_audio_ms.is_some());
    }

    #[test]
    fn a_flush_during_synthesis_plays_nothing_more() {
        let (manager, _) = manager(Duration::from_millis(200));
        let player = SpeechPlayer::new();
        let epoch = player.epoch();
        for piece in ["First.", "Second.", "Third."] {
            manager.speak(&player, epoch, piece.to_owned(), None, None);
        }
        std::thread::sleep(Duration::from_millis(50));
        player.flush();
        wait(|| manager.status().skipped == 3);
        assert!(player.is_empty());
        assert_eq!(manager.status().spoken, 0);
    }

    #[test]
    fn a_long_answer_stays_within_20_seconds_ahead() {
        let (manager, _) = manager(Duration::ZERO);
        let player = SpeechPlayer::new();
        let epoch = player.epoch();
        // A two-minute answer: 240 pieces of 0.5 s.
        for index in 0..240 {
            manager.speak(&player, epoch, format!("Piece {index}."), None, None);
        }
        wait(|| player.held_seconds() >= AHEAD_SECONDS);
        std::thread::sleep(Duration::from_millis(200));
        assert!(player.held_seconds() <= AHEAD_SECONDS + 0.5);
        // Playing makes room for more.
        let mut frame = Frame::silence();
        for _ in 0..100 {
            player.next_frame(&mut frame);
        }
        wait(|| manager.status().spoken > 41);
        player.flush();
    }

    /// Change `whisper-config-layout`, tasks 1.2 and 1.3: in the realtime mode the status
    /// lists the voice's missing files, and Download fetches them and loads nothing.
    #[test]
    fn the_realtime_mode_lists_the_voice_files_and_downloads_without_loading() {
        let dir = tempfile::tempdir().unwrap();
        let read = Arc::new(Mutex::new(Vec::new()));
        let fetched = Arc::new(Mutex::new(Vec::new()));
        let fetch: Arc<Fetch> = {
            let fetched = Arc::clone(&fetched);
            Arc::new(
                move |asset: &Asset, dir: &Path, _: &dyn Fn(assets::Progress)| {
                    let folder = dir.join(asset.directory);
                    std::fs::create_dir_all(&folder).unwrap();
                    std::fs::write(folder.join(asset.marker), b"x").unwrap();
                    fetched.lock().unwrap().push(asset.label.to_owned());
                    Ok(())
                },
            )
        };
        let manager = TtsManager::with_parts(
            Arc::new(FakeLoader {
                delay: Duration::ZERO,
                read: Arc::clone(&read),
            }),
            fetch,
            dir.path().to_path_buf(),
            EventBus::default(),
        );
        let settings = Settings::default();
        manager.configure(&settings);
        let status = manager.status();
        assert_eq!(status.state, TtsState::Off);
        assert_eq!(status.missing.len(), 2, "the engine and Supertonic 3");
        assert!(manager.download("piper:ru_RU-denis-medium").is_err());
        manager.download(&settings.tts.voice).unwrap();
        wait(|| fetched.lock().unwrap().len() == 2 && manager.status().missing.is_empty());
        assert_eq!(manager.status().state, TtsState::Off);
        assert!(read.lock().unwrap().is_empty(), "no voice was loaded");
    }

    /// Change `text-answers-mode`, D2: in the text answers mode the voice stays off, its
    /// missing files are still listed, and a join neither needs nor loads it.
    #[test]
    fn the_text_answers_mode_needs_and_loads_no_voice() {
        let dir = tempfile::tempdir().unwrap();
        let read = Arc::new(Mutex::new(Vec::new()));
        let manager = TtsManager::with_parts(
            Arc::new(FakeLoader {
                delay: Duration::ZERO,
                read: Arc::clone(&read),
            }),
            Arc::new(|_: &Asset, _: &Path, _: &dyn Fn(assets::Progress)| Ok(())),
            dir.path().to_path_buf(),
            EventBus::default(),
        );
        let mut settings = Settings::default();
        settings.assistant.mode = AssistantMode::Text;
        manager.configure(&settings);
        assert_eq!(manager.status().state, TtsState::Off);
        assert_eq!(
            manager.status().missing.len(),
            2,
            "the engine and Supertonic 3"
        );
        assert_eq!(
            manager.prepare_join(),
            Ok(()),
            "the join does not need the voice"
        );
        assert_eq!(manager.status().state, TtsState::Off);
        assert!(read.lock().unwrap().is_empty(), "no voice was loaded");
    }
}
