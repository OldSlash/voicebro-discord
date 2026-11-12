//! The settings file: load with migration, save through a temporary file and a rename.

use super::{Settings, CURRENT_VERSION};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a settings document: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn temp_path(&self) -> PathBuf {
        let mut name = self
            .path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| "settings.json".into());
        name.push(".tmp");
        self.path.with_file_name(name)
    }

    /// Removes a temporary file left by an interrupted save. Called at start.
    pub fn cleanup_partial(&self) {
        let temp = self.temp_path();
        if temp.exists() {
            if let Err(error) = std::fs::remove_file(&temp) {
                log::warn!("could not remove {}: {error}", temp.display());
            } else {
                log::info!("removed a partial settings file from an interrupted save");
            }
        }
    }

    /// `Ok(None)` when no file exists yet. An older document gets its missing fields from
    /// the defaults and its version bumped; the caller decides whether to write it back.
    pub fn load(&self) -> Result<Option<Settings>, StoreError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(StoreError::Read {
                    path: self.path.clone(),
                    source,
                })
            }
        };
        let mut value: Value = serde_json::from_str(&text).map_err(|source| StoreError::Parse {
            path: self.path.clone(),
            source,
        })?;
        let stored_version = value.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
        if stored_version < 2 {
            migrate_v1(&mut value);
        }
        let mut settings: Settings =
            serde_json::from_value(value).map_err(|source| StoreError::Parse {
                path: self.path.clone(),
                source,
            })?;
        if stored_version < CURRENT_VERSION {
            log::info!(
                "settings document version {stored_version} loaded with version {CURRENT_VERSION} defaults"
            );
            settings.version = CURRENT_VERSION;
        }
        if let Some(stored) = settings.diagnostics.replace_unknown_codec() {
            log::warn!(
                "settings: the stored Opus codec {stored} is not in this build; libopus is used"
            );
        }
        Ok(Some(settings))
    }

    /// Writes to `<file>.tmp` in the same directory, then renames over the old file, so the
    /// settings file is always a complete document.
    pub fn save(&self, settings: &Settings) -> Result<(), StoreError> {
        let write_error = |source| StoreError::Write {
            path: self.path.clone(),
            source,
        };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(write_error)?;
        }
        let text = serde_json::to_string_pretty(settings).expect("settings serialize");
        let temp = self.temp_path();
        std::fs::write(&temp, text).map_err(write_error)?;
        if let Err(source) = std::fs::rename(&temp, &self.path) {
            let _ = std::fs::remove_file(&temp);
            return Err(write_error(source));
        }
        Ok(())
    }
}

/// Version 1 kept one pair of phrase lists for whatever model was selected, and its only
/// model was the Russian one, so the model it names says nothing about the user's choice.
/// With phrases, moves the lists under `wake.phrases.<model>` (the Russian model when the
/// document names none) so they keep working. Without phrases, selects the default model
/// of a new file, English. Removes the old fields either way.
pub fn migrate_v1(document: &mut Value) {
    let Some(root) = document.as_object_mut() else {
        return;
    };
    let wake = root
        .entry("wake")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(wake) = wake.as_object_mut() else {
        return;
    };
    let take_list = |wake: &mut serde_json::Map<String, Value>, key: &str| {
        wake.remove(key)
            .filter(|list| list.as_array().is_some_and(|items| !items.is_empty()))
    };
    let activation = take_list(wake, "activation_phrases");
    let deactivation = take_list(wake, "deactivation_phrases");
    if activation.is_none() && deactivation.is_none() {
        let default = crate::wake::catalog::default_entry().id;
        wake.insert("model".to_owned(), Value::String(default.to_owned()));
        log::info!("settings: a version-1 file without wake phrases now selects {default}");
        return;
    }
    let model = match wake.get("model").and_then(Value::as_str) {
        Some(model) => model.to_owned(),
        None => {
            let model = crate::wake::catalog::RUSSIAN_SMALL.to_owned();
            wake.insert("model".to_owned(), Value::String(model.clone()));
            model
        }
    };
    let phrases = wake
        .entry("phrases")
        .or_insert_with(|| Value::Object(Default::default()));
    if let Some(phrases) = phrases.as_object_mut() {
        let entry = phrases
            .entry(model)
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(entry) = entry.as_object_mut() {
            if let Some(list) = activation {
                entry.insert("activation".to_owned(), list);
            }
            if let Some(list) = deactivation {
                entry.insert("deactivation".to_owned(), list);
            }
        }
    }
    log::info!("settings: the wake phrases moved under the model they were set for");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::catalog::RUSSIAN_SMALL;

    fn store_in(dir: &tempfile::TempDir) -> SettingsStore {
        SettingsStore::new(dir.path().join("settings.json"))
    }

    #[test]
    fn version_1_phrases_move_under_their_model_and_the_old_fields_go() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(
            store.path(),
            r#"{ "version": 1, "wake": { "engine": "vosk", "model": "vosk-model-small-ru-0.22",
                 "activation_phrases": ["братан"], "deactivation_phrases": ["спасибо братан"] } }"#,
        )
        .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.wake.model, RUSSIAN_SMALL);
        assert_eq!(loaded.wake.selected_phrases().activation, ["братан"]);
        assert_eq!(
            loaded.wake.selected_phrases().deactivation,
            ["спасибо братан"]
        );
        store.save(&loaded).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(!text.contains("activation_phrases"), "{text}");
        assert!(!text.contains("deactivation_phrases"), "{text}");
        assert!(text.contains("\"version\": 2"), "{text}");
        let again = store.load().unwrap().unwrap();
        assert_eq!(again, loaded, "a version-2 file loads unchanged");
    }

    #[test]
    fn version_1_without_a_model_gets_the_russian_model() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(
            store.path(),
            r#"{ "version": 1, "wake": { "activation_phrases": ["братан"] } }"#,
        )
        .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.wake.model, RUSSIAN_SMALL);
        assert_eq!(loaded.wake.selected_phrases().activation, ["братан"]);
        assert!(loaded.wake.selected_phrases().deactivation.is_empty());
    }

    #[test]
    fn version_1_without_phrases_selects_english() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        for document in [
            r#"{ "version": 1, "session": {} }"#,
            r#"{ "version": 1, "wake": { "model": "vosk-model-small-ru-0.22" } }"#,
            r#"{ "version": 1, "wake": { "model": "vosk-model-small-ru-0.22",
                 "activation_phrases": [], "deactivation_phrases": [] } }"#,
        ] {
            std::fs::write(store.path(), document).unwrap();
            let loaded = store.load().unwrap().unwrap();
            assert_eq!(
                loaded.wake.model, "vosk-model-small-en-us-0.15",
                "{document}"
            );
            assert!(loaded.wake.phrases.is_empty(), "{document}");
            store.save(&loaded).unwrap();
            let text = std::fs::read_to_string(store.path()).unwrap();
            assert!(!text.contains("activation_phrases"), "{text}");
            assert!(!text.contains("vosk-model-small-ru-0.22"), "{text}");
        }
    }

    /// The file the 0.1.0 build wrote on 2026-11-09: its defaults (commit `8b42a5e`), which
    /// the user's later install picked up with the Russian model selected.
    #[test]
    fn the_0_1_0_defaults_load_with_english_and_keep_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(
            store.path(),
            include_str!("../../testdata/settings-v1-0.1.0.json"),
        )
        .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.wake.engine, "vosk");
        assert_eq!(loaded.wake.model, "vosk-model-small-en-us-0.15");
        assert!(loaded.wake.phrases.is_empty());
        // The file's own values stay, also where today's defaults differ (the voice).
        assert_eq!(loaded.session.mode, crate::settings::SessionMode::WakeWord);
        assert_eq!(loaded.session.idle_timeout_seconds, 60);
        assert!(loaded.session.earcons);
        assert_eq!(loaded.session.earcon_volume, 0.18);
        assert_eq!(loaded.assistant.engine, "codex");
        assert_eq!(loaded.codex.voice, "juniper");
        assert!(loaded.codex.instructions.is_empty());
        assert_eq!(loaded.hotkey.mute, "Ctrl+Alt+M");
        assert!(loaded.validate().is_ok(), "{:?}", loaded.validate());
    }

    #[test]
    fn version_1_with_only_deactivation_phrases_keeps_the_russian_model() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(
            store.path(),
            r#"{ "version": 1, "wake": { "model": "vosk-model-small-ru-0.22",
                 "activation_phrases": [], "deactivation_phrases": ["отбой"] } }"#,
        )
        .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.wake.model, RUSSIAN_SMALL);
        assert_eq!(loaded.wake.selected_phrases().deactivation, ["отбой"]);
    }

    #[test]
    fn a_new_file_selects_english() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.save(&Settings::default()).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.wake.model, "vosk-model-small-en-us-0.15");
    }

    #[test]
    fn first_start_has_no_file_and_a_save_creates_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        assert!(store.load().unwrap().is_none());
        store.save(&Settings::default()).unwrap();
        let loaded = store.load().unwrap().expect("file exists now");
        assert_eq!(loaded, Settings::default());
        assert!(!store.temp_path().exists(), "no temp file left behind");
    }

    #[test]
    fn a_leftover_temp_file_is_removed_and_the_real_file_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.save(&Settings::default()).unwrap();
        std::fs::write(store.temp_path(), "{ partial").unwrap();
        store.cleanup_partial();
        assert!(!store.temp_path().exists());
        assert_eq!(store.load().unwrap().unwrap(), Settings::default());
    }

    #[test]
    fn an_older_version_loads_with_defaults_and_the_current_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(
            store.path(),
            r#"{ "version": 0, "codex": { "voice": "maple" }, "later": true }"#,
        )
        .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.version, CURRENT_VERSION);
        assert_eq!(loaded.codex.voice, "maple");
        assert_eq!(loaded.session.idle_timeout_seconds, 60);
        assert_eq!(loaded.extra["later"], true);
        store.save(&loaded).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(text.contains("\"later\": true"));
        assert!(text.contains(&format!("\"version\": {CURRENT_VERSION}")));
    }

    #[test]
    fn a_codec_from_the_four_codec_trial_loads_as_libopus() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(
            store.path(),
            r#"{ "version": 2, "diagnostics": { "memory": true, "opus_codec": "ropus" } }"#,
        )
        .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.diagnostics.opus_codec, "libopus");
        assert!(loaded.diagnostics.memory, "the rest of the section stays");
        loaded
            .validate()
            .expect("the loaded document passes validation");
        store.save(&loaded).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(text.contains("\"opus_codec\": \"libopus\""), "{text}");
    }

    #[test]
    fn a_corrupt_file_is_a_parse_error_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(store.path(), "not json").unwrap();
        let error = store.load().unwrap_err();
        assert!(matches!(error, StoreError::Parse { .. }));
        assert!(error.to_string().contains("settings.json"));
    }

    #[test]
    fn save_replaces_an_existing_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.save(&Settings::default()).unwrap();
        let mut changed = Settings::default();
        changed.session.idle_timeout_seconds = 300;
        store.save(&changed).unwrap();
        assert_eq!(
            store.load().unwrap().unwrap().session.idle_timeout_seconds,
            300
        );
    }
}
