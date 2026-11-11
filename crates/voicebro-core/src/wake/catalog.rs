//! Vetted wake-word models. Settings refer to a model by its `id`; the engine that loads it
//! is named by `engine`. The catalog describes models and where to get them; the phrases
//! to spot are the user's and live in the settings file.

/// A downloadable archive with a pinned hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub label: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    /// Directory the archive extracts to, relative to the assets directory.
    pub directory: &'static str,
    /// A file inside `directory` that proves a completed extraction.
    pub marker: &'static str,
    /// Expected size in bytes, for progress reporting.
    pub size: u64,
}

/// One model the app knows how to download, verify and load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: &'static str,
    pub engine: &'static str,
    /// BCP 47 language tag of the speech the model recognizes.
    pub language: &'static str,
    /// The name without the size: the window shows the size from `asset.size`.
    pub display_name: &'static str,
    pub license: &'static str,
    pub asset: Asset,
}

pub const VOSK_ENGINE: &str = "vosk";

/// The model of a settings file from schema version 1, which had only this one.
pub const RUSSIAN_SMALL: &str = "vosk-model-small-ru-0.22";

/// The first entry is the default of a new settings file.
const ENTRIES: &[ModelEntry] = &[
    ModelEntry {
        id: "vosk-model-small-en-us-0.15",
        engine: VOSK_ENGINE,
        language: "en-US",
        display_name: "Vosk small English (US) 0.15",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk English model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip",
            sha256: "30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498",
            directory: "vosk-model-small-en-us-0.15",
            marker: "am/final.mdl",
            size: 41_205_931,
        },
    },
    ModelEntry {
        id: RUSSIAN_SMALL,
        engine: VOSK_ENGINE,
        language: "ru",
        display_name: "Vosk small Russian 0.22",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk Russian model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-ru-0.22.zip",
            sha256: "961d5ff98a17f4aa6de69864d0aa71fa5bac682301d2b5d17a3f24c5c99a46d4",
            directory: "vosk-model-small-ru-0.22",
            marker: "am/final.mdl",
            size: 46_220_000,
        },
    },
];

pub fn entries() -> &'static [ModelEntry] {
    ENTRIES
}

pub fn find(id: &str) -> Option<&'static ModelEntry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

/// The model the settings start with.
pub fn default_entry() -> &'static ModelEntry {
    &ENTRIES[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_model_is_in_the_catalog_and_its_engine_is_vosk() {
        let entry = default_entry();
        assert_eq!(find(entry.id), Some(entry));
        assert_eq!(entry.engine, VOSK_ENGINE);
        assert!(find("nope").is_none());
    }

    #[test]
    fn english_is_the_default_and_russian_stays_in_the_catalog() {
        assert_eq!(default_entry().id, "vosk-model-small-en-us-0.15");
        assert_eq!(default_entry().language, "en-US");
        let russian = find(RUSSIAN_SMALL).expect("the Russian model");
        assert_eq!(russian.language, "ru");
        assert_eq!(entries().len(), 2);
    }

    #[test]
    fn every_entry_has_a_pinned_hash() {
        for entry in entries() {
            assert_eq!(entry.asset.sha256.len(), 64, "{}", entry.id);
            assert!(entry.asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(entry.asset.url.starts_with("https://"));
            assert!(entry.asset.size > 0);
        }
    }
}
