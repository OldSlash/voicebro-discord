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
    // The rest by English language name. Sizes and hashes were measured from a download on
    // 2026-11-11; alphacephei publishes neither.
    ModelEntry {
        id: "vosk-model-small-cs-0.4-rhasspy",
        engine: VOSK_ENGINE,
        language: "cs",
        display_name: "Vosk small Czech 0.4",
        license: "MIT",
        asset: Asset {
            label: "Vosk Czech model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-cs-0.4-rhasspy.zip",
            sha256: "287c3bbefc8ad67b4ab9636eecef3d62acc3719990777d03e226db5a7f19fbda",
            directory: "vosk-model-small-cs-0.4-rhasspy",
            marker: "am/final.mdl",
            size: 46_088_666,
        },
    },
    ModelEntry {
        id: "vosk-model-small-nl-0.22",
        engine: VOSK_ENGINE,
        language: "nl",
        display_name: "Vosk small Dutch 0.22",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk Dutch model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-nl-0.22.zip",
            sha256: "039811c3b829de64e4f123a9f684a53784005b212a346ac0b899dc7efce2ed0a",
            directory: "vosk-model-small-nl-0.22",
            marker: "am/final.mdl",
            size: 40_441_176,
        },
    },
    ModelEntry {
        id: "vosk-model-small-fr-0.22",
        engine: VOSK_ENGINE,
        language: "fr",
        display_name: "Vosk small French 0.22",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk French model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-fr-0.22.zip",
            sha256: "cabf6180e177eb9b3a9a9d43a437bd5e549f3a7d09525e5d69a3fed787be12ad",
            directory: "vosk-model-small-fr-0.22",
            marker: "am/final.mdl",
            size: 42_233_323,
        },
    },
    ModelEntry {
        id: "vosk-model-small-de-0.15",
        engine: VOSK_ENGINE,
        language: "de",
        display_name: "Vosk small German 0.15",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk German model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-de-0.15.zip",
            sha256: "b7e53c90b1f0a38456f4cd62b366ecd58803cd97cd42b06438e2c131713d5e43",
            directory: "vosk-model-small-de-0.15",
            marker: "am/final.mdl",
            size: 46_499_967,
        },
    },
    ModelEntry {
        id: "vosk-model-small-it-0.22",
        engine: VOSK_ENGINE,
        language: "it",
        display_name: "Vosk small Italian 0.22",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk Italian model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-it-0.22.zip",
            sha256: "9ec65e75861d1c6c2e457cccd932705340dcdf233f5b239f00733b4de0bf3267",
            directory: "vosk-model-small-it-0.22",
            marker: "am/final.mdl",
            size: 49_665_141,
        },
    },
    ModelEntry {
        id: "vosk-model-small-pl-0.22",
        engine: VOSK_ENGINE,
        language: "pl",
        display_name: "Vosk small Polish 0.22",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk Polish model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-pl-0.22.zip",
            sha256: "c4cd16498ea544f446f9e9a55cbd602b71cfe5a2b6f2b0834d81e1b6fce15f0d",
            directory: "vosk-model-small-pl-0.22",
            marker: "am/final.mdl",
            size: 52_979_372,
        },
    },
    ModelEntry {
        id: "vosk-model-small-pt-0.3",
        engine: VOSK_ENGINE,
        language: "pt-BR",
        display_name: "Vosk small Portuguese (Brazil) 0.3",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk Portuguese (Brazil) model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-pt-0.3.zip",
            sha256: "6e1ce909032e1afa7a88e68a3d628ecafff302bdf195befab308826c395e93b7",
            directory: "vosk-model-small-pt-0.3",
            // Vosk's older flat layout: the model files sit in the folder itself.
            marker: "final.mdl",
            size: 32_453_112,
        },
    },
    ModelEntry {
        id: "vosk-model-small-es-0.42",
        engine: VOSK_ENGINE,
        language: "es",
        display_name: "Vosk small Spanish 0.42",
        license: "Apache-2.0",
        asset: Asset {
            label: "Vosk Spanish model",
            url: "https://alphacephei.com/vosk/models/vosk-model-small-es-0.42.zip",
            sha256: "09b239888f633ef2f0b4e09736e3d9936acfd810bc65d53fad45261762c6511f",
            directory: "vosk-model-small-es-0.42",
            marker: "am/final.mdl",
            size: 39_817_833,
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
    }

    #[test]
    fn ten_models_english_and_russian_first_then_by_language_name() {
        let ids: Vec<&str> = entries().iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            [
                "vosk-model-small-en-us-0.15",
                "vosk-model-small-ru-0.22",
                "vosk-model-small-cs-0.4-rhasspy",
                "vosk-model-small-nl-0.22",
                "vosk-model-small-fr-0.22",
                "vosk-model-small-de-0.15",
                "vosk-model-small-it-0.22",
                "vosk-model-small-pl-0.22",
                "vosk-model-small-pt-0.3",
                "vosk-model-small-es-0.42",
            ]
        );
        let languages: Vec<&str> = entries().iter().map(|entry| entry.language).collect();
        assert_eq!(
            languages,
            ["en-US", "ru", "cs", "nl", "fr", "de", "it", "pl", "pt-BR", "es"]
        );
    }

    #[test]
    fn every_entry_has_a_pinned_hash() {
        for entry in entries() {
            assert_eq!(entry.asset.sha256.len(), 64, "{}", entry.id);
            assert!(entry.asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(entry.asset.size > 0);
        }
    }

    #[test]
    fn every_entry_downloads_its_own_archive_into_its_own_folder() {
        for entry in entries() {
            assert_eq!(entry.engine, VOSK_ENGINE, "{}", entry.id);
            assert_eq!(
                entry.asset.url,
                format!("https://alphacephei.com/vosk/models/{}.zip", entry.id)
            );
            assert_eq!(entry.asset.directory, entry.id);
            assert!(!entry.asset.marker.is_empty(), "{}", entry.id);
            assert!(
                ["Apache-2.0", "MIT"].contains(&entry.license),
                "{}",
                entry.id
            );
        }
        let mut ids: Vec<&str> = entries().iter().map(|entry| entry.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), entries().len(), "ids are unique");
    }
}
