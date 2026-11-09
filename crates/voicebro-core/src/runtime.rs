//! The composition root. It owns the settings, the secret store, the event bus and the
//! Discord adapter, and it is the only place that knows every subsystem. Later changes add
//! the wake engine, the gate and the assistant here.

use crate::audio::{Frame, OutputHandle};
use crate::discord::{AssistantState, DiscordError, DiscordRuntime, VoiceHost};
use crate::events::{AppEvent, EventBus};
use crate::secrets::{SecretError, SecretKey, SecretStore};
use crate::settings::store::{SettingsStore, StoreError};
use crate::settings::{Settings, ValidationError};
use crate::status::{SecretStatus, Status};
use serde_json::Value;
use serenity::all::GuildId;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::runtime::Handle;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Validation(#[from] ValidationError),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Secret(#[from] SecretError),
    #[error("the patch is not a settings document: {0}")]
    Patch(#[from] serde_json::Error),
    #[error("unknown secret «{0}»")]
    UnknownSecret(String),
    #[error("{0}")]
    Discord(#[from] DiscordError),
}

/// What Discord needs from the app before the session gate exists: the mute flag and the
/// assistant state. The gate replaces this in the next change.
struct Host {
    muted: Arc<AtomicBool>,
    events: EventBus,
    outputs: Mutex<BTreeMap<u64, OutputHandle>>,
}

impl VoiceHost for Host {
    fn on_tick(&self, _guild: u64, _mixed: &Frame, _speakers: &[(u32, &[i16])]) {}

    fn set_speaker_name(&self, _guild: u64, _ssrc: u32, _name: &str) {}

    fn set_participants(&self, _guild: u64, _names: &[String]) {}

    fn on_join(&self, guild: u64, output: OutputHandle) {
        output.set_muted(self.muted.load(Ordering::Acquire));
        self.outputs
            .lock()
            .expect("outputs lock")
            .insert(guild, output);
    }

    fn on_leave(&self, guild: u64) {
        self.outputs.lock().expect("outputs lock").remove(&guild);
    }

    fn assistant_state(&self, guild: u64) -> AssistantState {
        if !self
            .outputs
            .lock()
            .expect("outputs lock")
            .contains_key(&guild)
        {
            AssistantState::Off
        } else if self.muted.load(Ordering::Acquire) {
            AssistantState::Muted
        } else {
            AssistantState::Listening
        }
    }

    fn toggle_mute(&self) -> bool {
        let muted = !self.muted.fetch_xor(true, Ordering::AcqRel);
        self.apply_mute(muted);
        muted
    }

    fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }
}

impl Host {
    fn apply_mute(&self, muted: bool) {
        for output in self.outputs.lock().expect("outputs lock").values() {
            output.set_muted(muted);
        }
        log::info!("mute {}", if muted { "on" } else { "off" });
        self.events.publish(AppEvent::MuteChanged { muted });
    }
}

pub struct Runtime {
    build_stamp: String,
    data_dir: PathBuf,
    store: SettingsStore,
    secrets: Arc<dyn SecretStore>,
    events: EventBus,
    handle: Handle,
    settings: RwLock<Settings>,
    muted: Arc<AtomicBool>,
    host: Arc<Host>,
    discord: Arc<DiscordRuntime>,
    secrets_set: Mutex<HashSet<SecretKey>>,
    connection_errors: Mutex<BTreeMap<String, String>>,
}

impl Runtime {
    pub fn new(
        build_stamp: impl Into<String>,
        data_dir: PathBuf,
        store: SettingsStore,
        secrets: Arc<dyn SecretStore>,
        events: EventBus,
        handle: Handle,
    ) -> Self {
        let muted = Arc::new(AtomicBool::new(false));
        let host = Arc::new(Host {
            muted: Arc::clone(&muted),
            events: events.clone(),
            outputs: Mutex::new(BTreeMap::new()),
        });
        let discord = Arc::new(DiscordRuntime::new(
            events.clone(),
            host.clone(),
            handle.clone(),
        ));
        Self {
            build_stamp: build_stamp.into(),
            data_dir,
            store,
            secrets,
            events,
            handle,
            settings: RwLock::new(Settings::default()),
            muted,
            host,
            discord,
            secrets_set: Mutex::new(HashSet::new()),
            connection_errors: Mutex::new(BTreeMap::new()),
        }
    }

    /// Loads settings (writing the defaults on first start), learns which secrets are set,
    /// and connects to Discord when a token is stored and auto-connect is on.
    pub fn start(&self) -> Result<(), RuntimeError> {
        self.store.cleanup_partial();
        let settings = match self.store.load()? {
            Some(settings) => {
                if let Err(error) = settings.validate() {
                    self.events.log(format!(
                        "settings file has a problem ({error}); fix it in Settings"
                    ));
                }
                settings
            }
            None => {
                let defaults = Settings::default();
                self.store.save(&defaults)?;
                self.events.log(format!(
                    "no settings file yet; wrote the defaults to {}",
                    self.store.path().display()
                ));
                defaults
            }
        };
        *self.settings.write().expect("settings lock") = settings;

        for key in SecretKey::ALL {
            match self.secrets.get(*key) {
                Ok(Some(_)) => {
                    self.secrets_set.lock().expect("secrets lock").insert(*key);
                }
                Ok(None) => {}
                Err(error) => {
                    self.set_connection_error("secret_store", Some(error.to_string()));
                    log::error!("secret store: {error}");
                }
            }
        }
        log::info!("runtime started; data dir {}", self.data_dir.display());

        if self.settings().discord.auto_connect && self.is_secret_set(SecretKey::DiscordToken) {
            if let Err(error) = self.connect() {
                self.set_connection_error("discord", Some(error.to_string()));
            }
        }
        Ok(())
    }

    /// Stops Discord; waits for it so the process can exit cleanly.
    pub fn shutdown(&self) {
        let discord = Arc::clone(&self.discord);
        if discord.is_running() {
            let _ = self.handle.block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(10), discord.stop()).await
            });
        }
        log::info!("runtime stopped");
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().expect("settings lock").clone()
    }

    /// Validates and stores a patched document, then applies the changed sections.
    pub fn save_settings(&self, patch: &Value) -> Result<Settings, RuntimeError> {
        let current = self.settings();
        let next = current.with_patch(patch)?;
        next.validate()?;
        self.store.save(&next)?;
        let changed = current.changed_sections(&next);
        *self.settings.write().expect("settings lock") = next.clone();
        self.apply(&changed);
        self.events.publish(AppEvent::SettingsChanged);
        Ok(next)
    }

    /// Reconfigures only the subsystems whose section changed.
    fn apply(&self, changed: &[&'static str]) {
        for section in changed {
            match *section {
                "discord" => {
                    log::info!(
                        "settings: discord changed; the guild id applies at the next connection"
                    );
                }
                other => log::info!("settings: {other} changed"),
            }
        }
    }

    pub fn set_secret(&self, name: &str, value: &str) -> Result<(), RuntimeError> {
        let key = SecretKey::from_name(name)
            .ok_or_else(|| RuntimeError::UnknownSecret(name.to_owned()))?;
        self.secrets.set(key, value.trim())?;
        self.secrets_set.lock().expect("secrets lock").insert(key);
        self.set_connection_error("secret_store", None);
        log::info!("secret {} set", key.name());
        self.events.publish(AppEvent::SecretChanged {
            name: key.name().to_owned(),
            is_set: true,
        });
        if key == SecretKey::DiscordToken {
            self.reconnect();
        }
        Ok(())
    }

    pub fn clear_secret(&self, name: &str) -> Result<(), RuntimeError> {
        let key = SecretKey::from_name(name)
            .ok_or_else(|| RuntimeError::UnknownSecret(name.to_owned()))?;
        self.secrets.delete(key)?;
        self.secrets_set.lock().expect("secrets lock").remove(&key);
        log::info!("secret {} cleared", key.name());
        self.events.publish(AppEvent::SecretChanged {
            name: key.name().to_owned(),
            is_set: false,
        });
        if key == SecretKey::DiscordToken {
            self.disconnect();
        }
        Ok(())
    }

    /// The value itself, for the one consumer that needs it. Never logged or serialized.
    pub fn secret(&self, key: SecretKey) -> Result<Option<String>, RuntimeError> {
        Ok(self.secrets.get(key)?)
    }

    pub fn is_secret_set(&self, key: SecretKey) -> bool {
        self.secrets_set
            .lock()
            .expect("secrets lock")
            .contains(&key)
    }

    /// Starts the Discord client with the stored token.
    pub fn connect(&self) -> Result<(), RuntimeError> {
        let token = self
            .secret(SecretKey::DiscordToken)?
            .ok_or(DiscordError::NoToken)?;
        let guild = self.settings().discord.guild_id_value().map(GuildId::new);
        self.discord.start(token, guild)?;
        self.set_connection_error("discord", None);
        Ok(())
    }

    /// Stops the Discord client in the background.
    pub fn disconnect(&self) {
        let discord = Arc::clone(&self.discord);
        self.handle.spawn(async move {
            discord.stop().await;
        });
    }

    /// Stops, then starts again with the current token. Used when the token changes.
    fn reconnect(&self) {
        let discord = Arc::clone(&self.discord);
        let token = self.secret(SecretKey::DiscordToken).ok().flatten();
        let guild = self.settings().discord.guild_id_value().map(GuildId::new);
        let events = self.events.clone();
        self.handle.spawn(async move {
            discord.stop().await;
            if let Some(token) = token {
                if let Err(error) = discord.start(token, guild) {
                    events.log(format!("Discord: could not reconnect: {error}"));
                }
            }
        });
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    pub fn set_muted(&self, muted: bool) -> bool {
        let previous = self.muted.swap(muted, Ordering::AcqRel);
        if previous != muted {
            self.host.apply_mute(muted);
            self.discord.mark_panels_dirty();
        }
        muted
    }

    /// The single entry for all three mute sources (app button, hotkey, Discord panel).
    pub fn toggle_mute(&self) -> bool {
        self.set_muted(!self.is_muted())
    }

    pub fn set_connection_error(&self, source: &str, message: Option<String>) {
        let mut errors = self.connection_errors.lock().expect("errors lock");
        match &message {
            Some(text) => {
                errors.insert(source.to_owned(), text.clone());
            }
            None => {
                errors.remove(source);
            }
        }
        drop(errors);
        self.events.publish(AppEvent::ConnectionError {
            source: source.to_owned(),
            message,
        });
    }

    pub fn status(&self) -> Status {
        let set = self.secrets_set.lock().expect("secrets lock").clone();
        Status {
            build_stamp: self.build_stamp.clone(),
            data_dir: self.data_dir.display().to_string(),
            muted: self.is_muted(),
            secrets: SecretKey::ALL
                .iter()
                .map(|key| {
                    (
                        key.name().to_owned(),
                        SecretStatus {
                            is_set: set.contains(key),
                        },
                    )
                })
                .collect(),
            connection_errors: self.connection_errors.lock().expect("errors lock").clone(),
            discord: self.discord.status(),
            settings: self.settings(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use serde_json::json;

    fn runtime(dir: &tempfile::TempDir, tokio_rt: &tokio::runtime::Runtime) -> Runtime {
        Runtime::new(
            "0.1.0+test 2026-11-09",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            EventBus::new(16),
            tokio_rt.handle().clone(),
        )
    }

    #[test]
    fn first_start_writes_defaults_and_status_has_no_secret_values() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        assert!(dir.path().join("settings.json").exists());
        // A nonsense token: the connect attempt fails in the background; the secret path
        // itself must still behave.
        runtime
            .set_secret("discord_token", "  super-secret-token ")
            .unwrap();
        let status = runtime.status();
        assert!(status.secrets["discord_token"].is_set);
        let json = serde_json::to_string(&status).unwrap();
        assert!(
            !json.contains("super-secret-token"),
            "status must never carry the value"
        );
        assert_eq!(
            runtime.secret(SecretKey::DiscordToken).unwrap().as_deref(),
            Some("super-secret-token"),
            "the consumer gets the trimmed value"
        );
        runtime.clear_secret("discord_token").unwrap();
        assert!(!runtime.status().secrets["discord_token"].is_set);
        runtime.shutdown();
    }

    #[test]
    fn saving_a_patch_validates_persists_and_reports_the_changed_sections() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        let mut events = runtime.events().subscribe();
        let saved = runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 120 } }))
            .unwrap();
        assert_eq!(saved.session.idle_timeout_seconds, 120);
        assert_eq!(runtime.settings().session.idle_timeout_seconds, 120);
        let on_disk = SettingsStore::new(dir.path().join("settings.json"))
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(on_disk.session.idle_timeout_seconds, 120);
        let event = events.try_recv().unwrap();
        assert!(matches!(event, AppEvent::SettingsChanged));

        let error = runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 1 } }))
            .unwrap_err();
        assert!(error.to_string().contains("session.idle_timeout_seconds"));
        assert_eq!(
            runtime.settings().session.idle_timeout_seconds,
            120,
            "rejected save changes nothing"
        );
    }

    #[test]
    fn mute_is_one_state_with_an_event_per_change() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        let mut events = runtime.events().subscribe();
        assert!(runtime.toggle_mute());
        assert!(!runtime.toggle_mute());
        runtime.set_muted(false);
        assert!(matches!(
            events.try_recv(),
            Ok(AppEvent::MuteChanged { muted: true })
        ));
        assert!(matches!(
            events.try_recv(),
            Ok(AppEvent::MuteChanged { muted: false })
        ));
        assert!(
            events.try_recv().is_err(),
            "no event when the state did not change"
        );
    }

    #[test]
    fn connect_without_a_token_is_refused() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        assert!(matches!(
            runtime.connect(),
            Err(RuntimeError::Discord(DiscordError::NoToken))
        ));
    }

    #[test]
    fn unknown_secret_names_are_rejected() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        assert!(matches!(
            runtime.set_secret("password", "x"),
            Err(RuntimeError::UnknownSecret(_))
        ));
    }
}
