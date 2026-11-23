//! The voice engine, the phonemizer and the voices the app can download (change
//! `whisper-mode`, design D9b), each pinned by SHA-256. Windows' own voices are listed at
//! run time (`tts::windows`) and need no download.

use crate::assets::{Asset, AssetKind};

/// The voice engine's and the phonemizer's library files on this platform, dependencies
/// first (change `cross-platform-headless`, design D1; Linux not tested).
pub mod files {
    #[cfg(windows)]
    pub const ONNXRUNTIME: &str = "onnxruntime.dll";
    #[cfg(windows)]
    pub const SHERPA: &str = "sherpa-onnx-c-api.dll";
    #[cfg(windows)]
    pub const PHONEMIZER_CORE: &str = "piper_phonemize_core.dll";
    #[cfg(windows)]
    pub const PHONEMIZER_API: &str = "piper_phonemize_cxx_api.dll";

    #[cfg(not(windows))]
    pub const ONNXRUNTIME: &str = "libonnxruntime.so";
    #[cfg(not(windows))]
    pub const SHERPA: &str = "libsherpa-onnx-c-api.so";
    #[cfg(not(windows))]
    pub const PHONEMIZER_CORE: &str = "libpiper_phonemize_core.so";
    #[cfg(not(windows))]
    pub const PHONEMIZER_API: &str = "libpiper_phonemize_cxx_api.so";
}

/// sherpa-onnx v2.0.1's official Linux x64 shared build (change `cross-platform-headless`,
/// design D2; not tested): its C API library and its onnxruntime.
#[cfg(not(windows))]
pub const ENGINE: Asset = Asset {
    label: "Voice engine (sherpa-onnx 2.0.1)",
    page: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/v2.0.1",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-linux-x64-shared-lib.tar.bz2",
    sha256: "532c45bbd41036d83cd70606797529aa8086c2053395a58967736c727625b120",
    directory: "sherpa-onnx-2.0.1",
    marker: "libsherpa-onnx-c-api.so",
    size: 9_710_131,
    kind: AssetKind::Archive,
    root: "sherpa-onnx-v2.0.1-linux-x64-shared-lib/lib",
    keep: &["libonnxruntime.so", "libsherpa-onnx-c-api.so"],
};

/// piper-phonemize v1.5.9's official Linux x64 shared build (espeak-ng inside, GPL-3.0;
/// change `cross-platform-headless`, design D2; not tested).
#[cfg(not(windows))]
pub const PHONEMIZER: Asset = Asset {
    label: "Phonemizer for Piper and Kokoro (piper-phonemize 1.5.9, GPL-3.0)",
    page: "https://github.com/csukuangfj/piper-phonemize/releases/tag/v1.5.9",
    url: "https://github.com/csukuangfj/piper-phonemize/releases/download/v1.5.9/piper-phonemize-v1.5.9-linux-x64-shared-lib.tar.bz2",
    sha256: "609569420d356f561e13ee6d52ed0edf7dcae225ea498f4cc68587c11d6d295c",
    directory: "piper-phonemize-1.5.9",
    marker: "libpiper_phonemize_core.so",
    size: 305_777,
    kind: AssetKind::Archive,
    root: "lib",
    keep: &["libpiper_phonemize_core.so", "libpiper_phonemize_cxx_api.so"],
};

/// sherpa-onnx v2.0.1, the CPU build: only its C API library and its onnxruntime are kept.
#[cfg(windows)]
pub const ENGINE: Asset = Asset {
    label: "Voice engine (sherpa-onnx 2.0.1)",
    page: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/v2.0.1",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-win-x64-shared-MT-Release-lib.tar.bz2",
    sha256: "afa4de3ca4ab0afd53bb4aab40d28ee15608364b549ffc0fe0caf6489bfc39a5",
    directory: "sherpa-onnx-2.0.1",
    marker: "sherpa-onnx-c-api.dll",
    size: 7_899_712,
    kind: AssetKind::Archive,
    root: "sherpa-onnx-v2.0.1-win-x64-shared-MT-Release-lib/lib",
    keep: &["onnxruntime.dll", "sherpa-onnx-c-api.dll"],
};

/// piper-phonemize v1.5.9 (espeak-ng inside, GPL-3.0): the phonemes for Piper and Kokoro.
#[cfg(windows)]
pub const PHONEMIZER: Asset = Asset {
    label: "Phonemizer for Piper and Kokoro (piper-phonemize 1.5.9, GPL-3.0)",
    page: "https://github.com/csukuangfj/piper-phonemize/releases/tag/v1.5.9",
    url: "https://github.com/csukuangfj/piper-phonemize/releases/download/v1.5.9/piper-phonemize-v1.5.9-windows-x64-shared-lib.tar.bz2",
    sha256: "e0a66e68a96c7d65a408255599816408f7cf6a0b84866b4cfd5a80f3f71abfa7",
    directory: "piper-phonemize-1.5.9",
    marker: "piper_phonemize_core.dll",
    size: 427_015,
    kind: AssetKind::Archive,
    root: "bin",
    keep: &["*.dll"],
};

/// Where the phonemizer's source is, as its GPL-3.0 license asks to tell.
pub const PHONEMIZER_SOURCE: &str = "https://github.com/csukuangfj/piper-phonemize";

const VOICES_PAGE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models";

/// The engine family of a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceEngine {
    Supertonic,
    Piper,
    Kokoro,
    Windows,
}

/// One download that holds one or more voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicePack {
    pub asset: Asset,
    /// The license of the voice data, shown before the download and in the notices.
    pub data_license: &'static str,
    /// Piper and Kokoro read phonemes made by [`PHONEMIZER`].
    pub needs_phonemizer: bool,
    pub sample_rate: u32,
}

/// One voice the window lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    /// BCP 47, or `multi` for a voice that speaks every language its model knows.
    pub language: &'static str,
    pub engine: VoiceEngine,
    pub pack: &'static VoicePack,
    /// The speaker id within the pack.
    pub speaker: i32,
}

pub const SUPERTONIC3: VoicePack = VoicePack {
    asset: Asset {
        label: "Supertonic 3 voices (10 voices, 31 languages)",
        page: VOICES_PAGE,
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2",
        sha256: "82fa96f91c4ef8abaae3a14a3f4153facf88bed821d1f7331cec2700f432c427",
        directory: "sherpa-onnx-supertonic-3-tts-int8-2026-05-11",
        marker: "voice.bin",
        size: 128_774_318,
        ..Asset::ARCHIVE
    },
    data_license: "BigScience OpenRAIL-M",
    needs_phonemizer: false,
    sample_rate: 44_100,
};

/// The languages Supertonic 3 reads (`extra.lang`); `na` lets it infer the language.
pub const SUPERTONIC_LANGUAGES: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu",
    "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk", "vi",
];

pub const KOKORO_EN: VoicePack = VoicePack {
    asset: Asset {
        label: "Kokoro English voices (11 voices)",
        page: VOICES_PAGE,
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-en-v0_19.tar.bz2",
        sha256: "912804855a04745fa77a30be545b3f9a5d15c4d66db00b88cbcd4921df605ac7",
        directory: "kokoro-en-v0_19",
        marker: "model.onnx",
        size: 319_625_534,
        ..Asset::ARCHIVE
    },
    data_license: "Apache-2.0",
    needs_phonemizer: true,
    sample_rate: 24_000,
};

macro_rules! piper_pack {
    ($name:ident, $voice:literal, $bytes:literal, $sha:literal, $license:literal) => {
        pub const $name: VoicePack = VoicePack {
            asset: Asset {
                label: concat!("Piper voice ", $voice),
                page: VOICES_PAGE,
                url: concat!(
                    "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-",
                    $voice,
                    ".tar.bz2"
                ),
                sha256: $sha,
                directory: concat!("vits-piper-", $voice),
                marker: concat!($voice, ".onnx"),
                size: $bytes,
                ..Asset::ARCHIVE
            },
            data_license: $license,
            needs_phonemizer: true,
            sample_rate: 22_050,
        };
    };
}

piper_pack!(
    PIPER_EN_US_LIBRITTS_R,
    "en_US-libritts_r-medium",
    82_038_311,
    "10dc268f3e371696d721486123e2705a9fc1faa113491979fde4d88dba1f1b1c",
    "CC BY 4.0"
);
piper_pack!(
    PIPER_EN_GB_CORI,
    "en_GB-cori-high",
    115_574_061,
    "42922f07738fcde2e49eed4e959635692f73b933de35a6b7c1010162ff566292",
    "public domain"
);
piper_pack!(
    PIPER_EN_GB_ALBA,
    "en_GB-alba-medium",
    67_212_349,
    "fcd45962906933eec4431d3688f7d74aaac8713c87c6717f91fd3b23463aa1a1",
    "CC BY 4.0"
);
piper_pack!(
    PIPER_RU_DENIS,
    "ru_RU-denis-medium",
    67_190_991,
    "efa4c18e0b5e32b81d1b6df36b9d312831e5d545200e27848ef926a4cd930300",
    "CC0"
);
piper_pack!(
    PIPER_RU_DMITRI,
    "ru_RU-dmitri-medium",
    67_188_551,
    "c86d0803737de13d441923ff3b3f309482fab8d7af3ec85949942809eb9a3660",
    "CC0"
);
piper_pack!(
    PIPER_RU_IRINA,
    "ru_RU-irina-medium",
    67_153_308,
    "1fc0f54e5e084fe287c07909f2f6e0ba6d857864cf800e3ab80286a4e8233008",
    "not stated (RHVoice data)"
);
piper_pack!(
    PIPER_CS_JIRKA,
    "cs_CZ-jirka-medium",
    67_212_775,
    "321aafd83dd89b3d5b2c36c5898e0710d4f472f3c191cb7f73901a89b7f8539f",
    "CC0"
);
piper_pack!(
    PIPER_NL_PIM,
    "nl_NL-pim-medium",
    67_240_538,
    "e78ed301bf6fc66561a04c110754cd6928d772630a3b80b09a83f89410130171",
    "CC0"
);
piper_pack!(
    PIPER_NL_BE_NATHALIE,
    "nl_BE-nathalie-medium",
    67_226_472,
    "c7484313abea395243e872196bc75181c07aad8120b8a61e0f6f4b3863b6a7c8",
    "CC0"
);
piper_pack!(
    PIPER_FR_SIWIS,
    "fr_FR-siwis-medium",
    67_207_459,
    "375909aa30842b3a4efa10b1beb1d761af792960ae6873b4d53889f96c66195b",
    "CC BY 4.0"
);
piper_pack!(
    PIPER_DE_THORSTEN,
    "de_DE-thorsten-medium",
    67_214_254,
    "50487d9c95fdf2191f31d2588569381063ba1591dcd4c7d4bdd30f12b2191714",
    "CC0"
);
piper_pack!(
    PIPER_IT_PAOLA,
    "it_IT-paola-medium",
    67_221_173,
    "7541f75778afa164e44e34baaef63befad7698595df26a95ca944b63ef1a16b4",
    "CC0"
);
piper_pack!(
    PIPER_PL_GOSIA,
    "pl_PL-gosia-medium",
    67_211_182,
    "75bd34dcbdc4dd98d763954756b4b34b4208100497c836381542e4d73dcefa9c",
    "CC0"
);
piper_pack!(
    PIPER_PL_DARKMAN,
    "pl_PL-darkman-medium",
    67_215_635,
    "444727aa46eb6db645a2bc88fe73868e4bd7596b7f56ca39fad5ef53c41210d4",
    "CC0"
);
piper_pack!(
    PIPER_PT_BR_FABER,
    "pt_BR-faber-medium",
    67_183_065,
    "7add3f923ad6bc25ca8a192805fd1a64d1b3893e4611c4a9719545a825039a83",
    "CC0"
);
piper_pack!(
    PIPER_ES_DAVEFX,
    "es_ES-davefx-medium",
    67_184_952,
    "a3f6beb54a9cb893279f72978a22f807a4d9fc9c7848157b524d5cc7b7f58b22",
    "CC0"
);
piper_pack!(
    PIPER_ES_MX_ALD,
    "es_MX-ald-medium",
    67_196_497,
    "c60cf7bc853eb01a875fba0756523fecf9d9f46c763b19785d223fc981fb4fb6",
    "Unlicense"
);

macro_rules! voice {
    ($id:literal, $name:literal, $language:literal, $engine:ident, $pack:ident, $speaker:literal) => {
        VoiceEntry {
            id: $id,
            display_name: $name,
            language: $language,
            engine: VoiceEngine::$engine,
            pack: &$pack,
            speaker: $speaker,
        }
    };
}

/// The catalog in the order the window lists it, by engine; the default is
/// `supertonic3:F1`. Supertonic's speaker ids follow `voice.bin`, which sherpa-onnx builds
/// from the style files sorted by name: F1–F5, then M1–M5.
const VOICES: &[VoiceEntry] = &[
    voice!(
        "supertonic3:F1",
        "Supertonic 3 F1 (female)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        0
    ),
    voice!(
        "supertonic3:F2",
        "Supertonic 3 F2 (female)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        1
    ),
    voice!(
        "supertonic3:F3",
        "Supertonic 3 F3 (female)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        2
    ),
    voice!(
        "supertonic3:F4",
        "Supertonic 3 F4 (female)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        3
    ),
    voice!(
        "supertonic3:F5",
        "Supertonic 3 F5 (female)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        4
    ),
    voice!(
        "supertonic3:M1",
        "Supertonic 3 M1 (male)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        5
    ),
    voice!(
        "supertonic3:M2",
        "Supertonic 3 M2 (male)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        6
    ),
    voice!(
        "supertonic3:M3",
        "Supertonic 3 M3 (male)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        7
    ),
    voice!(
        "supertonic3:M4",
        "Supertonic 3 M4 (male)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        8
    ),
    voice!(
        "supertonic3:M5",
        "Supertonic 3 M5 (male)",
        "multi",
        Supertonic,
        SUPERTONIC3,
        9
    ),
    voice!(
        "piper:en_US-libritts_r-medium",
        "Piper English (US) LibriTTS-R",
        "en-US",
        Piper,
        PIPER_EN_US_LIBRITTS_R,
        0
    ),
    voice!(
        "piper:en_GB-cori-high",
        "Piper English (UK) Cori, female",
        "en-GB",
        Piper,
        PIPER_EN_GB_CORI,
        0
    ),
    voice!(
        "piper:en_GB-alba-medium",
        "Piper English (UK) Alba, female",
        "en-GB",
        Piper,
        PIPER_EN_GB_ALBA,
        0
    ),
    voice!(
        "piper:ru_RU-denis-medium",
        "Piper Russian Denis, male",
        "ru",
        Piper,
        PIPER_RU_DENIS,
        0
    ),
    voice!(
        "piper:ru_RU-dmitri-medium",
        "Piper Russian Dmitri, male",
        "ru",
        Piper,
        PIPER_RU_DMITRI,
        0
    ),
    voice!(
        "piper:ru_RU-irina-medium",
        "Piper Russian Irina, female",
        "ru",
        Piper,
        PIPER_RU_IRINA,
        0
    ),
    voice!(
        "piper:cs_CZ-jirka-medium",
        "Piper Czech Jirka, male",
        "cs",
        Piper,
        PIPER_CS_JIRKA,
        0
    ),
    voice!(
        "piper:nl_NL-pim-medium",
        "Piper Dutch Pim, male",
        "nl",
        Piper,
        PIPER_NL_PIM,
        0
    ),
    voice!(
        "piper:nl_BE-nathalie-medium",
        "Piper Dutch (Belgium) Nathalie, female",
        "nl-BE",
        Piper,
        PIPER_NL_BE_NATHALIE,
        0
    ),
    voice!(
        "piper:fr_FR-siwis-medium",
        "Piper French Siwis, female",
        "fr",
        Piper,
        PIPER_FR_SIWIS,
        0
    ),
    voice!(
        "piper:de_DE-thorsten-medium",
        "Piper German Thorsten, male",
        "de",
        Piper,
        PIPER_DE_THORSTEN,
        0
    ),
    voice!(
        "piper:it_IT-paola-medium",
        "Piper Italian Paola, female",
        "it",
        Piper,
        PIPER_IT_PAOLA,
        0
    ),
    voice!(
        "piper:pl_PL-gosia-medium",
        "Piper Polish Gosia, female",
        "pl",
        Piper,
        PIPER_PL_GOSIA,
        0
    ),
    voice!(
        "piper:pl_PL-darkman-medium",
        "Piper Polish Darkman, male",
        "pl",
        Piper,
        PIPER_PL_DARKMAN,
        0
    ),
    voice!(
        "piper:pt_BR-faber-medium",
        "Piper Portuguese (Brazil) Faber, male",
        "pt-BR",
        Piper,
        PIPER_PT_BR_FABER,
        0
    ),
    voice!(
        "piper:es_ES-davefx-medium",
        "Piper Spanish Davefx, male",
        "es",
        Piper,
        PIPER_ES_DAVEFX,
        0
    ),
    voice!(
        "piper:es_MX-ald-medium",
        "Piper Spanish (Mexico) Ald, male",
        "es-MX",
        Piper,
        PIPER_ES_MX_ALD,
        0
    ),
    voice!(
        "kokoro-en:af",
        "Kokoro English af (American, female)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        0
    ),
    voice!(
        "kokoro-en:af_bella",
        "Kokoro English Bella (American, female)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        1
    ),
    voice!(
        "kokoro-en:af_nicole",
        "Kokoro English Nicole (American, female)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        2
    ),
    voice!(
        "kokoro-en:af_sarah",
        "Kokoro English Sarah (American, female)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        3
    ),
    voice!(
        "kokoro-en:af_sky",
        "Kokoro English Sky (American, female)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        4
    ),
    voice!(
        "kokoro-en:am_adam",
        "Kokoro English Adam (American, male)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        5
    ),
    voice!(
        "kokoro-en:am_michael",
        "Kokoro English Michael (American, male)",
        "en-US",
        Kokoro,
        KOKORO_EN,
        6
    ),
    voice!(
        "kokoro-en:bf_emma",
        "Kokoro English Emma (British, female)",
        "en-GB",
        Kokoro,
        KOKORO_EN,
        7
    ),
    voice!(
        "kokoro-en:bf_isabella",
        "Kokoro English Isabella (British, female)",
        "en-GB",
        Kokoro,
        KOKORO_EN,
        8
    ),
    voice!(
        "kokoro-en:bm_george",
        "Kokoro English George (British, male)",
        "en-GB",
        Kokoro,
        KOKORO_EN,
        9
    ),
    voice!(
        "kokoro-en:bm_lewis",
        "Kokoro English Lewis (British, male)",
        "en-GB",
        Kokoro,
        KOKORO_EN,
        10
    ),
];

pub fn voices() -> &'static [VoiceEntry] {
    VOICES
}

pub fn find(id: &str) -> Option<&'static VoiceEntry> {
    VOICES.iter().find(|voice| voice.id == id)
}

pub const DEFAULT_VOICE: &str = "supertonic3:F1";

/// The prefix of a voice Windows has.
pub const WINDOWS_PREFIX: &str = "windows:";

/// A Windows voice id, `windows:<voice id>`, with a non-empty id.
pub fn windows_voice(id: &str) -> Option<&str> {
    id.strip_prefix(WINDOWS_PREFIX)
        .filter(|rest| !rest.trim().is_empty())
}

/// What a catalog voice needs on disk: the engine, the phonemizer when its engine needs
/// one, and its own pack. Empty for a Windows voice.
pub fn assets_for(voice: &VoiceEntry) -> Vec<Asset> {
    if voice.engine == VoiceEngine::Windows {
        return Vec::new();
    }
    let mut assets = vec![ENGINE];
    if voice.pack.needs_phonemizer {
        assets.push(PHONEMIZER);
    }
    assets.push(voice.pack.asset);
    assets
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 6.4 of change `whisper-mode`.
    #[test]
    fn the_voice_catalog_is_pinned_unique_and_commercially_usable() {
        let mut ids: Vec<&str> = voices().iter().map(|voice| voice.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), voices().len(), "ids are unique");
        for voice in voices() {
            let asset = voice.pack.asset;
            assert_eq!(asset.sha256.len(), 64, "{}", voice.id);
            assert!(asset.size > 0, "{}", voice.id);
            let license = voice.pack.data_license.to_lowercase();
            assert!(
                !license.contains("by-nc")
                    && !license.contains("non-commercial")
                    && !license.contains("noncommercial"),
                "{}: {license}",
                voice.id
            );
            assert!(!license.contains("agpl"), "{}", voice.id);
            assert_eq!(
                voice.pack.needs_phonemizer,
                matches!(voice.engine, VoiceEngine::Piper | VoiceEngine::Kokoro),
                "only Piper and Kokoro need the phonemizer: {}",
                voice.id
            );
        }
        for asset in [ENGINE, PHONEMIZER] {
            assert_eq!(asset.sha256.len(), 64);
            assert!(asset.size > 0);
        }
        assert_eq!(find(DEFAULT_VOICE).unwrap().engine, VoiceEngine::Supertonic);
        assert_eq!(
            voices()
                .iter()
                .filter(|v| v.engine == VoiceEngine::Piper)
                .count(),
            17
        );
        assert_eq!(
            voices()
                .iter()
                .filter(|v| v.engine == VoiceEngine::Kokoro)
                .count(),
            11
        );
        assert_eq!(
            windows_voice("windows:MSTTS_V110_ruRU_IrinaM"),
            Some("MSTTS_V110_ruRU_IrinaM")
        );
        assert_eq!(windows_voice("windows: "), None);
        assert_eq!(windows_voice("piper:x"), None);
        let supertonic = assets_for(find(DEFAULT_VOICE).unwrap());
        assert_eq!(supertonic.len(), 2, "engine and pack, no phonemizer");
        let piper = assets_for(find("piper:ru_RU-denis-medium").unwrap());
        assert_eq!(piper[1], PHONEMIZER);
    }
}
