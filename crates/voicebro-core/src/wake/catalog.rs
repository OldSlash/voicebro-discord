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
    pub display_name: &'static str,
    pub license: &'static str,
    pub asset: Asset,
}

pub const VOSK_ENGINE: &str = "vosk";

const ENTRIES: &[ModelEntry] = &[ModelEntry {
    id: "vosk-model-small-ru-0.22",
    engine: VOSK_ENGINE,
    language: "ru",
    display_name: "Vosk small Russian 0.22 (45 MB)",
    license: "Apache-2.0",
    asset: Asset {
        label: "Vosk Russian model",
        url: "https://alphacephei.com/vosk/models/vosk-model-small-ru-0.22.zip",
        sha256: "961d5ff98a17f4aa6de69864d0aa71fa5bac682301d2b5d17a3f24c5c99a46d4",
        directory: "vosk-model-small-ru-0.22",
        marker: "am/final.mdl",
        size: 46_220_000,
    },
}];

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
    fn every_entry_has_a_pinned_hash() {
        for entry in entries() {
            assert_eq!(entry.asset.sha256.len(), 64, "{}", entry.id);
            assert!(entry.asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(entry.asset.url.starts_with("https://"));
            assert!(entry.asset.size > 0);
        }
    }
}
