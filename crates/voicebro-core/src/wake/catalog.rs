//! Vetted wake-word models. Settings refer to a model by its `id`; the engine that loads it
//! is named by `engine`. The catalog describes models only; the phrases to spot are the
//! user's and live in the settings file.

/// One model the app knows how to download, verify and load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: &'static str,
    pub engine: &'static str,
    /// BCP 47 language tag of the speech the model recognizes.
    pub language: &'static str,
    pub display_name: &'static str,
    pub license: &'static str,
}

pub const VOSK_ENGINE: &str = "vosk";

const ENTRIES: &[ModelEntry] = &[ModelEntry {
    id: "vosk-model-small-ru-0.22",
    engine: VOSK_ENGINE,
    language: "ru",
    display_name: "Vosk small Russian 0.22 (45 MB)",
    license: "Apache-2.0",
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
}
