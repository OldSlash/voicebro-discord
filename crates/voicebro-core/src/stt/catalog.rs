//! The Whisper runtimes, models and VAD model the app can download (change `whisper-mode`,
//! designs D8 and D9), each pinned by SHA-256, and the languages Whisper knows.

use crate::assets::{Asset, AssetKind};

/// whisper.cpp's version that the runtimes below are, and the one `whisper_version()` must
/// report.
pub const WHISPER_VERSION: &str = "1.9.5";

const RELEASE_PAGE: &str = "https://github.com/ggml-org/whisper.cpp/releases/tag/b5454";
/// The Hugging Face commit the ggerganov/whisper.cpp models are pinned to.
/// (Written out in each URL below, as `concat!` takes only literals.)
#[allow(dead_code)]
const MODELS_COMMIT: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
const MODELS_PAGE: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1";

/// The CPU runtime: whisper.cpp v1.9.5 from the nightly tag `b5454` (same commit).
pub const CPU_RUNTIME: Asset = Asset {
    label: "Whisper runtime (CPU)",
    page: RELEASE_PAGE,
    url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip",
    sha256: "6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb",
    directory: "whisper-cpu-1.9.5",
    marker: "whisper.dll",
    size: 8_928_640,
    kind: AssetKind::Archive,
    root: "Release",
    keep: &["whisper.dll", "ggml.dll", "ggml-base.dll", "ggml-cpu-*.dll"],
};

/// The NVIDIA runtime (CUDA 12.4): the CPU set plus the CUDA backend and its libraries.
pub const GPU_RUNTIME: Asset = Asset {
    label: "Whisper runtime (NVIDIA GPU, CUDA 12.4)",
    page: RELEASE_PAGE,
    url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-win-cuda-12.4.0-x64.zip",
    sha256: "afef0b881c500958921c3f5523b50e59ee2ec9b6f5cbd25b324c51ed308a957a",
    directory: "whisper-cuda-1.9.5",
    marker: "ggml-cuda.dll",
    size: 684_913_404,
    kind: AssetKind::Archive,
    root: "Release",
    keep: &[
        "whisper.dll",
        "ggml.dll",
        "ggml-base.dll",
        "ggml-cpu-*.dll",
        "ggml-cuda.dll",
        "cudart64_12.dll",
        "cublas64_12.dll",
        "cublasLt64_12.dll",
    ],
};

/// Silero VAD for whisper.cpp, downloaded with every Whisper setup (design D3).
pub const VAD_MODEL: Asset = Asset {
    label: "Silero voice activity model",
    page: "https://huggingface.co/ggml-org/whisper-vad/tree/9ffd54a1e1ee413ddf265af9913beaf518d1639b",
    url: "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin",
    sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
    directory: "whisper-vad-silero-6.2.0",
    marker: "ggml-silero-v6.2.0.bin",
    size: 885_098,
    kind: AssetKind::File,
    root: "",
    keep: &[],
};

/// How the window groups the models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelGroup {
    Multilingual,
    EnglishOnly,
    RussianTuned,
    /// The user's own files in `whisper-models\`.
    Local,
}

/// One Whisper model the app can download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperModelEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub group: ModelGroup,
    /// The one-line note the window shows.
    pub note: &'static str,
    pub license: &'static str,
    pub asset: Asset,
}

macro_rules! ggml_model {
    ($id:literal, $name:literal, $group:expr, $file:literal, $bytes:literal, $sha:literal, $note:literal) => {
        WhisperModelEntry {
            id: $id,
            display_name: $name,
            group: $group,
            note: $note,
            license: "MIT",
            asset: Asset {
                label: concat!("Whisper ", $name, " model"),
                page: MODELS_PAGE,
                url: concat!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/",
                    "5359861c739e955e79d9a303bcbc70fb988958b1",
                    "/",
                    $file
                ),
                sha256: $sha,
                directory: $id,
                marker: $file,
                size: $bytes,
                kind: AssetKind::File,
                root: "",
                keep: &[],
            },
        }
    };
}

/// The catalog in the order the window lists it; the default is `whisper-base-q8_0`.
const ENTRIES: &[WhisperModelEntry] = &[
    ggml_model!(
        "whisper-tiny-q8_0",
        "tiny (q8_0)",
        ModelGroup::Multilingual,
        "ggml-tiny-q8_0.bin",
        43_537_433,
        "c2085835d3f50733e2ff6e4b41ae8a2b8d8110461e18821b09a15c40c42d1cca",
        "fastest, rough: about 0.25 s on a CPU"
    ),
    ggml_model!(
        "whisper-base-q8_0",
        "base (q8_0)",
        ModelGroup::Multilingual,
        "ggml-base-q8_0.bin",
        81_768_585,
        "c577b9a86e7e048a0b7eada054f4dd79a56bbfa911fbdacf900ac5b567cbb7d9",
        "fast: about 0.6 s on a CPU; weak for Russian"
    ),
    ggml_model!(
        "whisper-small-q8_0",
        "small (q8_0)",
        ModelGroup::Multilingual,
        "ggml-small-q8_0.bin",
        264_464_607,
        "49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f",
        "balanced: about 1.8 s on a CPU, 0.1–0.2 s on a GPU"
    ),
    ggml_model!(
        "whisper-medium-q5_0",
        "medium (q5_0)",
        ModelGroup::Multilingual,
        "ggml-medium-q5_0.bin",
        539_212_467,
        "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        "accurate: 0.2–0.5 s on a GPU, 6–10 s on a CPU"
    ),
    ggml_model!(
        "whisper-large-v3-turbo-q5_0",
        "large-v3-turbo (q5_0)",
        ModelGroup::Multilingual,
        "ggml-large-v3-turbo-q5_0.bin",
        574_041_195,
        "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        "accurate and fast on a GPU: about 0.14 s (11 s on a CPU)"
    ),
    ggml_model!(
        "whisper-large-v3-turbo-q8_0",
        "large-v3-turbo (q8_0)",
        ModelGroup::Multilingual,
        "ggml-large-v3-turbo-q8_0.bin",
        874_188_075,
        "317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1",
        "the same speed on a GPU, closer to full quality"
    ),
    ggml_model!(
        "whisper-large-v3-turbo",
        "large-v3-turbo",
        ModelGroup::Multilingual,
        "ggml-large-v3-turbo.bin",
        1_624_555_275,
        "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        "turbo at full precision: about 0.3 s on a GPU"
    ),
    ggml_model!(
        "whisper-large-v3-q5_0",
        "large-v3 (q5_0)",
        ModelGroup::Multilingual,
        "ggml-large-v3-q5_0.bin",
        1_081_140_203,
        "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        "most accurate, slower decoder: 0.2–0.6 s on a GPU"
    ),
    ggml_model!(
        "whisper-base.en-q8_0",
        "base.en (q8_0)",
        ModelGroup::EnglishOnly,
        "ggml-base.en-q8_0.bin",
        81_781_811,
        "a4d4a0768075e13cfd7e19df3ae2dbc4a68d37d36a7dad45e8410c9a34f8c87e",
        "fast, English only: about 0.7 s on a CPU"
    ),
    ggml_model!(
        "whisper-small.en-q8_0",
        "small.en (q8_0)",
        ModelGroup::EnglishOnly,
        "ggml-small.en-q8_0.bin",
        264_477_561,
        "67a179f608ea6114bd3fdb9060e762b588a3fb3bd00c4387971be4d177958067",
        "balanced, English only: about 2 s on a CPU"
    ),
    WhisperModelEntry {
        id: "whisper-distil-large-v3.5",
        display_name: "distil-large-v3.5",
        group: ModelGroup::EnglishOnly,
        note: "distilled large-v3, English only: about 0.3 s on a GPU",
        license: "MIT",
        asset: Asset {
            label: "Whisper distil-large-v3.5 model",
            page: "https://huggingface.co/distil-whisper/distil-large-v3.5-ggml/tree/960ecb5c2ecfba3ebb9ebe485c1032ec266cf436",
            url: "https://huggingface.co/distil-whisper/distil-large-v3.5-ggml/resolve/960ecb5c2ecfba3ebb9ebe485c1032ec266cf436/ggml-model.bin",
            sha256: "ec2498919b498c5f6b00041adb45650124b3cd9f26f545fffa8f5d11c28dcf26",
            directory: "whisper-distil-large-v3.5",
            marker: "ggml-distil-large-v3.5.bin",
            size: 1_519_521_155,
            kind: AssetKind::File,
            root: "",
            keep: &[],
        },
    },
    WhisperModelEntry {
        id: "whisper-large-v3-turbo-ru",
        display_name: "large-v3-turbo Russian (community)",
        group: ModelGroup::RussianTuned,
        note: "turbo fine-tuned on Common Voice 17 Russian: about 0.3 s on a GPU",
        license: "MIT",
        asset: Asset {
            label: "Whisper large-v3-turbo Russian model",
            page: "https://huggingface.co/dvislobokov/whisper-large-v3-turbo-russian/tree/fbecad6b44518d27a9b2411d0b3a031c2d5da20f",
            url: "https://huggingface.co/dvislobokov/whisper-large-v3-turbo-russian/resolve/fbecad6b44518d27a9b2411d0b3a031c2d5da20f/ggml-model.bin",
            sha256: "39f24d81480828d2b25e75c7c02b5c72b3db3b98e1c33d39990bc186cb3fe804",
            directory: "whisper-large-v3-turbo-ru",
            marker: "ggml-large-v3-turbo-russian.bin",
            size: 1_624_555_275,
            kind: AssetKind::File,
            root: "",
            keep: &[],
        },
    },
    WhisperModelEntry {
        id: "whisper-large-v3-ru-podlodka-q5_0",
        display_name: "large-v3 Russian Podlodka (q5_0, community)",
        group: ModelGroup::RussianTuned,
        note: "large-v3 fine-tuned for Russian: 0.2–0.7 s on a GPU",
        license: "Apache-2.0",
        asset: Asset {
            label: "Whisper large-v3 Russian Podlodka model",
            page: "https://huggingface.co/kdes70/whisper-large-v3-ru-podlodka-ggml/tree/e0bff69633e0e9dbf75412dac84b6266749a3215",
            url: "https://huggingface.co/kdes70/whisper-large-v3-ru-podlodka-ggml/resolve/e0bff69633e0e9dbf75412dac84b6266749a3215/ggml-podlodka-large-v3-q5_0.bin",
            sha256: "3f870a58279fd98cf60ca1ac6cd97333ef4ac1a34c5991ba75d0fa7d6d428ce5",
            directory: "whisper-large-v3-ru-podlodka-q5_0",
            marker: "ggml-podlodka-large-v3-q5_0.bin",
            size: 1_081_140_203,
            kind: AssetKind::File,
            root: "",
            keep: &[],
        },
    },
];

pub fn entries() -> &'static [WhisperModelEntry] {
    ENTRIES
}

pub fn find(id: &str) -> Option<&'static WhisperModelEntry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

pub const DEFAULT_MODEL: &str = "whisper-base-q8_0";

pub fn default_entry() -> &'static WhisperModelEntry {
    find(DEFAULT_MODEL).expect("the default Whisper model is in the catalog")
}

/// The prefix of a model file the user put into `whisper-models\`.
pub const LOCAL_PREFIX: &str = "local:";

/// The file name of a `local:` Whisper model id, when it is a plain file name (no
/// separators, no `..`, no drive colon).
pub fn local_file(id: &str) -> Option<&str> {
    let file = id.strip_prefix(LOCAL_PREFIX)?;
    let plain = !file.trim().is_empty()
        && file.trim() == file
        && !file.contains(['/', '\\', ':'])
        && file != "."
        && !file.contains("..");
    plain.then_some(file)
}

/// The languages whisper.cpp knows, as (ISO 639-1 code, English name), in whisper.cpp's
/// order.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "Chinese"),
    ("de", "German"),
    ("es", "Spanish"),
    ("ru", "Russian"),
    ("ko", "Korean"),
    ("fr", "French"),
    ("ja", "Japanese"),
    ("pt", "Portuguese"),
    ("tr", "Turkish"),
    ("pl", "Polish"),
    ("ca", "Catalan"),
    ("nl", "Dutch"),
    ("ar", "Arabic"),
    ("sv", "Swedish"),
    ("it", "Italian"),
    ("id", "Indonesian"),
    ("hi", "Hindi"),
    ("fi", "Finnish"),
    ("vi", "Vietnamese"),
    ("he", "Hebrew"),
    ("uk", "Ukrainian"),
    ("el", "Greek"),
    ("ms", "Malay"),
    ("cs", "Czech"),
    ("ro", "Romanian"),
    ("da", "Danish"),
    ("hu", "Hungarian"),
    ("ta", "Tamil"),
    ("no", "Norwegian"),
    ("th", "Thai"),
    ("ur", "Urdu"),
    ("hr", "Croatian"),
    ("bg", "Bulgarian"),
    ("lt", "Lithuanian"),
    ("la", "Latin"),
    ("mi", "Maori"),
    ("ml", "Malayalam"),
    ("cy", "Welsh"),
    ("sk", "Slovak"),
    ("te", "Telugu"),
    ("fa", "Persian"),
    ("lv", "Latvian"),
    ("bn", "Bengali"),
    ("sr", "Serbian"),
    ("az", "Azerbaijani"),
    ("sl", "Slovenian"),
    ("kn", "Kannada"),
    ("et", "Estonian"),
    ("mk", "Macedonian"),
    ("br", "Breton"),
    ("eu", "Basque"),
    ("is", "Icelandic"),
    ("hy", "Armenian"),
    ("ne", "Nepali"),
    ("mn", "Mongolian"),
    ("bs", "Bosnian"),
    ("kk", "Kazakh"),
    ("sq", "Albanian"),
    ("sw", "Swahili"),
    ("gl", "Galician"),
    ("mr", "Marathi"),
    ("pa", "Punjabi"),
    ("si", "Sinhala"),
    ("km", "Khmer"),
    ("sn", "Shona"),
    ("yo", "Yoruba"),
    ("so", "Somali"),
    ("af", "Afrikaans"),
    ("oc", "Occitan"),
    ("ka", "Georgian"),
    ("be", "Belarusian"),
    ("tg", "Tajik"),
    ("sd", "Sindhi"),
    ("gu", "Gujarati"),
    ("am", "Amharic"),
    ("yi", "Yiddish"),
    ("lo", "Lao"),
    ("uz", "Uzbek"),
    ("fo", "Faroese"),
    ("ht", "Haitian Creole"),
    ("ps", "Pashto"),
    ("tk", "Turkmen"),
    ("nn", "Nynorsk"),
    ("mt", "Maltese"),
    ("sa", "Sanskrit"),
    ("lb", "Luxembourgish"),
    ("my", "Myanmar"),
    ("bo", "Tibetan"),
    ("tl", "Tagalog"),
    ("mg", "Malagasy"),
    ("as", "Assamese"),
    ("tt", "Tatar"),
    ("haw", "Hawaiian"),
    ("ln", "Lingala"),
    ("ha", "Hausa"),
    ("ba", "Bashkir"),
    ("jw", "Javanese"),
    ("su", "Sundanese"),
    ("yue", "Cantonese"),
];

/// `auto` or a code Whisper knows.
pub fn is_known_language(code: &str) -> bool {
    code == "auto" || LANGUAGES.iter().any(|(known, _)| *known == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 4.1 of change `whisper-mode`.
    #[test]
    fn the_catalog_is_pinned_unique_grouped_and_licensed() {
        let mut ids: Vec<&str> = entries().iter().map(|entry| entry.id).collect();
        assert_eq!(ids.len(), 13);
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), entries().len(), "ids are unique");
        let mut files: Vec<&str> = entries().iter().map(|e| e.asset.file_name()).collect();
        files.sort_unstable();
        files.dedup();
        assert_eq!(files.len(), entries().len(), "file names are unique");
        for asset in
            entries()
                .iter()
                .map(|entry| entry.asset)
                .chain([CPU_RUNTIME, GPU_RUNTIME, VAD_MODEL])
        {
            assert_eq!(asset.sha256.len(), 64, "{}", asset.label);
            assert!(asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(asset.size > 0, "{}", asset.label);
            assert!(asset.url.starts_with("https://"), "{}", asset.label);
        }
        for entry in entries() {
            assert_eq!(entry.asset.kind, AssetKind::File, "{}", entry.id);
            assert_eq!(entry.asset.directory, entry.id);
            assert!(
                ["MIT", "Apache-2.0"].contains(&entry.license),
                "{}",
                entry.id
            );
            assert!(!entry.note.is_empty(), "{}", entry.id);
            assert_ne!(entry.group, ModelGroup::Local);
        }
        assert!(entries()
            .iter()
            .filter(|entry| entry.asset.page.contains("ggerganov"))
            .all(|entry| entry.asset.url.contains(MODELS_COMMIT)));
        assert_eq!(default_entry().id, "whisper-base-q8_0");
        assert_eq!(
            entries()
                .iter()
                .filter(|entry| entry.group == ModelGroup::RussianTuned)
                .count(),
            2
        );
    }

    #[test]
    fn local_models_are_plain_file_names() {
        assert_eq!(local_file("local:my-model.bin"), Some("my-model.bin"));
        for bad in [
            "local:",
            "local:..\\x.bin",
            "local:a/b.bin",
            "local:C:x.bin",
            "local:.",
        ] {
            assert_eq!(local_file(bad), None, "{bad}");
        }
        assert!(is_known_language("auto") && is_known_language("ru") && is_known_language("yue"));
        assert!(!is_known_language("xx") && !is_known_language("RU"));
    }
}
