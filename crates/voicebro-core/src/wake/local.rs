//! Vosk models the user extracted into the models folder (`<data dir>\models`). The app
//! lists them at start, loads them like catalog models and never downloads, changes or
//! deletes them. Their ids are `local:<folder>`.

use crate::settings::{local_model_folder, LOCAL_MODEL_PREFIX};
use serde::Serialize;
use std::path::Path;

/// A model folder found in the models folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalModel {
    /// The folder name inside the models folder.
    pub folder: String,
    /// The language from a Vosk folder name (`de`, `en-us`), or empty.
    pub language: String,
}

impl LocalModel {
    /// The id the settings use for this model.
    pub fn id(&self) -> String {
        format!("{LOCAL_MODEL_PREFIX}{}", self.folder)
    }
}

/// Whether `dir` holds a Vosk model: the acoustic model and the feature configuration.
pub fn is_model_dir(dir: &Path) -> bool {
    dir.join("am").join("final.mdl").is_file() && dir.join("conf").join("mfcc.conf").is_file()
}

/// The language in a Vosk model folder name: the `xx` or `xx-yy` after `vosk-model-` and
/// an optional `small-` (`vosk-model-small-en-us-0.15` → `en-us`). Empty for other names.
pub fn language_of(folder: &str) -> String {
    let Some(rest) = folder.strip_prefix("vosk-model-") else {
        return String::new();
    };
    let rest = rest.strip_prefix("small-").unwrap_or(rest);
    let mut parts = rest.split('-');
    let is_letters = |part: &str, lengths: std::ops::RangeInclusive<usize>| {
        lengths.contains(&part.len()) && part.chars().all(|c| c.is_ascii_lowercase())
    };
    match parts.next() {
        Some(first) if is_letters(first, 2..=3) => match parts.next() {
            Some(second) if is_letters(second, 2..=2) => format!("{first}-{second}"),
            _ => first.to_owned(),
        },
        _ => String::new(),
    }
}

/// Lists the model folders in `models_dir`, sorted by name. A folder that is not a model,
/// or whose name cannot be a `local:` id, is skipped with a log line.
pub fn scan(models_dir: &Path) -> Vec<LocalModel> {
    let Ok(entries) = std::fs::read_dir(models_dir) else {
        return Vec::new();
    };
    let mut models = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(folder) = path.file_name().and_then(|name| name.to_str()) else {
            log::info!(
                "models folder: skipped {} (the name is not valid text)",
                path.display()
            );
            continue;
        };
        if local_model_folder(&format!("{LOCAL_MODEL_PREFIX}{folder}")).is_none() {
            log::info!("models folder: skipped «{folder}» (the name cannot be a model id)");
            continue;
        }
        if !is_model_dir(&path) {
            log::info!(
                "models folder: skipped «{folder}» (no am\\final.mdl and conf\\mfcc.conf: not a Vosk model)"
            );
            continue;
        }
        models.push(LocalModel {
            folder: folder.to_owned(),
            language: language_of(folder),
        });
    }
    models.sort_by(|a, b| a.folder.cmp(&b.folder));
    if !models.is_empty() {
        log::info!(
            "models folder: {} model(s): {}",
            models.len(),
            models
                .iter()
                .map(|model| model.folder.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_model(dir: &Path, folder: &str) {
        let model = dir.join(folder);
        std::fs::create_dir_all(model.join("am")).unwrap();
        std::fs::create_dir_all(model.join("conf")).unwrap();
        std::fs::write(model.join("am").join("final.mdl"), b"mdl").unwrap();
        std::fs::write(model.join("conf").join("mfcc.conf"), b"conf").unwrap();
    }

    #[test]
    fn a_model_folder_is_listed_with_its_language() {
        let dir = tempfile::tempdir().unwrap();
        make_model(dir.path(), "vosk-model-small-de-0.15");
        make_model(dir.path(), "my-model");
        let models = scan(dir.path());
        assert_eq!(
            models,
            vec![
                LocalModel {
                    folder: "my-model".to_owned(),
                    language: String::new(),
                },
                LocalModel {
                    folder: "vosk-model-small-de-0.15".to_owned(),
                    language: "de".to_owned(),
                },
            ]
        );
        assert_eq!(models[1].id(), "local:vosk-model-small-de-0.15");
    }

    #[test]
    fn folders_that_are_not_models_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("half").join("conf")).unwrap();
        std::fs::write(dir.path().join("half").join("conf").join("mfcc.conf"), b"").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"a file, not a folder").unwrap();
        make_model(dir.path(), "a..b");
        assert!(scan(dir.path()).is_empty());
    }

    #[test]
    fn an_empty_or_missing_folder_lists_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan(dir.path()).is_empty());
        assert!(scan(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn the_language_comes_from_vosk_folder_names() {
        for (folder, language) in [
            ("vosk-model-small-en-us-0.15", "en-us"),
            ("vosk-model-small-de-0.15", "de"),
            ("vosk-model-small-cs-0.4-rhasspy", "cs"),
            ("vosk-model-small-fr-pguyot-0.3", "fr"),
            ("vosk-model-small-en-us-zamia-0.5", "en-us"),
            ("vosk-model-ru-0.42", "ru"),
            ("vosk-model-small-0.1", ""),
            ("my-german-model", ""),
        ] {
            assert_eq!(language_of(folder), language, "{folder}");
        }
    }
}
