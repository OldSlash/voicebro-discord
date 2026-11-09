//! The composition root. It owns the settings, the secret store and the event bus, and it
//! is the only place that knows every subsystem. Later changes add the Discord adapter,
//! the wake engine and the assistant here.

use crate::events::{AppEvent, EventBus};
use crate::secrets::{SecretError, SecretKey, SecretStore};
use crate::settings::store::{SettingsStore, StoreError};
use crate::settings::{Settings, ValidationError};
use crate::status::{SecretStatus, Status};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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
}

pub struct Runtime {
    build_stamp: String,
    data_dir: PathBuf,
    store: SettingsStore,
    secrets: Arc<dyn SecretStore>,
    events: EventBus,
    settings: RwLock<Settings>,
    muted: AtomicBool,
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
    ) -> Self {
        Self {
            build_stamp: build_stamp.into(),
            data_dir,
            store,
            secrets,
            events,
            settings: RwLock::new(Settings::default()),
            muted: AtomicBool::new(false),
            secrets_set: Mutex::new(HashSet::new()),
            connection_errors: Mutex::new(BTreeMap::new()),
        }
    }

    /// Loads settings (writing the defaults on first start) and learns which secrets are
    /// set. A failing secret store is reported in the status and the app keeps running.
    pub fn start(&self) -> Result<(), RuntimeError> {
        self.store.cleanup_partial();
        let settings = match self.store.load()? {
            Some(settings) => {
                if let Err(error) = settings.validate() {
                    // A document edited by hand can be invalid; keep running with it but say so.
                    self.events
                        .log(format!("settings file has a problem ({error}); fix it in Settings"));
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
        Ok(())
    }

    pub fn shutdown(&self) {
        log::info!("runtime stopped");
    }

    pub fn events(&self) -> &EventBus {
        &self.events
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

    /// Reconfigures only the subsystems whose section changed. Each later change adds its
    /// branch here; today every branch is a log line.
    fn apply(&self, changed: &[&'static str]) {
        for section in changed {
            match *section {
                "discord" => log::info!("settings: discord changed (connection policy applies at next start)"),
                "session" | "wake" | "assistant" | "codex" | "hotkey" => {
                    log::info!("settings: {section} changed")
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
        Ok(())
    }

    /// The value itself, for the one consumer that needs it. Never logged or serialized.
    pub fn secret(&self, key: SecretKey) -> Result<Option<String>, RuntimeError> {
        Ok(self.secrets.get(key)?)
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    pub fn set_muted(&self, muted: bool) -> bool {
        let previous = self.muted.swap(muted, Ordering::AcqRel);
        if previous != muted {
            log::info!("mute {}", if muted { "on" } else { "off" });
            self.events.publish(AppEvent::MuteChanged { muted });
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
            settings: self.settings(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use serde_json::json;

    fn runtime(dir: &tempfile::TempDir) -> Runtime {
        Runtime::new(
            "0.1.0+test 2026-11-09",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            EventBus::new(16),
        )
    }

    #[test]
    fn first_start_writes_defaults_and_status_has_no_secret_values() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir);
        runtime.start().unwrap();
        assert!(dir.path().join("settings.json").exists());
        runtime.set_secret("discord_token", "  super-secret-token ").unwrap();
        let status = runtime.status();
        assert!(status.secrets["discord_token"].is_set);
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("super-secret-token"), "status must never carry the value");
        assert_eq!(
            runtime.secret(SecretKey::DiscordToken).unwrap().as_deref(),
            Some("super-secret-token"),
            "the consumer gets the trimmed value"
        );
        runtime.clear_secret("discord_token").unwrap();
        assert!(!runtime.status().secrets["discord_token"].is_set);
    }

    #[test]
    fn saving_a_patch_validates_persists_and_reports_the_changed_sections() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir);
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
        assert_eq!(runtime.settings().session.idle_timeout_seconds, 120, "rejected save changes nothing");
    }

    #[test]
    fn mute_is_one_state_with_an_event_per_change() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir);
        let mut events = runtime.events().subscribe();
        assert!(runtime.toggle_mute());
        assert!(!runtime.toggle_mute());
        runtime.set_muted(false);
        assert_eq!(events.try_recv().ok().map(|e| matches!(e, AppEvent::MuteChanged { muted: true })), Some(true));
        assert_eq!(events.try_recv().ok().map(|e| matches!(e, AppEvent::MuteChanged { muted: false })), Some(true));
        assert!(events.try_recv().is_err(), "no event when the state did not change");
    }

    #[test]
    fn unknown_secret_names_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir);
        assert!(matches!(
            runtime.set_secret("password", "x"),
            Err(RuntimeError::UnknownSecret(_))
        ));
    }
}
