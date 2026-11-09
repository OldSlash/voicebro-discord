//! Wake-word recognition behind traits, so the engine and the model are settings and the
//! phrase logic never changes with them. See `docs/ARCHITECTURE.md` 4.4.
//!
//! Three layers: a [`SpeechRecognizer`] per speaker (streaming text out of 16 kHz mono),
//! a shared [`LoadedModel`], and a [`WakeEngine`] that knows how to fetch and load a model
//! from the catalog. The [`spotter`] decides when a phrase counts; the [`worker`] thread
//! runs it all; the [`WakeManager`] loads and unloads according to the settings.

pub mod assets;
pub mod catalog;
pub mod spotter;
pub mod vosk;
pub mod worker;

use crate::events::{AppEvent, EventBus};
use crate::settings::{normalize_phrases, SessionMode, Settings};
use catalog::{Asset, ModelEntry};
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
    #[error("model «{0}» is not in the catalog")]
    UnknownModel(String),
    #[error("engine «{0}» is not available")]
    UnknownEngine(String),
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

/// An engine: how to fetch its assets and load a model from the catalog.
pub trait WakeEngine: Send + Sync {
    fn id(&self) -> &'static str;
    /// The archives this engine needs for `model`, library first.
    fn assets(&self, model: &ModelEntry) -> Vec<Asset>;
    fn load(
        &self,
        assets_dir: &Path,
        model: &ModelEntry,
    ) -> Result<Box<dyn LoadedModel>, WakeError>;
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
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            activation: normalize_phrases(&settings.wake.activation_phrases),
            deactivation: normalize_phrases(&settings.wake.deactivation_phrases),
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
    Downloading {
        label: String,
        percent: u8,
    },
    Loading,
    Ready,
    Error {
        message: String,
    },
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

struct Inner {
    engines: Vec<Arc<dyn WakeEngine>>,
    assets_dir: PathBuf,
    events: EventBus,
    sink: Arc<dyn PhraseSink>,
    status: Mutex<WakeStatus>,
    sender: Mutex<Option<SyncSender<WakeMessage>>>,
    /// The (engine, model) the running worker was started with.
    running: Mutex<Option<(String, String)>>,
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
        events: EventBus,
        sink: Arc<dyn PhraseSink>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                engines,
                assets_dir,
                events,
                sink,
                status: Mutex::new(WakeStatus::default()),
                sender: Mutex::new(None),
                running: Mutex::new(None),
                starting: AtomicBool::new(false),
                enabled: AtomicBool::new(false),
                dropped: Arc::new(AtomicU64::new(0)),
            }),
        }
    }

    /// The default engine set: Vosk.
    pub fn with_default_engines(
        assets_dir: PathBuf,
        events: EventBus,
        sink: Arc<dyn PhraseSink>,
    ) -> Self {
        Self::new(vec![Arc::new(vosk::VoskEngine)], assets_dir, events, sink)
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

    /// Starts, reconfigures or stops the engine to match the settings. Loading happens on
    /// a thread of its own; this returns at once.
    pub fn configure(&self, settings: &Settings) {
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
        if phrases.is_empty() {
            self.stop();
            self.set_state(WakeState::NoPhrases);
            return;
        }
        let wanted = (settings.wake.engine.clone(), settings.wake.model.clone());
        let same_model = self
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
            return;
        }
        let manager = self.clone();
        std::thread::Builder::new()
            .name("voicebro-wake-load".to_owned())
            .spawn(move || {
                let result = manager.start(&wanted.0, &wanted.1, phrases);
                manager.inner.starting.store(false, Ordering::Release);
                match result {
                    Ok(()) => {
                        *manager.inner.running.lock().expect("running lock") = Some(wanted);
                        manager.inner.enabled.store(true, Ordering::Release);
                        manager.set_state(WakeState::Ready);
                    }
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

    pub fn stop(&self) {
        self.inner.enabled.store(false, Ordering::Release);
        let sender = self.inner.sender.lock().expect("sender lock").take();
        if let Some(sender) = sender {
            let _ = sender.send(WakeMessage::Stop);
        }
        *self.inner.running.lock().expect("running lock") = None;
    }

    fn start(&self, engine_id: &str, model_id: &str, phrases: PhraseSet) -> Result<(), WakeError> {
        let entry =
            catalog::find(model_id).ok_or_else(|| WakeError::UnknownModel(model_id.to_owned()))?;
        let engine = self
            .inner
            .engines
            .iter()
            .find(|engine| engine.id() == engine_id && engine.id() == entry.engine)
            .cloned()
            .ok_or_else(|| WakeError::UnknownEngine(engine_id.to_owned()))?;
        for asset in engine.assets(entry) {
            if assets::is_installed(&self.inner.assets_dir, &asset) {
                continue;
            }
            let events = self.inner.events.clone();
            let manager = self.clone();
            let label = asset.label.to_owned();
            events.log(format!("downloading the {label} (first use only)"));
            assets::ensure(&asset, &self.inner.assets_dir, &|percent| {
                manager.set_state(WakeState::Downloading {
                    label: label.clone(),
                    percent,
                });
                events.publish(AppEvent::Assets {
                    label: label.clone(),
                    percent,
                    error: None,
                });
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
        self.set_state(WakeState::Loading);
        let model = engine.load(&self.inner.assets_dir, entry)?;
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
        Ok(())
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
            EventBus::new(8),
            Arc::new(NoSink),
        );
        let mut settings = Settings::default();
        settings.wake.activation_phrases = vec!["alpha".to_owned()];
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
}
