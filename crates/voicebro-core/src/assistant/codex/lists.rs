//! The Codex lists saved from the last read (change `config-tab-and-codex-lists`): the
//! models with their reasoning efforts, the voices and the default voice, with the time of
//! the read. Kept in `codex-lists.json` next to the executable so the window offers them at
//! start, before any Codex process runs. Written after every successful `model/list`.

use super::account::Voices;
use crate::assistant::ModelInfo;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file format this build writes.
pub const LISTS_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedLists {
    pub version: u32,
    /// Unix milliseconds of the read.
    pub read_at_ms: u64,
    /// The Codex version that answered, for the log; not used otherwise.
    pub codex_version: Option<String>,
    pub models: Vec<ModelInfo>,
    pub voices: Vec<String>,
    pub default_voice: Option<String>,
}

impl SavedLists {
    pub fn new(
        read_at_ms: u64,
        codex_version: Option<String>,
        models: Vec<ModelInfo>,
        voices: Option<&Voices>,
    ) -> Self {
        Self {
            version: LISTS_VERSION,
            read_at_ms,
            codex_version,
            models,
            voices: voices.map(|voices| voices.all.clone()).unwrap_or_default(),
            default_voice: voices.and_then(|voices| voices.default.clone()),
        }
    }

    /// The voices as the call reads them; `None` when the read had none.
    pub fn voices(&self) -> Option<Voices> {
        (!self.voices.is_empty() || self.default_voice.is_some()).then(|| Voices {
            all: self.voices.clone(),
            default: self.default_voice.clone(),
        })
    }
}

/// The saved lists: `Ok(None)` without a file, `Err` with a message naming the path when
/// the file cannot be read or parsed (the caller logs it once the log is up; the next read
/// replaces the file).
pub fn load(path: &Path) -> Result<Option<SavedLists>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Codex lists: could not read {}: {error}; the lists are read again from Codex",
                path.display()
            ))
        }
    };
    serde_json::from_str::<SavedLists>(&text)
        .map(Some)
        .map_err(|error| {
            format!(
                "Codex lists: {} is not a lists file ({error}); the lists are read again from Codex",
                path.display()
            )
        })
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "codex-lists.json".into());
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes `<file>.tmp` next to the file, then renames it over the old one, so the file is
/// always complete; a leftover temporary file from an interrupted write is overwritten.
pub fn save(path: &Path, lists: &SavedLists) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(lists).map_err(std::io::Error::other)?;
    let temp = temp_path(path);
    std::fs::write(&temp, text)?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, efforts: &[&str], is_default: bool) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            display_name: id.to_uppercase(),
            description: String::new(),
            reasoning_efforts: efforts.iter().map(|e| (*e).to_owned()).collect(),
            default_reasoning_effort: efforts.first().map(|e| (*e).to_owned()),
            is_default,
        }
    }

    fn sample() -> SavedLists {
        SavedLists::new(
            1_763_141_400_000,
            Some("0.167.0".to_owned()),
            vec![
                model("gpt-6-luna", &["low", "medium", "high"], true),
                model("gpt-6-sol", &["low"], false),
            ],
            Some(&Voices {
                all: vec!["juniper".to_owned(), "cove".to_owned()],
                default: Some("juniper".to_owned()),
            }),
        )
    }

    #[test]
    fn a_saved_file_loads_as_it_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex-lists.json");
        let lists = sample();
        save(&path, &lists).unwrap();
        assert_eq!(load(&path), Ok(Some(lists.clone())));
        assert_eq!(lists.version, LISTS_VERSION);
        let voices = lists.voices().unwrap();
        assert_eq!(voices.all, ["juniper", "cove"]);
        assert_eq!(voices.default.as_deref(), Some("juniper"));
        assert!(!temp_path(&path).exists(), "no temporary file is left");
    }

    #[test]
    fn a_missing_file_is_no_lists() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("codex-lists.json")), Ok(None));
    }

    #[test]
    fn a_broken_file_is_an_error_naming_it_and_the_next_save_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex-lists.json");
        std::fs::write(&path, "{ not json").unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.contains("codex-lists.json"), "{error}");
        assert!(error.contains("read again"), "{error}");
        std::fs::write(&path, r#"{ "models": 3 }"#).unwrap();
        assert!(load(&path).is_err(), "valid JSON of the wrong shape");
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path), Ok(Some(sample())));
    }

    #[test]
    fn a_leftover_temporary_file_does_not_block_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex-lists.json");
        std::fs::write(temp_path(&path), "half a file").unwrap();
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path), Ok(Some(sample())));
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn a_read_without_voices_saves_none() {
        let lists = SavedLists::new(1, None, vec![model("m", &[], true)], None);
        assert_eq!(lists.voices(), None);
        assert!(lists.voices.is_empty() && lists.default_voice.is_none());
    }
}
