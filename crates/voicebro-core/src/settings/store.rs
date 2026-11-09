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
        let value: Value = serde_json::from_str(&text).map_err(|source| StoreError::Parse {
            path: self.path.clone(),
            source,
        })?;
        let stored_version = value.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> SettingsStore {
        SettingsStore::new(dir.path().join("settings.json"))
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
        assert_eq!(store.load().unwrap().unwrap().session.idle_timeout_seconds, 300);
    }
}
