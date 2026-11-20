//! Loads Whisper for the Whisper mode and feeds it (change `whisper-mode`, designs D2, D10,
//! D11, D12 and D15).
//!
//! - `configure` checks in the calling thread which files are missing (the runtime by
//!   device, the model, the VAD model) and publishes `NeedsDownload` without touching the
//!   network; the user's Download is the consent, held in memory for that model and device.
//! - The model loads at a join ([`WhisperManager::prepare_join`]) and unloads 0.5 s after
//!   the last leave ([`WhisperManager::release`]); between joins nothing of it is in memory.
//! - A change of model or device while loaded reloads in the background; the old model
//!   keeps serving until the new one is in. Language, threads and phrases apply to the next
//!   utterance.
//! - The gate's frames reach the cutter thread (`voicebro-whisper-cut`), whose utterances
//!   reach the transcriber thread (`voicebro-whisper`).

use super::catalog::{self, CPU_RUNTIME, GPU_RUNTIME, VAD_MODEL};
use super::cutter::{Cut, Cutter};
use super::transcriber::{self, Counters, CountersSnapshot, ModelSlot, TranscriptSink};
use super::whisper::{self, DeviceChoice, WhisperModel, NO_SPEECH_THRESHOLD};
use super::{local, DecodeOptions, SpeechToText, SttError};
use crate::assets::{self, Asset};
use crate::events::{AppEvent, EventBus};
use crate::settings::{Settings, WhisperDevice, WhisperSettings};
use crate::wake::{megabytes, DownloadItem, Phrase, PhraseSet, WakeFeed};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The reply to a `/join` in Whisper mode while files are missing (design D12; the places
/// since change `models-in-settings`). Its start, up to the colon, is what the window looks
/// for in the log.
pub const JOIN_NEEDS_DOWNLOADS: &str = "The Whisper mode needs downloads: open VoiceBro → Settings and press Download in the Whisper and Local voice cards, then run /join again";
/// The same reply in the text answers mode, which needs no voice (change
/// `text-answers-mode`); it starts as [`JOIN_NEEDS_DOWNLOADS`] does, for the window.
pub const JOIN_NEEDS_WHISPER_FILES: &str = "The Whisper mode needs downloads: open VoiceBro → Settings and press Download in the Whisper card, then run /join again";
/// Whisper unloads this long after the bot left its last voice channel.
pub const UNLOAD_AFTER: Duration = Duration::from_millis(500);
/// About 5 s of one speaker's frames toward the cutter.
const CUT_QUEUE: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WhisperState {
    /// Realtime mode: nothing of Whisper is used.
    Off,
    NeedsDownload {
        items: Vec<DownloadItem>,
    },
    Downloading {
        label: String,
        percent: u8,
    },
    Installing {
        label: String,
    },
    /// The files are on disk; the model loads at the next join.
    Idle,
    Loading,
    Ready,
    Error {
        message: String,
    },
}

/// `Status.whisper` (design D15).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WhisperStatus {
    pub state: WhisperState,
    pub model: String,
    pub device_requested: WhisperDevice,
    /// The device of the loaded model.
    pub device_used: Option<WhisperDevice>,
    /// Why the device in use is not the one asked for.
    pub fallback_reason: Option<String>,
    /// "CUDA0: NVIDIA GeForce RTX 5090 Laptop GPU" or "CPU ggml-cpu-alderlake, 8 threads".
    pub backend: Option<String>,
    pub language: String,
    pub load_ms: Option<u64>,
    pub activation_phrases: Vec<String>,
    pub deactivation_phrases: Vec<String>,
    pub counters: CountersSnapshot,
    /// The files of the selected model and device that are not on disk, in either mode
    /// (change `whisper-config-layout`): the window's notice and its Download button.
    pub missing: Vec<DownloadItem>,
}

impl Default for WhisperStatus {
    fn default() -> Self {
        Self {
            state: WhisperState::Off,
            model: catalog::DEFAULT_MODEL.to_owned(),
            device_requested: WhisperDevice::Cpu,
            device_used: None,
            fallback_reason: None,
            backend: None,
            language: "auto".to_owned(),
            load_ms: None,
            activation_phrases: Vec::new(),
            deactivation_phrases: Vec::new(),
            counters: CountersSnapshot::default(),
            missing: Vec::new(),
        }
    }
}

/// A Whisper model as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WhisperModelInfo {
    pub id: String,
    pub display_name: String,
    pub group: catalog::ModelGroup,
    pub note: String,
    pub license: String,
    pub bytes: u64,
    pub installed: bool,
}

/// What a load needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadRequest {
    pub assets_dir: PathBuf,
    pub model: PathBuf,
    pub vad: PathBuf,
    pub device: WhisperDevice,
}

/// A loaded model and the device it runs on.
pub struct Loaded {
    pub model: Box<dyn SpeechToText>,
    pub device: DeviceChoice,
}

/// Loads a model: whisper.cpp in the app, a fake in tests.
pub trait Engine: Send + Sync {
    fn load(&self, request: &LoadRequest) -> Result<Loaded, SttError>;

    /// Gives the GPU's memory back once no model is loaded or loading (change
    /// `whisper-config-layout`, design D9); `None` when there was nothing to give back.
    fn release_gpu(&self) -> Option<Result<(), String>> {
        None
    }
}

/// whisper.cpp through the process's runtime (design D8).
pub struct WhisperCpp;

impl Engine for WhisperCpp {
    fn load(&self, request: &LoadRequest) -> Result<Loaded, SttError> {
        let runtime = whisper::runtime(&request.assets_dir)?;
        let device = whisper::choose_device(&runtime, request.device, &request.assets_dir);
        let model = WhisperModel::load(
            runtime,
            &request.model,
            &request.vad,
            device.used == WhisperDevice::Gpu,
        )?;
        Ok(Loaded {
            model: Box::new(model),
            device,
        })
    }

    fn release_gpu(&self) -> Option<Result<(), String>> {
        whisper::loaded_runtime()?.reset_cuda()
    }
}

/// Downloads, verifies and installs one asset: [`assets::ensure`], replaced in tests.
type Fetch = dyn Fn(&Asset, &Path, &dyn Fn(assets::Progress)) -> Result<(), String> + Send + Sync;

/// What a loaded model was loaded for: the model id and the device asked for.
type Key = (String, WhisperDevice);

fn key_of(settings: &WhisperSettings) -> Key {
    (settings.model.clone(), settings.device)
}

enum CutMessage {
    Audio {
        guild: u64,
        ssrc: u32,
        tick: u64,
        at: Instant,
        samples: Vec<i16>,
    },
    Tick {
        guild: u64,
        tick: u64,
    },
    Clear(u64),
}

/// Adds the model and the device in use to each transcript.
struct LabeledSink {
    sink: Arc<dyn TranscriptSink>,
    label: Arc<Mutex<(String, String)>>,
}

impl TranscriptSink for LabeledSink {
    fn on_transcript(&self, mut transcript: transcriber::Transcript) {
        let (model, device) = self.label.lock().expect("label lock").clone();
        transcript.model = model;
        transcript.device = device;
        self.sink.on_transcript(transcript);
    }
}

/// The two threads of a loaded model.
struct Pipeline {
    sender: SyncSender<CutMessage>,
    queue: Arc<transcriber::UtteranceQueue>,
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // The cutter ends when its sender is gone; the transcriber when the queue closes.
        self.queue.close();
    }
}

struct Inner {
    engine: Arc<dyn Engine>,
    fetch: Arc<Fetch>,
    assets_dir: PathBuf,
    models_dir: PathBuf,
    local_models: Mutex<Vec<String>>,
    events: EventBus,
    sink: Arc<dyn TranscriptSink>,
    status: Mutex<WhisperStatus>,
    settings: Mutex<Option<Settings>>,
    /// The model and device whose missing files the user agreed to download.
    consent: Mutex<Option<Key>>,
    /// Counts `configure` calls: a background job that is not the newest gives way.
    generation: AtomicU64,
    /// One download or load at a time.
    busy: Mutex<()>,
    /// The bot is in a voice channel in Whisper mode.
    wanted: AtomicBool,
    /// Counts joins and leaves, so a pending unload sees a join that came after it.
    presence: AtomicU64,
    loaded: Mutex<Option<Key>>,
    slot: ModelSlot,
    options: Arc<Mutex<DecodeOptions>>,
    counters: Arc<Counters>,
    pipeline: Mutex<Option<Pipeline>>,
    dropped_frames: AtomicU64,
    /// The model and the device in use, for the transcripts.
    label: Arc<Mutex<(String, String)>>,
}

/// See the module documentation.
#[derive(Clone)]
pub struct WhisperManager {
    inner: Arc<Inner>,
}

impl WhisperManager {
    pub fn new(
        assets_dir: PathBuf,
        models_dir: PathBuf,
        events: EventBus,
        sink: Arc<dyn TranscriptSink>,
    ) -> Self {
        Self::with_parts(
            Arc::new(WhisperCpp),
            Arc::new(
                |asset: &Asset, dir: &Path, progress: &dyn Fn(assets::Progress)| {
                    assets::ensure(asset, dir, progress)
                },
            ),
            assets_dir,
            models_dir,
            events,
            sink,
        )
    }

    fn with_parts(
        engine: Arc<dyn Engine>,
        fetch: Arc<Fetch>,
        assets_dir: PathBuf,
        models_dir: PathBuf,
        events: EventBus,
        sink: Arc<dyn TranscriptSink>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                engine,
                fetch,
                assets_dir,
                models_dir,
                local_models: Mutex::new(Vec::new()),
                events,
                sink,
                status: Mutex::new(WhisperStatus::default()),
                settings: Mutex::new(None),
                consent: Mutex::new(None),
                generation: AtomicU64::new(0),
                busy: Mutex::new(()),
                wanted: AtomicBool::new(false),
                presence: AtomicU64::new(0),
                loaded: Mutex::new(None),
                slot: Arc::new(Mutex::new(None)),
                options: Arc::new(Mutex::new(DecodeOptions {
                    language: "auto".to_owned(),
                    threads: 0,
                    prompt: None,
                })),
                counters: Arc::new(Counters::default()),
                pipeline: Mutex::new(None),
                dropped_frames: AtomicU64::new(0),
                label: Arc::new(Mutex::new((String::new(), String::new()))),
            }),
        }
    }

    pub fn models_dir(&self) -> &Path {
        &self.inner.models_dir
    }

    /// Creates `whisper-models\` when it is missing and lists its models. Called at start;
    /// a model added later needs a restart.
    pub fn scan_local_models(&self) -> Vec<String> {
        if let Err(error) = std::fs::create_dir_all(&self.inner.models_dir) {
            log::warn!(
                "could not create the Whisper models folder {}: {error}",
                self.inner.models_dir.display()
            );
        }
        let models = local::scan(&self.inner.models_dir);
        *self.inner.local_models.lock().expect("local models lock") = models.clone();
        models
    }

    /// The catalog, with whether each model is downloaded, then the user's own models.
    pub fn models(&self) -> Vec<WhisperModelInfo> {
        let catalog = catalog::entries().iter().map(|entry| WhisperModelInfo {
            id: entry.id.to_owned(),
            display_name: entry.display_name.to_owned(),
            group: entry.group,
            note: entry.note.to_owned(),
            license: entry.license.to_owned(),
            bytes: entry.asset.size,
            installed: assets::is_installed(&self.inner.assets_dir, &entry.asset),
        });
        let local = self
            .inner
            .local_models
            .lock()
            .expect("local models lock")
            .clone()
            .into_iter()
            .map(|id| WhisperModelInfo {
                display_name: catalog::local_file(&id).unwrap_or_default().to_owned(),
                id,
                group: catalog::ModelGroup::Local,
                note: "your own model".to_owned(),
                license: String::new(),
                bytes: 0,
                installed: true,
            });
        catalog.chain(local).collect()
    }

    /// Whether `id` names a model: a catalog entry or a file of the last scan.
    pub fn knows_model(&self, id: &str) -> bool {
        catalog::find(id).is_some()
            || self
                .inner
                .local_models
                .lock()
                .expect("local models lock")
                .iter()
                .any(|local| local == id)
    }

    pub fn status(&self) -> WhisperStatus {
        let mut status = self
            .inner
            .status
            .lock()
            .expect("whisper status lock")
            .clone();
        status.counters = self.inner.counters.snapshot();
        status
    }

    fn update(&self, change: impl FnOnce(&mut WhisperStatus)) {
        let snapshot = {
            let mut status = self.inner.status.lock().expect("whisper status lock");
            change(&mut status);
            status.counters = self.inner.counters.snapshot();
            status.clone()
        };
        self.inner
            .events
            .publish(AppEvent::Whisper(Box::new(snapshot)));
    }

    fn set_state(&self, state: WhisperState) {
        self.update(|status| status.state = state);
    }

    /// The model file of `model` and the runtime of `device`, as paths.
    fn model_path(&self, model: &str) -> Result<PathBuf, String> {
        if let Some(entry) = catalog::find(model) {
            return Ok(self
                .inner
                .assets_dir
                .join(entry.asset.directory)
                .join(entry.asset.marker));
        }
        let path = local::path(&self.inner.models_dir, model)
            .ok_or_else(|| format!("«{model}» is not a Whisper model"))?;
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!(
                "the Whisper model {} is no longer in the models folder",
                path.display()
            ))
        }
    }

    /// The files `model` on `device` needs that are not on disk: the runtime (the CPU set,
    /// or the GPU set that also holds the CPU set), the model and the VAD model.
    fn missing(&self, model: &str, device: WhisperDevice) -> Result<Vec<Asset>, String> {
        self.model_path(model)?;
        let dir = &self.inner.assets_dir;
        let runtime = match device {
            WhisperDevice::Gpu => GPU_RUNTIME,
            WhisperDevice::Cpu if assets::is_installed(dir, &GPU_RUNTIME) => GPU_RUNTIME,
            WhisperDevice::Cpu => CPU_RUNTIME,
        };
        let model_asset = catalog::find(model).map(|entry| entry.asset);
        Ok([Some(runtime), model_asset, Some(VAD_MODEL)]
            .into_iter()
            .flatten()
            .filter(|asset| !assets::is_installed(dir, asset))
            .collect())
    }

    /// What choosing `model` on `device` would download, for the install dialog.
    pub fn requirements(
        &self,
        model: &str,
        device: WhisperDevice,
    ) -> Result<Vec<DownloadItem>, String> {
        Ok(self
            .missing(model, device)?
            .iter()
            .map(DownloadItem::of)
            .collect())
    }

    fn agreed(&self, key: &Key) -> bool {
        self.inner
            .consent
            .lock()
            .expect("consent lock")
            .as_ref()
            .is_some_and(|agreed| agreed == key)
    }

    fn apply_options(&self, whisper: &WhisperSettings) {
        let lists = whisper.phrases_in_effect();
        let phrases = PhraseSet::new(&lists.activation, &lists.deactivation);
        *self.inner.options.lock().expect("decode options lock") = DecodeOptions {
            language: whisper.language.clone(),
            threads: whisper.threads,
            prompt: super::phrases::prompt(&phrases),
        };
        self.update(|status| {
            status.model = whisper.model.clone();
            status.device_requested = whisper.device;
            status.language = whisper.language.clone();
            status.activation_phrases = Phrase::texts(&phrases.activation);
            status.deactivation_phrases = Phrase::texts(&phrases.deactivation);
        });
    }

    /// Follows the settings: off in realtime mode, the missing files reported, a consented
    /// download started, and a model change reloaded in the background while loaded.
    /// Returns at once.
    pub fn configure(&self, settings: &Settings) {
        *self.inner.settings.lock().expect("whisper settings lock") = Some(settings.clone());
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.apply_options(&settings.whisper);
        let key = key_of(&settings.whisper);
        let missing = self.missing(&key.0, key.1);
        let items: Vec<DownloadItem> = missing
            .as_ref()
            .map(|missing| missing.iter().map(DownloadItem::of).collect())
            .unwrap_or_default();
        self.update(|status| status.missing = items);
        if !settings.assistant.mode.hears_with_whisper() {
            // The CUDA context goes only when nothing else loads or downloads now; a busy
            // manager leaves it for the next unload.
            if self.unload() {
                if let Ok(_busy) = self.inner.busy.try_lock() {
                    self.release_gpu();
                }
            }
            // A download agreed in the realtime mode (change `whisper-config-layout`)
            // fetches the files and loads nothing.
            match missing {
                Ok(missing) if !missing.is_empty() && self.agreed(&key) => {
                    let manager = self.clone();
                    std::thread::Builder::new()
                        .name("voicebro-whisper-load".to_owned())
                        .spawn(move || manager.background(generation, key, missing))
                        .expect("spawn the Whisper loader thread");
                }
                _ => self.set_state(WhisperState::Off),
            }
            return;
        }
        let missing = match missing {
            Ok(missing) => missing,
            Err(message) => {
                self.set_state(WhisperState::Error { message });
                return;
            }
        };
        if !missing.is_empty() && !self.agreed(&key) {
            let items: Vec<DownloadItem> = missing.iter().map(DownloadItem::of).collect();
            self.inner.events.log(format!(
                "Whisper: {} needs a download first ({}); waiting for the OK in the window",
                key.0,
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
            self.set_state(WhisperState::NeedsDownload { items });
            return;
        }
        let loaded = self.inner.loaded.lock().expect("loaded lock").clone();
        let wanted = self.inner.wanted.load(Ordering::Acquire);
        if missing.is_empty() && (loaded.as_ref() == Some(&key) || !wanted) {
            let ready = loaded.as_ref() == Some(&key);
            self.set_state(if ready {
                WhisperState::Ready
            } else {
                WhisperState::Idle
            });
            return;
        }
        let manager = self.clone();
        std::thread::Builder::new()
            .name("voicebro-whisper-load".to_owned())
            .spawn(move || manager.background(generation, key, missing))
            .expect("spawn the Whisper loader thread");
    }

    /// The download and the reload of a `configure`, unless a newer one came first.
    fn background(&self, generation: u64, key: Key, missing: Vec<Asset>) {
        let _busy = self.inner.busy.lock().expect("whisper busy lock");
        if self.inner.generation.load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(message) = self.fetch_all(&missing) {
            self.set_state(WhisperState::Error { message });
            return;
        }
        let still = self.missing(&key.0, key.1).unwrap_or_default();
        self.update(|status| status.missing = still.iter().map(DownloadItem::of).collect());
        let whisper_mode = self
            .inner
            .settings
            .lock()
            .expect("whisper settings lock")
            .as_ref()
            .is_some_and(|settings| settings.assistant.mode.hears_with_whisper());
        if !whisper_mode {
            self.set_state(WhisperState::Off);
            return;
        }
        if !self.inner.wanted.load(Ordering::Acquire) {
            self.set_state(WhisperState::Idle);
            return;
        }
        if self.inner.generation.load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(message) = self.load(&key) {
            self.set_state(WhisperState::Error { message });
        }
    }

    fn fetch_all(&self, missing: &[Asset]) -> Result<(), String> {
        for asset in missing {
            let label = asset.label.to_owned();
            if assets::archive_path(&self.inner.assets_dir, asset).is_file() {
                self.inner.events.log(format!(
                    "using {} from the assets folder for the {label}",
                    asset.file_name()
                ));
            } else {
                self.inner
                    .events
                    .log(format!("downloading the {label} (first use only)"));
            }
            let manager = self.clone();
            let events = self.inner.events.clone();
            (self.inner.fetch)(asset, &self.inner.assets_dir, &|step| match step {
                assets::Progress::Downloaded(percent) => {
                    manager.set_state(WhisperState::Downloading {
                        label: label.clone(),
                        percent,
                    });
                    events.publish(AppEvent::Assets {
                        label: label.clone(),
                        percent,
                        error: None,
                    });
                }
                assets::Progress::Unpacking => manager.set_state(WhisperState::Installing {
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

    /// Loads `key` and swaps it in; the model in use keeps serving until then.
    fn load(&self, key: &Key) -> Result<(), String> {
        let request = LoadRequest {
            assets_dir: self.inner.assets_dir.clone(),
            model: self.model_path(&key.0)?,
            vad: self
                .inner
                .assets_dir
                .join(VAD_MODEL.directory)
                .join(VAD_MODEL.marker),
            device: key.1,
        };
        self.set_state(WhisperState::Loading);
        let started = Instant::now();
        let loaded = self
            .inner
            .engine
            .load(&request)
            .map_err(|error| format!("Whisper could not load {}: {error}", key.0))?;
        let load_ms = started.elapsed().as_millis() as u64;
        if let Some(line) = loaded.device.fallback_line() {
            self.inner.events.log(line);
        }
        let threads = match self
            .inner
            .options
            .lock()
            .expect("decode options lock")
            .threads
        {
            0 => super::auto_threads(),
            threads => threads,
        };
        let backend = if loaded.device.used == WhisperDevice::Cpu {
            format!("{}, {threads} threads", loaded.device.backend)
        } else {
            loaded.device.backend.clone()
        };
        self.inner.events.log(format!(
            "Whisper model {} loaded on {} in {load_ms} ms ({backend})",
            key.0,
            loaded.device.used.name()
        ));
        *self.inner.slot.lock().expect("whisper model lock") = Some(loaded.model);
        *self.inner.label.lock().expect("label lock") =
            (key.0.clone(), loaded.device.used.name().to_owned());
        *self.inner.loaded.lock().expect("loaded lock") = Some(key.clone());
        self.start_pipeline();
        let device = loaded.device;
        self.update(|status| {
            status.state = WhisperState::Ready;
            status.device_used = Some(device.used);
            status.fallback_reason = device.reason.clone();
            status.backend = Some(backend);
            status.load_ms = Some(load_ms);
        });
        Ok(())
    }

    fn start_pipeline(&self) {
        let mut pipeline = self.inner.pipeline.lock().expect("whisper pipeline lock");
        if pipeline.is_some() {
            return;
        }
        let queue = transcriber::UtteranceQueue::new();
        let (sender, receiver) = sync_channel(CUT_QUEUE);
        {
            let sink: Arc<dyn TranscriptSink> = Arc::new(LabeledSink {
                sink: Arc::clone(&self.inner.sink),
                label: Arc::clone(&self.inner.label),
            });
            let (queue, slot, options, counters) = (
                Arc::clone(&queue),
                Arc::clone(&self.inner.slot),
                Arc::clone(&self.inner.options),
                Arc::clone(&self.inner.counters),
            );
            std::thread::Builder::new()
                .name("voicebro-whisper".to_owned())
                .spawn(move || {
                    transcriber::run(queue, slot, options, counters, sink, NO_SPEECH_THRESHOLD)
                })
                .expect("spawn the Whisper thread");
        }
        {
            let (queue, counters) = (Arc::clone(&queue), Arc::clone(&self.inner.counters));
            std::thread::Builder::new()
                .name("voicebro-whisper-cut".to_owned())
                .spawn(move || cut(receiver, &queue, &counters))
                .expect("spawn the Whisper cutter thread");
        }
        *pipeline = Some(Pipeline { sender, queue });
    }

    /// Frees the model and ends both threads.
    /// Frees the model; true when it was on the GPU. Closing the pipeline first stops the
    /// utterance being transcribed (design D8 of change `whisper-config-layout`), so the
    /// model's lock is free soon.
    fn unload(&self) -> bool {
        let pipeline = self
            .inner
            .pipeline
            .lock()
            .expect("whisper pipeline lock")
            .take();
        drop(pipeline);
        let was_loaded = self.inner.loaded.lock().expect("loaded lock").take();
        *self.inner.slot.lock().expect("whisper model lock") = None;
        let on_gpu = was_loaded.is_some() && self.status().device_used == Some(WhisperDevice::Gpu);
        if let Some((model, _)) = was_loaded {
            self.inner
                .events
                .log(format!("Whisper model {model} unloaded"));
            self.update(|status| {
                status.device_used = None;
                status.fallback_reason = None;
                status.backend = None;
            });
        }
        on_gpu
    }

    /// After a model on the GPU was freed, with the busy lock held so nothing loads
    /// meanwhile: the CUDA context goes too (design D9 of change `whisper-config-layout`).
    fn release_gpu(&self) {
        match self.inner.engine.release_gpu() {
            Some(Ok(())) => self
                .inner
                .events
                .log("Whisper: the GPU's memory was given back (CUDA context reset)"),
            Some(Err(error)) => log::warn!("Whisper: the CUDA context was not reset: {error}"),
            None => {}
        }
    }

    /// A `/join` in Whisper mode: loads the model now, or refuses with
    /// [`JOIN_NEEDS_DOWNLOADS`] when files are missing. Blocks; the runtime runs it off the
    /// async threads.
    pub fn prepare_join(&self) -> Result<(), String> {
        self.inner.presence.fetch_add(1, Ordering::AcqRel);
        let settings = self
            .inner
            .settings
            .lock()
            .expect("whisper settings lock")
            .clone()
            .ok_or_else(|| "Whisper has not been configured yet".to_owned())?;
        if !settings.assistant.mode.hears_with_whisper() {
            return Ok(());
        }
        // Wanted from here: a download agreed later loads the model for this join.
        self.inner.wanted.store(true, Ordering::Release);
        let key = key_of(&settings.whisper);
        let missing = self.missing(&key.0, key.1)?;
        if !missing.is_empty() {
            if !matches!(
                self.status().state,
                WhisperState::Downloading { .. } | WhisperState::Installing { .. }
            ) {
                self.set_state(WhisperState::NeedsDownload {
                    items: missing.iter().map(DownloadItem::of).collect(),
                });
            }
            return Err(JOIN_NEEDS_DOWNLOADS.to_owned());
        }
        let _busy = self.inner.busy.lock().expect("whisper busy lock");
        if self.inner.loaded.lock().expect("loaded lock").as_ref() == Some(&key) {
            return Ok(());
        }
        self.load(&key).inspect_err(|message| {
            self.set_state(WhisperState::Error {
                message: message.clone(),
            });
        })
    }

    /// The bot left its last voice channel: unloads after [`UNLOAD_AFTER`] unless it
    /// joins again first.
    pub fn release(&self) {
        self.inner.wanted.store(false, Ordering::Release);
        let presence = self.inner.presence.fetch_add(1, Ordering::AcqRel) + 1;
        let manager = self.clone();
        std::thread::Builder::new()
            .name("voicebro-whisper-unload".to_owned())
            .spawn(move || {
                std::thread::sleep(UNLOAD_AFTER);
                let _busy = manager.inner.busy.lock().expect("whisper busy lock");
                if manager.inner.presence.load(Ordering::Acquire) != presence
                    || manager.inner.wanted.load(Ordering::Acquire)
                {
                    return;
                }
                if manager.unload() {
                    manager.release_gpu();
                }
                if matches!(manager.status().state, WhisperState::Ready) {
                    manager.set_state(WhisperState::Idle);
                }
            })
            .expect("spawn the Whisper unload thread");
    }

    /// The user agreed to download what the selected model needs. Refused unless `model`
    /// is the selected one and something waits for a download (or the last one failed).
    pub fn download(&self, model: &str) -> Result<(), String> {
        let settings = self
            .inner
            .settings
            .lock()
            .expect("whisper settings lock")
            .clone()
            .ok_or_else(|| "Whisper has not been configured yet".to_owned())?;
        if settings.whisper.model != model {
            return Err(format!(
                "{model} is not the selected Whisper model ({})",
                settings.whisper.model
            ));
        }
        let key = key_of(&settings.whisper);
        // In either mode (change `whisper-config-layout`): whatever the selected model and
        // device miss, or a retry after a failed download.
        let waiting = !self.missing(&key.0, key.1)?.is_empty();
        if !waiting && !matches!(self.status().state, WhisperState::Error { .. }) {
            return Err("nothing is waiting for a Whisper download".to_owned());
        }
        self.inner
            .events
            .log(format!("Whisper: download agreed for {}", key.0));
        *self.inner.consent.lock().expect("consent lock") = Some(key);
        self.configure(&settings);
        Ok(())
    }

    /// Drops what is cut or queued for `guild` (the bot left, or a mute ended the session).
    pub fn clear_guild(&self, guild: u64) {
        let pipeline = self.inner.pipeline.lock().expect("whisper pipeline lock");
        if let Some(pipeline) = pipeline.as_ref() {
            let _ = pipeline.sender.try_send(CutMessage::Clear(guild));
            pipeline.queue.clear_guild(guild);
        }
    }

    /// Frames toward the cutter dropped because it fell behind.
    pub fn dropped_frames(&self) -> u64 {
        self.inner.dropped_frames.load(Ordering::Relaxed)
    }

    fn send(&self, message: CutMessage) -> bool {
        let pipeline = self.inner.pipeline.lock().expect("whisper pipeline lock");
        !matches!(
            pipeline
                .as_ref()
                .map(|pipeline| pipeline.sender.try_send(message)),
            Some(Err(TrySendError::Full(_)))
        )
    }
}

impl WakeFeed for WhisperManager {
    fn feed(&self, guild: u64, ssrc: u32, tick: u64, frame: &[i16]) {
        let queued = self.send(CutMessage::Audio {
            guild,
            ssrc,
            tick,
            at: Instant::now(),
            samples: frame.to_vec(),
        });
        if !queued {
            self.inner.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn tick(&self, guild: u64, tick: u64) {
        self.send(CutMessage::Tick { guild, tick });
    }
}

/// The cutter thread: until the pipeline's sender is dropped.
fn cut(receiver: Receiver<CutMessage>, queue: &transcriber::UtteranceQueue, counters: &Counters) {
    let mut cutter = Cutter::new();
    let hand_over = |found: Cut| match found {
        Cut::Utterance(utterance) => transcriber::enqueue(queue, counters, utterance),
        Cut::Noise {
            guild,
            ssrc,
            voiced_frames,
        } => {
            counters.dropped_noise.fetch_add(1, Ordering::Relaxed);
            crate::detail!(
                "whisper",
                "[whisper:{guild}] ssrc {ssrc}: {voiced_frames} voiced frames are no speech"
            );
        }
    };
    while let Ok(message) = receiver.recv() {
        match message {
            CutMessage::Audio {
                guild,
                ssrc,
                tick,
                at,
                samples,
            } => {
                if let Some(found) = cutter.on_frame(guild, ssrc, tick, at, &samples) {
                    hand_over(found);
                }
            }
            CutMessage::Tick { guild, tick } => {
                for found in cutter.on_tick(guild, tick) {
                    hand_over(found);
                }
            }
            CutMessage::Clear(guild) => cutter.clear_guild(guild),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AssistantMode;
    use crate::stt::transcriber::Transcript;
    use crate::stt::{RawSegment, RawTranscript};

    /// Answers every clip with one sentence; loads take `delay`.
    struct FakeEngine {
        delay: Duration,
        loads: Mutex<Vec<LoadRequest>>,
        fallback: bool,
        /// Calls of `release_gpu`.
        resets: AtomicU64,
    }

    struct FakeModel(String);

    impl SpeechToText for FakeModel {
        fn transcribe(&mut self, _: &[f32], _: &DecodeOptions) -> Result<RawTranscript, SttError> {
            Ok(RawTranscript {
                segments: vec![RawSegment {
                    text: format!(" heard by {}", self.0),
                    no_speech_prob: 0.0,
                }],
                language: Some("en".to_owned()),
            })
        }
    }

    impl Engine for FakeEngine {
        fn load(&self, request: &LoadRequest) -> Result<Loaded, SttError> {
            std::thread::sleep(self.delay);
            self.loads.lock().unwrap().push(request.clone());
            let name = request
                .model
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let used = if self.fallback {
                WhisperDevice::Cpu
            } else {
                request.device
            };
            Ok(Loaded {
                model: Box::new(FakeModel(name)),
                device: DeviceChoice {
                    requested: request.device,
                    used,
                    reason: (used != request.device).then(|| "no CUDA device".to_owned()),
                    backend: "CPU fake".to_owned(),
                },
            })
        }

        fn release_gpu(&self) -> Option<Result<(), String>> {
            self.resets.fetch_add(1, Ordering::Relaxed);
            Some(Ok(()))
        }
    }

    #[derive(Default)]
    struct Collect(Mutex<Vec<Transcript>>);

    impl TranscriptSink for Collect {
        fn on_transcript(&self, transcript: Transcript) {
            self.0.lock().unwrap().push(transcript);
        }
    }

    struct Rig {
        manager: WhisperManager,
        engine: Arc<FakeEngine>,
        fetched: Arc<Mutex<Vec<String>>>,
        heard: Arc<Collect>,
        dir: PathBuf,
        events: EventBus,
    }

    fn rig(name: &str, delay: Duration, fallback: bool) -> Rig {
        let dir = std::env::temp_dir().join(format!(
            "voicebro-whisper-manager-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let engine = Arc::new(FakeEngine {
            delay,
            loads: Mutex::new(Vec::new()),
            fallback,
            resets: AtomicU64::new(0),
        });
        let fetched = Arc::new(Mutex::new(Vec::new()));
        let fetch: Arc<Fetch> = {
            let fetched = Arc::clone(&fetched);
            Arc::new(
                move |asset: &Asset, dir: &Path, progress: &dyn Fn(assets::Progress)| {
                    progress(assets::Progress::Downloaded(100));
                    let folder = dir.join(asset.directory);
                    std::fs::create_dir_all(&folder).unwrap();
                    std::fs::write(folder.join(asset.marker), b"x").unwrap();
                    fetched.lock().unwrap().push(asset.label.to_owned());
                    Ok(())
                },
            )
        };
        let heard = Arc::new(Collect::default());
        let events = EventBus::default();
        let manager = WhisperManager::with_parts(
            Arc::clone(&engine) as Arc<dyn Engine>,
            fetch,
            dir.join("assets"),
            dir.join("whisper-models"),
            events.clone(),
            Arc::clone(&heard) as Arc<dyn TranscriptSink>,
        );
        Rig {
            manager,
            engine,
            fetched,
            heard,
            dir,
            events,
        }
    }

    fn whisper_settings(model: &str) -> Settings {
        let mut settings = Settings::default();
        settings.assistant.mode = AssistantMode::Whisper;
        settings.whisper.model = model.to_owned();
        settings
    }

    fn wait_for(manager: &WhisperManager, wanted: impl Fn(&WhisperState) -> bool) {
        for _ in 0..300 {
            if wanted(&manager.status().state) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("state stayed {:?}", manager.status().state);
    }

    #[test]
    fn missing_files_wait_for_the_consent_then_download_without_loading() {
        let rig = rig("consent", Duration::ZERO, false);
        let settings = whisper_settings("whisper-base-q8_0");
        rig.manager.configure(&settings);
        let WhisperState::NeedsDownload { items } = rig.manager.status().state else {
            panic!("{:?}", rig.manager.status().state);
        };
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Whisper runtime (CPU)",
                "Whisper base (q8_0) model",
                "Silero voice activity model"
            ]
        );
        assert!(rig.fetched.lock().unwrap().is_empty(), "nothing fetched");
        assert_eq!(
            rig.manager.prepare_join(),
            Err(JOIN_NEEDS_DOWNLOADS.to_owned())
        );
        // The refused join left: the download agreed now loads nothing.
        rig.manager.release();
        assert!(rig.manager.download("whisper-small-q8_0").is_err());
        rig.manager.download("whisper-base-q8_0").unwrap();
        wait_for(&rig.manager, |state| *state == WhisperState::Idle);
        assert_eq!(rig.fetched.lock().unwrap().len(), 3);
        assert!(rig.engine.loads.lock().unwrap().is_empty(), "not joined");
        std::fs::remove_dir_all(&rig.dir).unwrap();
    }

    fn install_all(rig: &Rig, models: &[&str]) {
        let dir = rig.dir.join("assets");
        let mut wanted = vec![CPU_RUNTIME, VAD_MODEL];
        wanted.extend(models.iter().map(|id| catalog::find(id).unwrap().asset));
        for asset in wanted {
            std::fs::create_dir_all(dir.join(asset.directory)).unwrap();
            std::fs::write(dir.join(asset.directory).join(asset.marker), b"x").unwrap();
        }
    }

    #[test]
    fn the_model_loads_at_a_join_and_unloads_after_the_leave() {
        let rig = rig("join", Duration::ZERO, false);
        install_all(&rig, &["whisper-base-q8_0"]);
        rig.manager
            .configure(&whisper_settings("whisper-base-q8_0"));
        assert_eq!(rig.manager.status().state, WhisperState::Idle);
        rig.manager.prepare_join().unwrap();
        assert_eq!(rig.manager.status().state, WhisperState::Ready);
        assert_eq!(rig.engine.loads.lock().unwrap().len(), 1);

        // A frame per tick for 0.5 s, then silence: one utterance reaches the sink.
        let voiced = vec![3_000i16; 1920];
        for tick in 1..=25 {
            rig.manager.feed(1, 7, tick, &voiced);
            rig.manager.tick(1, tick);
        }
        for tick in 26..=70 {
            rig.manager.tick(1, tick);
        }
        for _ in 0..200 {
            if !rig.heard.0.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            rig.heard.0.lock().unwrap()[0].text,
            "heard by ggml-base-q8_0.bin"
        );

        rig.manager.release();
        rig.manager.prepare_join().unwrap();
        std::thread::sleep(UNLOAD_AFTER + Duration::from_millis(200));
        assert_eq!(
            rig.manager.status().state,
            WhisperState::Ready,
            "a join within 0.5 s keeps it"
        );
        rig.manager.release();
        std::thread::sleep(UNLOAD_AFTER + Duration::from_millis(200));
        assert_eq!(rig.manager.status().state, WhisperState::Idle);
        assert!(rig.manager.inner.slot.lock().unwrap().is_none());
        assert_eq!(
            rig.engine.resets.load(Ordering::Relaxed),
            0,
            "a model on the CPU leaves the GPU alone"
        );
        std::fs::remove_dir_all(&rig.dir).unwrap();
    }

    /// Design D9 of change `whisper-config-layout`: once a model on the GPU is freed, after
    /// the last leave or at a switch to the realtime mode, the CUDA context goes too.
    #[test]
    fn the_gpu_is_given_back_after_a_model_on_it_unloads() {
        let rig = rig("gpu-reset", Duration::ZERO, false);
        install_all(&rig, &["whisper-base-q8_0"]);
        let assets = rig.dir.join("assets").join(GPU_RUNTIME.directory);
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join(GPU_RUNTIME.marker), b"x").unwrap();
        let mut log = rig.events.subscribe();
        let mut settings = whisper_settings("whisper-base-q8_0");
        settings.whisper.device = WhisperDevice::Gpu;
        rig.manager.configure(&settings);
        rig.manager.prepare_join().unwrap();
        assert_eq!(rig.manager.status().device_used, Some(WhisperDevice::Gpu));
        rig.manager.release();
        std::thread::sleep(UNLOAD_AFTER + Duration::from_millis(200));
        assert_eq!(rig.engine.resets.load(Ordering::Relaxed), 1);
        let mut lines = Vec::new();
        while let Ok(event) = log.try_recv() {
            if let AppEvent::Log { line } = event {
                lines.push(line);
            }
        }
        assert!(
            lines.contains(
                &"Whisper: the GPU's memory was given back (CUDA context reset)".to_owned()
            ),
            "{lines:?}"
        );

        rig.manager.prepare_join().unwrap();
        settings.assistant.mode = AssistantMode::Realtime;
        rig.manager.configure(&settings);
        assert_eq!(
            rig.engine.resets.load(Ordering::Relaxed),
            2,
            "at the switch"
        );
        std::fs::remove_dir_all(&rig.dir).unwrap();
    }

    #[test]
    fn a_model_change_reloads_in_the_background_and_the_newest_settings_win() {
        let rig = rig("reload", Duration::from_millis(150), false);
        install_all(
            &rig,
            &[
                "whisper-base-q8_0",
                "whisper-small-q8_0",
                "whisper-tiny-q8_0",
            ],
        );
        rig.manager
            .configure(&whisper_settings("whisper-base-q8_0"));
        rig.manager.prepare_join().unwrap();
        rig.manager
            .configure(&whisper_settings("whisper-small-q8_0"));
        // A newer configure during that load: only it is applied.
        std::thread::sleep(Duration::from_millis(20));
        rig.manager
            .configure(&whisper_settings("whisper-tiny-q8_0"));
        assert!(
            rig.manager.inner.slot.lock().unwrap().is_some(),
            "the old model serves during the reload"
        );
        for _ in 0..200 {
            if rig
                .manager
                .inner
                .loaded
                .lock()
                .unwrap()
                .as_ref()
                .map(|key| key.0.as_str())
                == Some("whisper-tiny-q8_0")
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let loads: Vec<String> = rig
            .engine
            .loads
            .lock()
            .unwrap()
            .iter()
            .map(|load| {
                load.model
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            loads.first().map(String::as_str),
            Some("ggml-base-q8_0.bin")
        );
        assert_eq!(loads.last().map(String::as_str), Some("ggml-tiny-q8_0.bin"));
        assert!(loads.len() <= 3);
        assert_eq!(rig.manager.status().state, WhisperState::Ready);
        rig.manager.configure(&Settings::default());
        assert_eq!(rig.manager.status().state, WhisperState::Off);
        assert!(
            rig.manager.inner.slot.lock().unwrap().is_none(),
            "realtime unloads"
        );
        std::fs::remove_dir_all(&rig.dir).unwrap();
    }

    #[test]
    fn a_gpu_without_cuda_falls_back_to_the_cpu_and_says_so() {
        let rig = rig("fallback", Duration::ZERO, true);
        install_all(&rig, &["whisper-base-q8_0"]);
        let dir = rig.dir.join("assets").join(GPU_RUNTIME.directory);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(GPU_RUNTIME.marker), b"x").unwrap();
        let mut log = rig.events.subscribe();
        let mut settings = whisper_settings("whisper-base-q8_0");
        settings.whisper.device = WhisperDevice::Gpu;
        rig.manager.configure(&settings);
        rig.manager.prepare_join().unwrap();
        let status = rig.manager.status();
        assert_eq!(status.device_used, Some(WhisperDevice::Cpu));
        assert_eq!(status.fallback_reason.as_deref(), Some("no CUDA device"));
        let mut lines = Vec::new();
        while let Ok(event) = log.try_recv() {
            if let AppEvent::Log { line } = event {
                lines.push(line);
            }
        }
        assert!(
            lines
                .iter()
                .any(|line| line == "Whisper device: gpu requested, cpu used: no CUDA device"),
            "{lines:?}"
        );
        std::fs::remove_dir_all(&rig.dir).unwrap();
    }

    /// Change `whisper-config-layout`, tasks 1.2 and 1.3: in the realtime mode the status
    /// lists the missing files, and Download fetches them and loads nothing.
    #[test]
    fn the_realtime_mode_lists_missing_files_and_downloads_without_loading() {
        let rig = rig("realtime", Duration::ZERO, false);
        let mut settings = Settings::default();
        settings.whisper.model = "whisper-base-q8_0".to_owned();
        rig.manager.configure(&settings);
        let status = rig.manager.status();
        assert_eq!(status.state, WhisperState::Off);
        let labels: Vec<&str> = status
            .missing
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(
            labels,
            [
                "Whisper runtime (CPU)",
                "Whisper base (q8_0) model",
                "Silero voice activity model"
            ]
        );
        assert!(
            rig.fetched.lock().unwrap().is_empty(),
            "nothing fetched yet"
        );
        assert!(
            rig.manager.download("whisper-small-q8_0").is_err(),
            "only the selected model"
        );
        rig.manager.download("whisper-base-q8_0").unwrap();
        for _ in 0..300 {
            if rig.fetched.lock().unwrap().len() == 3 && rig.manager.status().missing.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = rig.manager.status();
        assert_eq!(
            status.state,
            WhisperState::Off,
            "the realtime mode stays off"
        );
        assert!(status.missing.is_empty());
        assert!(
            rig.engine.loads.lock().unwrap().is_empty(),
            "nothing loaded"
        );
        assert!(
            rig.manager.download("whisper-base-q8_0").is_err(),
            "nothing waits any more"
        );
        // The GPU device adds the CUDA runtime to what is missing.
        settings.whisper.device = WhisperDevice::Gpu;
        rig.manager.configure(&settings);
        let missing = rig.manager.status().missing;
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].label, "Whisper runtime (NVIDIA GPU, CUDA 12.4)");
        std::fs::remove_dir_all(&rig.dir).unwrap();
    }

    #[test]
    fn the_status_serializes_every_field() {
        let status = WhisperStatus {
            state: WhisperState::NeedsDownload {
                items: vec![DownloadItem::of(&VAD_MODEL)],
            },
            device_used: Some(WhisperDevice::Gpu),
            fallback_reason: Some("x".to_owned()),
            backend: Some("CUDA0".to_owned()),
            load_ms: Some(812),
            ..WhisperStatus::default()
        };
        let json = serde_json::to_value(&status).unwrap();
        for field in [
            "state",
            "model",
            "device_requested",
            "device_used",
            "fallback_reason",
            "backend",
            "language",
            "load_ms",
            "activation_phrases",
            "deactivation_phrases",
            "counters",
            "missing",
        ] {
            assert!(json.get(field).is_some(), "{field}: {json}");
        }
        assert_eq!(json["state"]["kind"], "needs_download");
        assert_eq!(json["device_used"], "gpu");
        for field in [
            "queued",
            "transcribed",
            "dropped_late",
            "dropped_noise",
            "dropped_hallucination",
            "last_transcribe_ms",
            "last_audio_seconds",
        ] {
            assert!(json["counters"].get(field).is_some(), "{field}");
        }
    }

    /// Records when each transcript came.
    #[derive(Default)]
    struct Heard(Mutex<Vec<(Instant, String)>>);

    impl TranscriptSink for Heard {
        fn on_transcript(&self, transcript: Transcript) {
            self.0
                .lock()
                .unwrap()
                .push((Instant::now(), transcript.text));
        }
    }

    /// Waits up to `limit` for the unload lines of Whisper and the voice, in either order;
    /// how long after the start each came.
    fn wait_for_unloads(
        log: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        limit: Duration,
    ) -> (Option<Duration>, Option<Duration>) {
        let started = Instant::now();
        let (mut whisper, mut voice) = (None, None);
        while started.elapsed() < limit && (whisper.is_none() || voice.is_none()) {
            match log.try_recv() {
                Ok(AppEvent::Log { line }) if line.ends_with(" unloaded") => {
                    if line.starts_with("Whisper model ") {
                        whisper = Some(started.elapsed());
                    } else if line.starts_with("voice ") {
                        voice = Some(started.elapsed());
                    }
                }
                Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        (whisper, voice)
    }

    /// The join and the leave without Discord, as the runtime takes them (change
    /// `whisper-config-layout`, task 6.4), gated by `VOICEBRO_WHISPER_UNLOAD_DIR` (an assets
    /// folder with the runtime, the model, the VAD model and the voice) and
    /// `VOICEBRO_WHISPER_UNLOAD_WAV` (speech). Each of `VOICEBRO_WHISPER_UNLOAD_CYCLES`
    /// cycles (default 3): idle; `prepare_join` of Whisper and the voice; idle loaded;
    /// `VOICEBRO_WHISPER_UNLOAD_UTTERANCES` utterances (default 4) through `feed` and `tick`
    /// in real time, 48 kHz stereo as songbird gives them; then at once `clear_guild` and
    /// `release`, as `/leave` does; idle again. Every phase prints the wall clock, so the
    /// CPU, memory and GPU memory measured from outside line up. Fails when a model does not
    /// unload within 30 s. `VOICEBRO_WHISPER_MODEL` and `VOICEBRO_WHISPER_DEVICE` (`cpu` or
    /// `gpu`) pick the model.
    #[test]
    fn join_and_leave_unload_when_enabled() {
        let Ok(assets) = std::env::var("VOICEBRO_WHISPER_UNLOAD_DIR") else {
            return;
        };
        let var =
            |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
        let wav = var("VOICEBRO_WHISPER_UNLOAD_WAV", "");
        let cycles: u32 = var("VOICEBRO_WHISPER_UNLOAD_CYCLES", "3").parse().unwrap();
        let utterances: u32 = var("VOICEBRO_WHISPER_UNLOAD_UTTERANCES", "4")
            .parse()
            .unwrap();
        let mut settings = Settings::default();
        settings.assistant.mode = AssistantMode::Whisper;
        settings.whisper.model = var("VOICEBRO_WHISPER_MODEL", "whisper-large-v3-turbo");
        settings.whisper.device = if var("VOICEBRO_WHISPER_DEVICE", "gpu") == "cpu" {
            WhisperDevice::Cpu
        } else {
            WhisperDevice::Gpu
        };
        let stamp = |what: &str| {
            eprintln!("{} {what}", chrono::Local::now().format("%H:%M:%S%.3f"));
        };
        // 16 kHz mono to 48 kHz stereo i16, 20 ms frames.
        let speech = crate::stt::read_wav_16k(Path::new(&wav)).expect("the WAV");
        let frames: Vec<Vec<i16>> = speech
            .chunks(320)
            .map(|chunk| {
                let mut frame = Vec::with_capacity(1920);
                for index in 0..320 {
                    let sample = chunk.get(index).copied().unwrap_or(0.0);
                    let value = (sample * 32_767.0).clamp(-32_768.0, 32_767.0) as i16;
                    frame.extend([value; 6]);
                }
                frame
            })
            .collect();
        let events = EventBus::default();
        let mut log = events.subscribe();
        let heard = Arc::new(Heard::default());
        let models = Path::new(&assets).join("..").join("whisper-unload-models");
        std::fs::create_dir_all(&models).unwrap();
        let whisper = WhisperManager::new(
            PathBuf::from(&assets),
            models,
            events.clone(),
            Arc::clone(&heard) as Arc<dyn TranscriptSink>,
        );
        let voice = crate::tts::manager::TtsManager::new(PathBuf::from(&assets), events.clone());
        whisper.configure(&settings);
        voice.configure(&settings);
        stamp(&format!(
            "start: {} on {:?}, voice {}, {cycles} cycles of {utterances} utterances of {:.1} s",
            settings.whisper.model,
            settings.whisper.device,
            settings.tts.voice,
            speech.len() as f64 / 16_000.0
        ));
        let guild = 1;
        let mut tick = 0_u64;
        for cycle in 1..=cycles {
            stamp(&format!("cycle {cycle}: idle, nothing loaded"));
            std::thread::sleep(Duration::from_secs(8));
            stamp(&format!("cycle {cycle}: join"));
            let started = Instant::now();
            whisper.prepare_join().expect("Whisper loads");
            let whisper_ms = started.elapsed().as_millis();
            voice.prepare_join().expect("the voice loads");
            stamp(&format!(
                "cycle {cycle}: loaded (Whisper {whisper_ms} ms, voice {} ms), device {:?}; idle",
                started.elapsed().as_millis() - whisper_ms,
                whisper.status().device_used
            ));
            std::thread::sleep(Duration::from_secs(8));
            stamp(&format!("cycle {cycle}: speech"));
            let before = heard.0.lock().unwrap().len();
            let clock = Instant::now();
            let mut step = 0_u32;
            let mut next = |tick: &mut u64| {
                *tick += 1;
                step += 1;
                let due = clock + Duration::from_millis(20 * u64::from(step));
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
            };
            for _ in 0..utterances {
                for frame in &frames {
                    whisper.feed(guild, 7, tick, frame);
                    whisper.tick(guild, tick);
                    next(&mut tick);
                }
                // 1.5 s of silence: the utterance ends after 0.7 s of it.
                for _ in 0..75 {
                    whisper.tick(guild, tick);
                    next(&mut tick);
                }
            }
            let queued = whisper.status().counters.queued;
            let leave = Instant::now();
            stamp(&format!(
                "cycle {cycle}: leave ({} transcripts so far, {queued} waiting)",
                heard.0.lock().unwrap().len() - before
            ));
            while log.try_recv().is_ok() {}
            whisper.clear_guild(guild);
            whisper.release();
            voice.release();
            let (whisper_gone, voice_gone) = wait_for_unloads(&mut log, Duration::from_secs(30));
            let after: Vec<String> = heard
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(at, _)| *at > leave)
                .map(|(at, text)| format!("{:.1} s after: {text}", (*at - leave).as_secs_f64()))
                .collect();
            stamp(&format!(
                "cycle {cycle}: Whisper unloaded {:?} after the leave, the voice {:?} after the leave; transcripts after the leave: {after:?}; idle",
                whisper_gone, voice_gone
            ));
            assert!(whisper_gone.is_some(), "Whisper did not unload in 30 s");
            assert!(voice_gone.is_some(), "the voice did not unload in 30 s");
            std::thread::sleep(Duration::from_secs(12));
        }
        stamp("end");
    }
}
