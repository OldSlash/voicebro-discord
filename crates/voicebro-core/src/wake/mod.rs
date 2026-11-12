//! Wake-word recognition behind traits, so the engine and the model are settings and the
//! phrase logic never changes with them. See `docs/ARCHITECTURE.md` 4.4.
//!
//! Three layers: a [`SpeechRecognizer`] per speaker (streaming text out of 16 kHz mono),
//! a shared [`LoadedModel`], and a [`WakeEngine`] that knows how to fetch and load a model
//! from the catalog. The [`spotter`] decides when a phrase counts; the [`worker`] thread
//! runs it all; the [`WakeManager`] loads and unloads according to the settings.

pub mod assets;
pub mod catalog;
pub mod local;
pub mod spotter;
pub mod vosk;
pub mod worker;

use crate::events::{AppEvent, EventBus};
use crate::settings::{local_model_folder, normalize_phrases, SessionMode, Settings};
use catalog::{Asset, ModelEntry};
use local::LocalModel;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 16 kHz mono: what every recognizer consumes.
pub const RECOGNIZER_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhraseKind {
    Activate,
    Deactivate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub word: String,
    /// End of the word in seconds of audio fed to the recognizer.
    pub end: f64,
}

/// A partial or final recognizer result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recognition {
    pub is_final: bool,
    pub text: String,
    /// Word timings when the engine gives them (Vosk: final results only).
    pub words: Vec<Word>,
}

#[derive(Debug, thiserror::Error)]
pub enum WakeError {
    #[error("{0}")]
    Assets(String),
    #[error("loading the recognizer: {0}")]
    Load(String),
    #[error("model «{0}» is neither in the catalog nor a local: model")]
    UnknownModel(String),
    #[error("engine «{0}» is not available")]
    UnknownEngine(String),
    #[error("the model folder {} is missing; put the model back or choose another one", .0.display())]
    MissingLocalModel(PathBuf),
}

/// Where a model comes from: the catalog (downloaded by the app) or the models folder
/// (put there by the user).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    Catalog(&'static ModelEntry),
    Local { folder: String },
}

impl ModelSource {
    /// The source of a settings model id: a catalog id, or `local:<plain folder name>`.
    pub fn from_id(id: &str) -> Option<Self> {
        if let Some(entry) = catalog::find(id) {
            return Some(Self::Catalog(entry));
        }
        local_model_folder(id).map(|folder| Self::Local {
            folder: folder.to_owned(),
        })
    }

    /// The engine that loads this model.
    pub fn engine(&self) -> &'static str {
        match self {
            Self::Catalog(entry) => entry.engine,
            Self::Local { .. } => catalog::VOSK_ENGINE,
        }
    }

    /// The model's directory: under the assets for a catalog model, under the models
    /// folder for a local one.
    pub fn directory(&self, assets_dir: &Path, models_dir: &Path) -> PathBuf {
        match self {
            Self::Catalog(entry) => assets_dir.join(entry.asset.directory),
            Self::Local { folder } => models_dir.join(folder),
        }
    }
}

/// A streaming recognizer for one speaker.
pub trait SpeechRecognizer: Send {
    /// Feeds 16 kHz mono PCM; returns the current partial or final result.
    fn accept(&mut self, pcm_16k_mono: &[i16]) -> Option<Recognition>;
}

/// A loaded model, shared by every recognizer of one engine.
pub trait LoadedModel: Send + Sync {
    fn new_recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, WakeError>;
    /// `None` when the engine cannot tell.
    fn knows_word(&self, word: &str) -> Option<bool>;
}

/// An engine: which archives it downloads for a model, and how it loads a model directory.
pub trait WakeEngine: Send + Sync {
    fn id(&self) -> &'static str;
    /// The archives this engine needs for `model`, library first: the library and the
    /// model's archive for a catalog model, the library only for a local one.
    fn assets(&self, model: &ModelSource) -> Vec<Asset>;
    /// Loads the model in `model_dir` with the engine's library from `assets_dir`.
    fn load(&self, assets_dir: &Path, model_dir: &Path) -> Result<Box<dyn LoadedModel>, WakeError>;
}

/// Receives confirmed phrases. The session gate implements it.
pub trait PhraseSink: Send + Sync {
    /// Returns whether the phrase changed anything (so the worker can log it).
    fn on_phrase(
        &self,
        guild: u64,
        kind: PhraseKind,
        phrase: &str,
        ssrc: u32,
        since_phrase_end: Duration,
    ) -> bool;
}

/// Receives each speaker's 48 kHz stereo frames from the receive path.
pub trait WakeFeed: Send + Sync {
    fn feed(&self, guild: u64, ssrc: u32, frame: &[i16]);
}

/// Activation and deactivation phrases, normalized.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhraseSet {
    pub activation: Vec<String>,
    pub deactivation: Vec<String>,
}

impl PhraseSet {
    /// The selected model's phrases; other models' phrases are not spotted.
    pub fn from_settings(settings: &Settings) -> Self {
        let selected = settings.wake.selected_phrases();
        Self {
            activation: normalize_phrases(&selected.activation),
            deactivation: normalize_phrases(&selected.deactivation),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.activation.is_empty() && self.deactivation.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WakeState {
    /// Session mode is not wake word; nothing is loaded.
    Off,
    /// Wake-word mode, but no phrases to spot; nothing is loaded.
    NoPhrases,
    /// Files are missing and the user has not agreed to download them yet; nothing is
    /// requested from the network until [`WakeManager::download`].
    NeedsDownload {
        items: Vec<DownloadItem>,
    },
    Downloading {
        label: String,
        percent: u8,
    },
    /// An archive, downloaded or put into the assets folder by hand, is being checked
    /// against its pinned hash and extracted.
    Installing {
        label: String,
    },
    Loading,
    Ready,
    Error {
        message: String,
    },
}

/// One file the engine needs before it can load, as the download notice shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadItem {
    pub label: String,
    /// The site it comes from (`github.com`, `alphacephei.com`).
    pub host: String,
    pub bytes: u64,
    /// The archive's file name, as a download by hand must be named in the assets folder.
    pub file: String,
    /// The official page to download it from by hand.
    pub page: String,
}

impl DownloadItem {
    fn of(asset: &Asset) -> Self {
        let host = asset
            .url
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(asset.url)
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        Self {
            label: asset.label.to_owned(),
            host,
            bytes: asset.size,
            file: asset.file_name().to_owned(),
            page: asset.page.to_owned(),
        }
    }
}

/// A model as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub engine: String,
    /// BCP 47 tag of a catalog model, or the language from a local folder name (may be empty).
    pub language: String,
    pub display_name: String,
    /// Empty for a local model: the user knows where it came from.
    pub license: String,
    /// `catalog` or `local`.
    pub source: &'static str,
    /// The model's files are on disk (always for a local model).
    pub installed: bool,
    /// The size of the model's download; 0 for a local model.
    pub bytes: u64,
}

/// Decimal megabytes, rounded, as the window shows sizes (41,205,931 bytes → 41).
pub fn megabytes(bytes: u64) -> u64 {
    (bytes + 500_000) / 1_000_000
}

/// What a start ended in, when it did not fail.
enum Started {
    Ready,
    /// The files were downloaded; with no phrases to spot, nothing was loaded.
    Installed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WakeStatus {
    pub state: WakeState,
    pub engine: String,
    pub model: String,
    pub activation_phrases: Vec<String>,
    pub deactivation_phrases: Vec<String>,
    /// Words of the configured phrases the model does not know.
    pub unknown_words: Vec<String>,
    /// Audio chunks dropped because the worker fell behind.
    pub dropped_chunks: u64,
}

impl Default for WakeStatus {
    fn default() -> Self {
        Self {
            state: WakeState::Off,
            engine: String::new(),
            model: String::new(),
            activation_phrases: Vec::new(),
            deactivation_phrases: Vec::new(),
            unknown_words: Vec::new(),
            dropped_chunks: 0,
        }
    }
}

pub(crate) enum WakeMessage {
    Audio {
        guild: u64,
        ssrc: u32,
        samples: Vec<i16>,
    },
    SetPhrases(PhraseSet),
    Stop,
}

/// Capacity of the queue toward the worker: about 5 s of one speaker's frames.
pub const AUDIO_QUEUE: usize = 256;

/// Downloads, verifies and extracts one asset: [`assets::ensure`], replaced in tests.
type Fetch =
    dyn Fn(&Asset, &Path, &dyn Fn(assets::Progress)) -> Result<(), WakeError> + Send + Sync;

struct Inner {
    engines: Vec<Arc<dyn WakeEngine>>,
    fetch: Arc<Fetch>,
    assets_dir: PathBuf,
    models_dir: PathBuf,
    /// The models folder's models, as found by the last [`WakeManager::scan_local_models`].
    local_models: Mutex<Vec<LocalModel>>,
    events: EventBus,
    sink: Arc<dyn PhraseSink>,
    status: Mutex<WakeStatus>,
    sender: Mutex<Option<SyncSender<WakeMessage>>>,
    /// The (engine, model) the running worker was started with.
    running: Mutex<Option<(String, String)>>,
    /// The (engine, model) whose missing files the user agreed to download. In memory
    /// only: once the files are on disk, nothing needs asking again.
    consent: Mutex<Option<(String, String)>>,
    /// The settings of the last `configure`, run again when the user agrees.
    last_settings: Mutex<Option<Settings>>,
    /// Counts `configure` calls, so a load that ends after newer settings arrived applies
    /// them instead of keeping what it was started with.
    generation: AtomicU64,
    starting: AtomicBool,
    enabled: AtomicBool,
    dropped: Arc<AtomicU64>,
}

/// Loads the engine only in wake-word mode with phrases to spot, reconfigures on settings
/// changes, and feeds the worker.
#[derive(Clone)]
pub struct WakeManager {
    inner: Arc<Inner>,
}

impl WakeManager {
    pub fn new(
        engines: Vec<Arc<dyn WakeEngine>>,
        assets_dir: PathBuf,
        models_dir: PathBuf,
        events: EventBus,
        sink: Arc<dyn PhraseSink>,
    ) -> Self {
        Self::with_fetch(
            engines,
            Arc::new(assets::ensure),
            assets_dir,
            models_dir,
            events,
            sink,
        )
    }

    fn with_fetch(
        engines: Vec<Arc<dyn WakeEngine>>,
        fetch: Arc<Fetch>,
        assets_dir: PathBuf,
        models_dir: PathBuf,
        events: EventBus,
        sink: Arc<dyn PhraseSink>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                engines,
                fetch,
                assets_dir,
                models_dir,
                local_models: Mutex::new(Vec::new()),
                events,
                sink,
                status: Mutex::new(WakeStatus::default()),
                sender: Mutex::new(None),
                running: Mutex::new(None),
                consent: Mutex::new(None),
                last_settings: Mutex::new(None),
                generation: AtomicU64::new(0),
                starting: AtomicBool::new(false),
                enabled: AtomicBool::new(false),
                dropped: Arc::new(AtomicU64::new(0)),
            }),
        }
    }

    /// The default engine set: Vosk.
    pub fn with_default_engines(
        assets_dir: PathBuf,
        models_dir: PathBuf,
        events: EventBus,
        sink: Arc<dyn PhraseSink>,
    ) -> Self {
        Self::new(
            vec![Arc::new(vosk::VoskEngine)],
            assets_dir,
            models_dir,
            events,
            sink,
        )
    }

    /// Creates the models folder when it is missing and lists its models. Called at start;
    /// a model added later needs a restart.
    pub fn scan_local_models(&self) -> Vec<LocalModel> {
        if let Err(error) = std::fs::create_dir_all(&self.inner.models_dir) {
            log::warn!(
                "could not create the models folder {}: {error}",
                self.inner.models_dir.display()
            );
        }
        let models = local::scan(&self.inner.models_dir);
        *self.inner.local_models.lock().expect("local models lock") = models.clone();
        models
    }

    /// Every model the window can offer: the catalog's, with whether each is downloaded,
    /// then the models folder's from the last scan.
    pub fn models(&self) -> Vec<ModelInfo> {
        let catalog = catalog::entries().iter().map(|entry| ModelInfo {
            id: entry.id.to_owned(),
            engine: entry.engine.to_owned(),
            language: entry.language.to_owned(),
            display_name: entry.display_name.to_owned(),
            license: entry.license.to_owned(),
            source: "catalog",
            installed: assets::is_installed(&self.inner.assets_dir, &entry.asset),
            bytes: entry.asset.size,
        });
        let local = self.local_models().into_iter().map(|model| ModelInfo {
            id: model.id(),
            engine: catalog::VOSK_ENGINE.to_owned(),
            display_name: model.folder.clone(),
            language: model.language,
            license: String::new(),
            source: "local",
            installed: true,
            bytes: 0,
        });
        catalog.chain(local).collect()
    }

    /// The models found by the last scan.
    pub fn local_models(&self) -> Vec<LocalModel> {
        self.inner
            .local_models
            .lock()
            .expect("local models lock")
            .clone()
    }

    pub fn assets_dir(&self) -> &Path {
        &self.inner.assets_dir
    }

    pub fn models_dir(&self) -> &Path {
        &self.inner.models_dir
    }

    pub fn status(&self) -> WakeStatus {
        let mut status = self.inner.status.lock().expect("wake status lock").clone();
        status.dropped_chunks = self.inner.dropped.load(Ordering::Relaxed);
        status
    }

    /// Whether frames should be fed right now.
    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Acquire)
    }

    fn set_state(&self, state: WakeState) {
        let snapshot = {
            let mut status = self.inner.status.lock().expect("wake status lock");
            status.state = state;
            status.dropped_chunks = self.inner.dropped.load(Ordering::Relaxed);
            status.clone()
        };
        self.inner
            .events
            .publish(AppEvent::Wake(Box::new(snapshot)));
    }

    pub(crate) fn set_unknown_words(&self, words: Vec<String>) {
        let snapshot = {
            let mut status = self.inner.status.lock().expect("wake status lock");
            status.unknown_words = words;
            status.clone()
        };
        self.inner
            .events
            .publish(AppEvent::Wake(Box::new(snapshot)));
    }

    /// Starts, reconfigures or stops the engine to match the settings. In wake-word mode
    /// the selected model's files are checked here, before the phrases, so a missing model
    /// is reported at once, with or without phrases. Downloading and loading happen on a
    /// thread of its own; this returns at once.
    pub fn configure(&self, settings: &Settings) {
        *self.inner.last_settings.lock().expect("last settings lock") = Some(settings.clone());
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let phrases = PhraseSet::from_settings(settings);
        {
            let mut status = self.inner.status.lock().expect("wake status lock");
            status.engine = settings.wake.engine.clone();
            status.model = settings.wake.model.clone();
            status.activation_phrases = phrases.activation.clone();
            status.deactivation_phrases = phrases.deactivation.clone();
        }
        if settings.session.mode != SessionMode::WakeWord {
            self.stop();
            self.set_state(WakeState::Off);
            return;
        }
        let wanted = (settings.wake.engine.clone(), settings.wake.model.clone());
        // `None` when the files cannot be told now (an unknown model or engine, a missing
        // local folder): the loader reports that as an error.
        let missing = self.missing_assets(&wanted.0, &wanted.1);
        let to_fetch = missing.clone().unwrap_or_default();
        if !to_fetch.is_empty() && !self.agreed(&wanted) {
            self.stop();
            let items: Vec<DownloadItem> = to_fetch.iter().map(DownloadItem::of).collect();
            self.inner.events.log(format!(
                "wake word: {} needs a download first ({}); waiting for the OK in the window",
                wanted.1,
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
            self.set_state(WakeState::NeedsDownload { items });
            return;
        }
        if phrases.is_empty() && to_fetch.is_empty() {
            self.stop();
            self.set_state(WakeState::NoPhrases);
            return;
        }
        let same_model = to_fetch.is_empty()
            && self
                .inner
                .running
                .lock()
                .expect("running lock")
                .as_ref()
                .map(|running| *running == wanted)
                .unwrap_or(false);
        if same_model {
            let sender = self.inner.sender.lock().expect("sender lock").clone();
            if let Some(sender) = sender {
                if sender
                    .send(WakeMessage::SetPhrases(phrases.clone()))
                    .is_ok()
                {
                    return;
                }
            }
        }
        self.stop();
        if self.inner.starting.swap(true, Ordering::AcqRel) {
            // A load is running; it applies these settings when it ends (see below).
            return;
        }
        let manager = self.clone();
        std::thread::Builder::new()
            .name("voicebro-wake-load".to_owned())
            .spawn(move || {
                let result = manager.start(&wanted.0, &wanted.1, phrases, to_fetch);
                manager.inner.starting.store(false, Ordering::Release);
                if manager.inner.generation.load(Ordering::Acquire) != generation {
                    // Newer settings arrived during the download or load: apply them
                    // rather than publish a state for settings that are gone.
                    let latest = manager
                        .inner
                        .last_settings
                        .lock()
                        .expect("last settings lock")
                        .clone();
                    if let Some(latest) = latest {
                        manager.configure(&latest);
                        return;
                    }
                }
                match result {
                    Ok(Started::Ready) => {
                        *manager.inner.running.lock().expect("running lock") = Some(wanted);
                        manager.inner.enabled.store(true, Ordering::Release);
                        manager.set_state(WakeState::Ready);
                    }
                    Ok(Started::Installed) => manager.set_state(WakeState::NoPhrases),
                    Err(error) => {
                        manager
                            .inner
                            .events
                            .log(format!("wake word engine failed: {error}"));
                        manager.set_state(WakeState::Error {
                            message: error.to_string(),
                        });
                    }
                }
            })
            .expect("spawn the wake loader thread");
    }

    /// The engine that loads `source` under the engine id the settings name.
    fn engine_for(&self, engine_id: &str, source: &ModelSource) -> Option<Arc<dyn WakeEngine>> {
        self.inner
            .engines
            .iter()
            .find(|engine| engine.id() == engine_id && engine.id() == source.engine())
            .cloned()
    }

    /// The selected model's files that are not on disk; `None` when that cannot be told
    /// (an unknown model or engine, a local folder that is gone).
    fn missing_assets(&self, engine_id: &str, model_id: &str) -> Option<Vec<Asset>> {
        let source = ModelSource::from_id(model_id)?;
        let engine = self.engine_for(engine_id, &source)?;
        if matches!(source, ModelSource::Local { .. })
            && !source
                .directory(&self.inner.assets_dir, &self.inner.models_dir)
                .is_dir()
        {
            return None;
        }
        Some(
            engine
                .assets(&source)
                .into_iter()
                .filter(|asset| !assets::is_installed(&self.inner.assets_dir, asset))
                .collect(),
        )
    }

    fn agreed(&self, wanted: &(String, String)) -> bool {
        self.inner
            .consent
            .lock()
            .expect("consent lock")
            .as_ref()
            .is_some_and(|agreed| agreed == wanted)
    }

    /// What choosing `model_id` would need to download, without changing anything: the
    /// engine's library and/or the model, as the install dialog lists them.
    pub fn requirements(&self, model_id: &str) -> Result<Vec<DownloadItem>, String> {
        let source = ModelSource::from_id(model_id)
            .ok_or_else(|| WakeError::UnknownModel(model_id.to_owned()).to_string())?;
        let engine = self
            .engine_for(source.engine(), &source)
            .ok_or_else(|| WakeError::UnknownEngine(source.engine().to_owned()).to_string())?;
        Ok(engine
            .assets(&source)
            .iter()
            .filter(|asset| !assets::is_installed(&self.inner.assets_dir, asset))
            .map(DownloadItem::of)
            .collect())
    }

    pub fn stop(&self) {
        self.inner.enabled.store(false, Ordering::Release);
        let sender = self.inner.sender.lock().expect("sender lock").take();
        if let Some(sender) = sender {
            let _ = sender.send(WakeMessage::Stop);
        }
        *self.inner.running.lock().expect("running lock") = None;
    }

    /// The user agreed to download what `model_id` needs. Refused unless that is the
    /// selected model, so a settings change in between never downloads a model the user
    /// did not agree to. Records the consent for that model and starts the engine again
    /// with the last settings. Also the retry after a failed download (the `Error` state).
    pub fn download(&self, model_id: &str) -> Result<(), String> {
        let settings = self
            .inner
            .last_settings
            .lock()
            .expect("last settings lock")
            .clone()
            .ok_or_else(|| "the wake engine has not been configured yet".to_owned())?;
        if settings.wake.model != model_id {
            return Err(format!(
                "{model_id} is not the selected wake model ({})",
                settings.wake.model
            ));
        }
        if !matches!(
            self.status().state,
            WakeState::NeedsDownload { .. } | WakeState::Error { .. }
        ) {
            return Err("nothing is waiting for a download".to_owned());
        }
        let wanted = (settings.wake.engine.clone(), settings.wake.model.clone());
        self.inner
            .events
            .log(format!("wake word: download agreed for {}", wanted.1));
        *self.inner.consent.lock().expect("consent lock") = Some(wanted);
        self.configure(&settings);
        Ok(())
    }

    /// Downloads `missing` (which `configure` found and the user agreed to), then loads
    /// the model when there are phrases to spot.
    fn start(
        &self,
        engine_id: &str,
        model_id: &str,
        phrases: PhraseSet,
        missing: Vec<Asset>,
    ) -> Result<Started, WakeError> {
        let source = ModelSource::from_id(model_id)
            .ok_or_else(|| WakeError::UnknownModel(model_id.to_owned()))?;
        let engine = self
            .engine_for(engine_id, &source)
            .ok_or_else(|| WakeError::UnknownEngine(engine_id.to_owned()))?;
        let model_dir = source.directory(&self.inner.assets_dir, &self.inner.models_dir);
        if matches!(source, ModelSource::Local { .. }) && !model_dir.is_dir() {
            return Err(WakeError::MissingLocalModel(model_dir));
        }
        for asset in missing {
            let events = self.inner.events.clone();
            let manager = self.clone();
            let label = asset.label.to_owned();
            if assets::archive_path(&self.inner.assets_dir, &asset).is_file() {
                events.log(format!(
                    "using {} from the assets folder for the {label}",
                    asset.file_name()
                ));
            } else {
                events.log(format!("downloading the {label} (first use only)"));
            }
            (self.inner.fetch)(&asset, &self.inner.assets_dir, &|step| match step {
                assets::Progress::Downloaded(percent) => {
                    manager.set_state(WakeState::Downloading {
                        label: label.clone(),
                        percent,
                    });
                    events.publish(AppEvent::Assets {
                        label: label.clone(),
                        percent,
                        error: None,
                    });
                }
                assets::Progress::Unpacking => {
                    manager.set_state(WakeState::Installing {
                        label: label.clone(),
                    });
                }
            })
            .inspect_err(|error| {
                self.inner.events.publish(AppEvent::Assets {
                    label: asset.label.to_owned(),
                    percent: 0,
                    error: Some(error.to_string()),
                });
            })?;
            events.log(format!("{label} installed"));
        }
        if phrases.is_empty() {
            return Ok(Started::Installed);
        }
        self.set_state(WakeState::Loading);
        let model = engine.load(&self.inner.assets_dir, &model_dir)?;
        let (sender, receiver) = sync_channel(AUDIO_QUEUE);
        let worker = worker::Worker::new(
            model,
            phrases,
            Arc::clone(&self.inner.sink),
            self.inner.events.clone(),
            Arc::clone(&self.inner.dropped),
            self.clone(),
        );
        std::thread::Builder::new()
            .name("voicebro-wake".to_owned())
            .spawn(move || worker.run(receiver))
            .map_err(|error| {
                WakeError::Load(format!("could not start the wake thread: {error}"))
            })?;
        *self.inner.sender.lock().expect("sender lock") = Some(sender);
        Ok(Started::Ready)
    }
}

impl WakeFeed for WakeManager {
    fn feed(&self, guild: u64, ssrc: u32, frame: &[i16]) {
        if !self.is_enabled() {
            return;
        }
        let sender = self.inner.sender.lock().expect("sender lock").clone();
        let Some(sender) = sender else {
            return;
        };
        match sender.try_send(WakeMessage::Audio {
            guild,
            ssrc,
            samples: frame.to_vec(),
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.inner.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.inner.sender.lock().expect("sender lock").take();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    struct NoSink;
    impl PhraseSink for NoSink {
        fn on_phrase(&self, _: u64, _: PhraseKind, _: &str, _: u32, _: Duration) -> bool {
            false
        }
    }

    #[test]
    fn always_mode_and_empty_phrases_keep_the_engine_off() {
        let dir = tempfile::tempdir().unwrap();
        let manager = WakeManager::new(
            Vec::new(),
            dir.path().to_path_buf(),
            dir.path().join("models"),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let mut settings = Settings::default();
        settings.session.mode = SessionMode::Always;
        manager.configure(&settings);
        assert_eq!(manager.status().state, WakeState::Off);
        settings.session.mode = SessionMode::WakeWord;
        manager.configure(&settings);
        assert_eq!(manager.status().state, WakeState::NoPhrases);
        assert!(!manager.is_enabled());
    }

    #[test]
    fn an_unknown_engine_ends_in_the_error_state() {
        let dir = tempfile::tempdir().unwrap();
        let manager = WakeManager::new(
            Vec::new(),
            dir.path().to_path_buf(),
            dir.path().join("models"),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            settings.wake.model.clone(),
            crate::settings::ModelPhrases {
                activation: vec!["alpha".to_owned()],
                deactivation: Vec::new(),
            },
        );
        manager.configure(&settings);
        for _ in 0..50 {
            if matches!(manager.status().state, WakeState::Error { .. }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            matches!(manager.status().state, WakeState::Error { .. }),
            "{:?}",
            manager.status().state
        );
    }

    #[test]
    fn phrases_of_another_model_leave_the_selected_one_without_phrases() {
        let dir = tempfile::tempdir().unwrap();
        let manager = WakeManager::new(
            Vec::new(),
            dir.path().to_path_buf(),
            dir.path().join("models"),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            catalog::RUSSIAN_SMALL.to_owned(),
            crate::settings::ModelPhrases {
                activation: vec!["братан".to_owned()],
                deactivation: vec!["спасибо братан".to_owned()],
            },
        );
        assert_ne!(settings.wake.model, catalog::RUSSIAN_SMALL);
        manager.configure(&settings);
        let status = manager.status();
        assert_eq!(status.state, WakeState::NoPhrases);
        assert!(status.activation_phrases.is_empty(), "{status:?}");
        assert!(!manager.is_enabled());
    }

    /// A "vosk" engine that needs no archives and loads any existing directory, recording
    /// what it was asked for.
    #[derive(Default)]
    struct FakeEngine {
        asked: Mutex<Vec<ModelSource>>,
        loaded: Mutex<Vec<PathBuf>>,
    }

    struct FakeModel;
    impl LoadedModel for FakeModel {
        fn new_recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, WakeError> {
            Err(WakeError::Load("not in this test".to_owned()))
        }
        fn knows_word(&self, _: &str) -> Option<bool> {
            Some(true)
        }
    }

    impl WakeEngine for FakeEngine {
        fn id(&self) -> &'static str {
            catalog::VOSK_ENGINE
        }
        fn assets(&self, model: &ModelSource) -> Vec<Asset> {
            self.asked.lock().unwrap().push(model.clone());
            Vec::new()
        }
        fn load(&self, _: &Path, model_dir: &Path) -> Result<Box<dyn LoadedModel>, WakeError> {
            self.loaded.lock().unwrap().push(model_dir.to_path_buf());
            Ok(Box::new(FakeModel))
        }
    }

    fn wait_for(manager: &WakeManager, done: impl Fn(&WakeState) -> bool) -> WakeState {
        for _ in 0..100 {
            let state = manager.status().state;
            if done(&state) {
                return state;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        manager.status().state
    }

    fn local_settings(folder: &str) -> Settings {
        let mut settings = Settings::default();
        settings.wake.model = format!("local:{folder}");
        settings.wake.phrases.insert(
            settings.wake.model.clone(),
            crate::settings::ModelPhrases {
                activation: vec!["hallo".to_owned()],
                deactivation: Vec::new(),
            },
        );
        settings
    }

    #[test]
    fn a_local_model_loads_from_the_models_folder() {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models");
        let folder = "vosk-model-small-de-0.15";
        for sub in ["am", "conf"] {
            std::fs::create_dir_all(models.join(folder).join(sub)).unwrap();
        }
        std::fs::write(models.join(folder).join("am/final.mdl"), b"").unwrap();
        std::fs::write(models.join(folder).join("conf/mfcc.conf"), b"").unwrap();
        let engine = Arc::new(FakeEngine::default());
        let manager = WakeManager::new(
            vec![engine.clone()],
            dir.path().join("assets"),
            models.clone(),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let found = manager.scan_local_models();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].language, "de");
        manager.configure(&local_settings(folder));
        let state = wait_for(&manager, |state| *state == WakeState::Ready);
        assert_eq!(state, WakeState::Ready);
        assert_eq!(
            *engine.asked.lock().unwrap(),
            vec![ModelSource::Local {
                folder: folder.to_owned()
            }]
        );
        assert_eq!(*engine.loaded.lock().unwrap(), vec![models.join(folder)]);
    }

    #[test]
    fn a_missing_local_folder_is_an_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(FakeEngine::default());
        let manager = WakeManager::new(
            vec![engine.clone()],
            dir.path().join("assets"),
            dir.path().join("models"),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        manager.scan_local_models();
        assert!(
            dir.path().join("models").is_dir(),
            "the scan creates the folder"
        );
        manager.configure(&local_settings("gone-model"));
        let state = wait_for(&manager, |state| matches!(state, WakeState::Error { .. }));
        let WakeState::Error { message } = state else {
            panic!("{state:?}");
        };
        assert!(message.contains("gone-model"), "{message}");
        assert!(engine.loaded.lock().unwrap().is_empty());
    }

    /// A "vosk" engine that needs one archive for any model.
    struct OneAssetEngine;
    const TEST_ASSET: Asset = Asset {
        label: "Test model",
        page: "https://example.test/models",
        url: "https://example.test/models/test-model.zip",
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        directory: "test-model",
        marker: "am/final.mdl",
        size: 12_000_000,
    };
    impl WakeEngine for OneAssetEngine {
        fn id(&self) -> &'static str {
            catalog::VOSK_ENGINE
        }
        fn assets(&self, _: &ModelSource) -> Vec<Asset> {
            vec![TEST_ASSET]
        }
        fn load(&self, _: &Path, _: &Path) -> Result<Box<dyn LoadedModel>, WakeError> {
            Ok(Box::new(FakeModel))
        }
    }

    /// A manager whose downloads only create the marker file, counting each call.
    fn counting_manager(dir: &Path) -> (WakeManager, Arc<AtomicU64>) {
        let fetched = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&fetched);
        let fetch: Arc<Fetch> = Arc::new(move |asset: &Asset, assets_dir: &Path, progress| {
            counter.fetch_add(1, Ordering::SeqCst);
            let marker = assets_dir.join(asset.directory).join(asset.marker);
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(marker, b"").unwrap();
            progress(assets::Progress::Downloaded(100));
            Ok(())
        });
        let manager = WakeManager::with_fetch(
            vec![Arc::new(OneAssetEngine)],
            fetch,
            dir.join("assets"),
            dir.join("models"),
            EventBus::new(64),
            Arc::new(NoSink),
        );
        (manager, fetched)
    }

    fn english_with_phrases() -> Settings {
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            settings.wake.model.clone(),
            crate::settings::ModelPhrases {
                activation: vec!["bro".to_owned()],
                deactivation: Vec::new(),
            },
        );
        settings
    }

    #[test]
    fn nothing_downloads_until_the_user_agrees() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, fetched) = counting_manager(dir.path());
        assert!(
            manager.download(catalog::default_entry().id).is_err(),
            "nothing configured yet"
        );
        manager.configure(&english_with_phrases());
        let state = wait_for(&manager, |state| {
            matches!(state, WakeState::NeedsDownload { .. })
        });
        assert_eq!(
            state,
            WakeState::NeedsDownload {
                items: vec![DownloadItem {
                    label: "Test model".to_owned(),
                    host: "example.test".to_owned(),
                    bytes: 12_000_000,
                    file: "test-model.zip".to_owned(),
                    page: "https://example.test/models".to_owned(),
                }]
            }
        );
        assert_eq!(
            fetched.load(Ordering::SeqCst),
            0,
            "no request without the OK"
        );
        assert!(!manager.is_enabled(), "no session from a wake phrase");
        // A save that changes nothing about the model asks again, still without fetching.
        manager.configure(&english_with_phrases());
        wait_for(&manager, |state| {
            matches!(state, WakeState::NeedsDownload { .. })
        });
        assert_eq!(fetched.load(Ordering::SeqCst), 0);

        manager
            .download(catalog::default_entry().id)
            .expect("waiting for the OK");
        assert_eq!(
            wait_for(&manager, |state| *state == WakeState::Ready),
            WakeState::Ready
        );
        assert_eq!(fetched.load(Ordering::SeqCst), 1);
        assert!(manager.is_enabled());
        assert!(
            manager.download(catalog::default_entry().id).is_err(),
            "nothing is waiting any more"
        );
    }

    #[test]
    fn a_failed_download_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let attempts = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&attempts);
        let fetch: Arc<Fetch> = Arc::new(move |asset: &Asset, assets_dir: &Path, _progress| {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(WakeError::Assets(format!(
                    "{}: SHA-256 mismatch",
                    asset.label
                )));
            }
            let marker = assets_dir.join(asset.directory).join(asset.marker);
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(marker, b"").unwrap();
            Ok(())
        });
        let manager = WakeManager::with_fetch(
            vec![Arc::new(OneAssetEngine)],
            fetch,
            dir.path().join("assets"),
            dir.path().join("models"),
            EventBus::new(64),
            Arc::new(NoSink),
        );
        manager.configure(&english_with_phrases());
        wait_for(&manager, |state| {
            matches!(state, WakeState::NeedsDownload { .. })
        });
        manager.download(catalog::default_entry().id).unwrap();
        let state = wait_for(&manager, |state| matches!(state, WakeState::Error { .. }));
        assert!(
            matches!(&state, WakeState::Error { message } if message.contains("mismatch")),
            "{state:?}"
        );
        manager
            .download(catalog::default_entry().id)
            .expect("a retry after the error");
        assert_eq!(
            wait_for(&manager, |state| *state == WakeState::Ready),
            WakeState::Ready
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn installed_files_need_no_consent() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir
            .path()
            .join("assets")
            .join(TEST_ASSET.directory)
            .join(TEST_ASSET.marker);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, b"").unwrap();
        let (manager, fetched) = counting_manager(dir.path());
        manager.configure(&english_with_phrases());
        assert_eq!(
            wait_for(&manager, |state| *state == WakeState::Ready),
            WakeState::Ready
        );
        assert_eq!(fetched.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn no_phrases_and_missing_files_ask_for_the_download_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, fetched) = counting_manager(dir.path());
        manager.configure(&Settings::default());
        // Checked in `configure` itself: the window sees it right after a save or a start.
        assert!(
            matches!(&manager.status().state, WakeState::NeedsDownload { items } if items.len() == 1),
            "{:?}",
            manager.status().state
        );
        assert_eq!(fetched.load(Ordering::SeqCst), 0);
        assert!(!manager.is_enabled());
    }

    #[test]
    fn no_phrases_and_installed_files_are_no_phrases() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir
            .path()
            .join("assets")
            .join(TEST_ASSET.directory)
            .join(TEST_ASSET.marker);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, b"").unwrap();
        let (manager, fetched) = counting_manager(dir.path());
        manager.configure(&Settings::default());
        assert_eq!(manager.status().state, WakeState::NoPhrases);
        assert_eq!(fetched.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn always_mode_checks_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(FakeEngine::default());
        let manager = WakeManager::new(
            vec![engine.clone()],
            dir.path().join("assets"),
            dir.path().join("models"),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let mut settings = english_with_phrases();
        settings.session.mode = SessionMode::Always;
        manager.configure(&settings);
        assert_eq!(manager.status().state, WakeState::Off);
        assert!(engine.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn an_agreed_download_without_phrases_ends_in_no_phrases() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, fetched) = counting_manager(dir.path());
        manager.configure(&Settings::default());
        manager
            .download(catalog::default_entry().id)
            .expect("waiting for the OK");
        assert_eq!(
            wait_for(&manager, |state| *state == WakeState::NoPhrases),
            WakeState::NoPhrases
        );
        assert_eq!(fetched.load(Ordering::SeqCst), 1);
        assert!(dir
            .path()
            .join("assets")
            .join(TEST_ASSET.directory)
            .join(TEST_ASSET.marker)
            .is_file());
        assert!(!manager.is_enabled(), "nothing to spot, nothing loaded");
    }

    /// A "vosk" engine that needs the catalog archive of the model and no library.
    struct CatalogAssetEngine;
    impl WakeEngine for CatalogAssetEngine {
        fn id(&self) -> &'static str {
            catalog::VOSK_ENGINE
        }
        fn assets(&self, model: &ModelSource) -> Vec<Asset> {
            match model {
                ModelSource::Catalog(entry) => vec![entry.asset],
                ModelSource::Local { .. } => Vec::new(),
            }
        }
        fn load(&self, _: &Path, _: &Path) -> Result<Box<dyn LoadedModel>, WakeError> {
            Ok(Box::new(FakeModel))
        }
    }

    #[test]
    fn agreeing_for_one_model_downloads_no_other() {
        let dir = tempfile::tempdir().unwrap();
        let fetched = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&fetched);
        let fetch: Arc<Fetch> = Arc::new(move |asset: &Asset, assets_dir: &Path, _| {
            log.lock().unwrap().push(asset.directory.to_owned());
            let marker = assets_dir.join(asset.directory).join(asset.marker);
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(marker, b"").unwrap();
            Ok(())
        });
        let manager = WakeManager::with_fetch(
            vec![Arc::new(CatalogAssetEngine)],
            fetch,
            dir.path().join("assets"),
            dir.path().join("models"),
            EventBus::new(64),
            Arc::new(NoSink),
        );
        let (german, french) = ("vosk-model-small-de-0.15", "vosk-model-small-fr-0.22");
        let mut settings = Settings::default();
        settings.wake.model = german.to_owned();
        manager.configure(&settings);
        assert!(
            manager.download(french).is_err(),
            "French is not the selected model"
        );
        manager.download(german).expect("German waits for the OK");
        wait_for(&manager, |state| *state == WakeState::NoPhrases);
        assert_eq!(*fetched.lock().unwrap(), [german]);

        settings.wake.model = french.to_owned();
        manager.configure(&settings);
        let WakeState::NeedsDownload { items } = manager.status().state else {
            panic!("{:?}", manager.status().state);
        };
        assert_eq!(items[0].label, "Vosk French model");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            *fetched.lock().unwrap(),
            [german],
            "no French without its OK"
        );
    }

    #[test]
    fn requirements_list_what_a_choice_would_download() {
        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        let models = dir.path().join("models");
        let manager = WakeManager::with_default_engines(
            assets_dir.clone(),
            models.clone(),
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let labels = |model: &str| -> Vec<String> {
            manager
                .requirements(model)
                .unwrap()
                .into_iter()
                .map(|item| item.label)
                .collect()
        };
        let german = "vosk-model-small-de-0.15";
        assert_eq!(labels(german), ["Vosk library", "Vosk German model"]);
        assert_eq!(
            manager.requirements(german).unwrap()[0],
            DownloadItem {
                label: "Vosk library".to_owned(),
                host: "github.com".to_owned(),
                bytes: vosk::LIBRARY.size,
                file: "vosk-win64-0.3.45.zip".to_owned(),
                page: "https://github.com/alphacep/vosk-api/releases/tag/v0.3.45".to_owned(),
            }
        );
        let english = manager
            .requirements(catalog::default_entry().id)
            .unwrap()
            .into_iter()
            .map(|item| (item.file, item.page))
            .collect::<Vec<_>>();
        assert_eq!(
            english,
            [
                (
                    "vosk-win64-0.3.45.zip".to_owned(),
                    "https://github.com/alphacep/vosk-api/releases/tag/v0.3.45".to_owned()
                ),
                (
                    "vosk-model-small-en-us-0.15.zip".to_owned(),
                    "https://alphacephei.com/vosk/models".to_owned()
                ),
            ]
        );

        for sub in ["am", "conf"] {
            std::fs::create_dir_all(models.join("vosk-model-small-ca-0.4").join(sub)).unwrap();
        }
        assert_eq!(labels("local:vosk-model-small-ca-0.4"), ["Vosk library"]);

        let library = assets_dir
            .join(vosk::LIBRARY.directory)
            .join(vosk::LIBRARY.marker);
        std::fs::create_dir_all(library.parent().unwrap()).unwrap();
        std::fs::write(&library, b"").unwrap();
        assert_eq!(labels(german), ["Vosk German model"]);
        assert!(labels("local:vosk-model-small-ca-0.4").is_empty());

        assert!(manager.requirements("vosk-model-nope").is_err());
        assert!(manager.requirements("local:../x").is_err());
    }

    #[test]
    fn newer_settings_during_a_download_are_applied_when_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let fetch: Arc<Fetch> = Arc::new(move |asset: &Asset, assets_dir: &Path, _| {
            gate.lock().unwrap().recv().unwrap();
            let marker = assets_dir.join(asset.directory).join(asset.marker);
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(marker, b"").unwrap();
            Ok(())
        });
        let manager = WakeManager::with_fetch(
            vec![Arc::new(OneAssetEngine)],
            fetch,
            dir.path().join("assets"),
            dir.path().join("models"),
            EventBus::new(64),
            Arc::new(NoSink),
        );
        manager.configure(&Settings::default());
        manager.download(catalog::default_entry().id).unwrap();
        // While the download waits, the user switches to always listening.
        let mut always = Settings::default();
        always.session.mode = SessionMode::Always;
        manager.configure(&always);
        release.send(()).unwrap();
        assert_eq!(
            wait_for(&manager, |state| *state == WakeState::Off),
            WakeState::Off,
            "the newer settings win over the finished download"
        );
    }

    #[test]
    fn the_model_list_has_the_catalog_with_its_state_and_the_local_models() {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models");
        for sub in ["am", "conf"] {
            std::fs::create_dir_all(models.join("vosk-model-small-ca-0.4").join(sub)).unwrap();
        }
        std::fs::write(models.join("vosk-model-small-ca-0.4/am/final.mdl"), b"").unwrap();
        std::fs::write(models.join("vosk-model-small-ca-0.4/conf/mfcc.conf"), b"").unwrap();
        let russian = catalog::find(catalog::RUSSIAN_SMALL).unwrap();
        let marker = dir
            .path()
            .join("assets")
            .join(russian.asset.directory)
            .join(russian.asset.marker);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, b"").unwrap();
        let manager = WakeManager::new(
            Vec::new(),
            dir.path().join("assets"),
            models,
            EventBus::new(8),
            Arc::new(NoSink),
        );
        manager.scan_local_models();
        let list = manager.models();
        let summary: Vec<_> = list
            .iter()
            .map(|m| (m.id.as_str(), m.source, m.installed, m.language.as_str()))
            .collect();
        assert_eq!(summary.len(), catalog::entries().len() + 1);
        assert_eq!(
            summary[..2],
            [
                ("vosk-model-small-en-us-0.15", "catalog", false, "en-US"),
                ("vosk-model-small-ru-0.22", "catalog", true, "ru"),
            ]
        );
        assert!(summary[2..catalog::entries().len()]
            .iter()
            .all(|(_, source, installed, _)| *source == "catalog" && !installed));
        assert_eq!(
            summary.last(),
            Some(&("local:vosk-model-small-ca-0.4", "local", true, "ca"))
        );
        assert_eq!(list[0].bytes, 41_205_931);
        assert_eq!(list.last().unwrap().bytes, 0);
    }

    /// A "vosk" engine that needs the given archives for any model.
    struct ArchivesEngine(Vec<Asset>);
    impl WakeEngine for ArchivesEngine {
        fn id(&self) -> &'static str {
            catalog::VOSK_ENGINE
        }
        fn assets(&self, _: &ModelSource) -> Vec<Asset> {
            self.0.clone()
        }
        fn load(&self, _: &Path, _: &Path) -> Result<Box<dyn LoadedModel>, WakeError> {
            Ok(Box::new(FakeModel))
        }
    }

    #[test]
    fn a_library_put_into_the_folder_is_unpacked_and_only_the_model_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
        let file_url =
            |path: &Path| format!("file:///{}", path.display().to_string().replace('\\', "/"));
        let assets_dir = dir.path().join("assets");
        let served = dir.path().join("served");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::create_dir_all(&served).unwrap();
        // The library: downloaded by hand into the assets folder; its URL leads nowhere.
        let (library_zip, library_hash) =
            assets::test_zip(dir.path(), "test-lib", "lib.dll", "made-lib.zip");
        std::fs::copy(&library_zip, assets_dir.join("test-lib.zip")).unwrap();
        let library = Asset {
            label: "Test library",
            page: "https://example.test/releases",
            url: leak(file_url(&dir.path().join("nowhere").join("test-lib.zip"))),
            sha256: leak(library_hash),
            directory: "test-lib",
            marker: "lib.dll",
            size: std::fs::metadata(&library_zip).unwrap().len(),
        };
        // The model: not in the folder, served by a file URL.
        let (model_zip, model_hash) =
            assets::test_zip(dir.path(), "test-model", "am/final.mdl", "made-model.zip");
        std::fs::copy(&model_zip, served.join("test-model.zip")).unwrap();
        let model = Asset {
            label: "Test model",
            page: "https://example.test/models",
            url: leak(file_url(&served.join("test-model.zip"))),
            sha256: leak(model_hash),
            directory: "test-model",
            marker: "am/final.mdl",
            size: std::fs::metadata(&model_zip).unwrap().len(),
        };
        let events = EventBus::new(256);
        let mut seen = events.subscribe();
        let manager = WakeManager::new(
            vec![Arc::new(ArchivesEngine(vec![library, model]))],
            assets_dir.clone(),
            dir.path().join("models"),
            events,
            Arc::new(NoSink),
        );
        manager.configure(&Settings::default());
        manager.download(catalog::default_entry().id).unwrap();
        assert_eq!(
            wait_for(&manager, |state| *state == WakeState::NoPhrases),
            WakeState::NoPhrases
        );
        assert!(assets::is_installed(&assets_dir, &library));
        assert!(assets::is_installed(&assets_dir, &model));
        assert!(!assets_dir.join("test-lib.zip").exists(), "archive removed");

        let (mut lines, mut states) = (Vec::new(), Vec::new());
        while let Ok(event) = seen.try_recv() {
            match event {
                AppEvent::Log { line } => lines.push(line),
                AppEvent::Wake(status) => states.push(status.state),
                _ => {}
            }
        }
        assert!(
            lines.contains(
                &"using test-lib.zip from the assets folder for the Test library".to_owned()
            ),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"downloading the Test model (first use only)".to_owned()),
            "{lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("downloading the Test library")),
            "{lines:?}"
        );
        let downloading: Vec<&str> = states
            .iter()
            .filter_map(|state| match state {
                WakeState::Downloading { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            downloading.iter().all(|label| *label == "Test model"),
            "{states:?}"
        );
        assert!(!downloading.is_empty(), "{states:?}");
        for label in ["Test library", "Test model"] {
            assert!(
                states.contains(&WakeState::Installing {
                    label: label.to_owned()
                }),
                "{states:?}"
            );
        }
    }

    #[test]
    fn the_download_item_names_the_site() {
        let item = DownloadItem::of(&vosk::LIBRARY);
        assert_eq!(item.host, "github.com");
        assert_eq!(item.label, "Vosk library");
        assert_eq!(item.file, "vosk-win64-0.3.45.zip");
        assert_eq!(
            item.page,
            "https://github.com/alphacep/vosk-api/releases/tag/v0.3.45"
        );
        let english = catalog::default_entry();
        assert_eq!(DownloadItem::of(&english.asset).host, "alphacephei.com");
        assert_eq!(
            DownloadItem::of(&english.asset).file,
            "vosk-model-small-en-us-0.15.zip"
        );
        assert_eq!(DownloadItem::of(&english.asset).page, catalog::MODELS_PAGE);
        assert_eq!(DownloadItem::of(&english.asset).bytes, 41_205_931);
        assert_eq!(megabytes(english.asset.size), 41);
        assert_eq!(megabytes(vosk::LIBRARY.size), 15);
        assert_eq!(
            megabytes(catalog::find(catalog::RUSSIAN_SMALL).unwrap().asset.size),
            46
        );
    }

    #[test]
    fn vosk_needs_only_its_library_for_a_local_model() {
        let local = ModelSource::Local {
            folder: "x".to_owned(),
        };
        assert_eq!(vosk::VoskEngine.assets(&local), vec![vosk::LIBRARY]);
        let catalog = ModelSource::from_id(catalog::RUSSIAN_SMALL).unwrap();
        assert_eq!(
            vosk::VoskEngine.assets(&catalog),
            vec![
                vosk::LIBRARY,
                catalog::find(catalog::RUSSIAN_SMALL).unwrap().asset
            ]
        );
        assert_eq!(ModelSource::from_id("local:a/b"), None);
        assert_eq!(ModelSource::from_id("nope"), None);
    }
}
