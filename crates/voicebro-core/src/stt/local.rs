//! The user's own Whisper models in `whisper-models\` next to the app (change
//! `whisper-mode`, design D9): every `.bin` file there is listed once at start as
//! `local:<file name>`, the way Vosk's models folder works. The app never changes the folder.

use super::catalog::{local_file, LOCAL_PREFIX};
use std::path::{Path, PathBuf};

/// The folder's name next to the app.
pub const FOLDER: &str = "whisper-models";

/// The ids of the `.bin` files in `folder`, sorted; other entries are skipped with a log
/// line. A missing folder lists nothing.
pub fn scan(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_bin = entry.path().is_file()
                && Path::new(&name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"));
            let id = format!("{LOCAL_PREFIX}{name}");
            if is_bin && local_file(&id).is_some() {
                Some(id)
            } else {
                log::info!(
                    "Whisper models folder: skipped {name} (only ggml .bin files are models)"
                );
                None
            }
        })
        .collect();
    ids.sort();
    ids
}

/// The path of a `local:` model id inside `folder`; `None` for any other id or a name that
/// is not a plain file name.
pub fn path(folder: &Path, id: &str) -> Option<PathBuf> {
    local_file(id).map(|file| folder.join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_files_are_listed_and_other_files_skipped() {
        let folder =
            std::env::temp_dir().join(format!("voicebro-whisper-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("nested.bin")).unwrap();
        std::fs::write(folder.join("ggml-my-finetune.bin"), b"x").unwrap();
        std::fs::write(folder.join("readme.txt"), b"x").unwrap();
        assert_eq!(scan(&folder), vec!["local:ggml-my-finetune.bin"]);
        assert_eq!(
            path(&folder, "local:ggml-my-finetune.bin"),
            Some(folder.join("ggml-my-finetune.bin"))
        );
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(scan(&folder).is_empty());
    }

    #[test]
    fn a_name_with_dots_or_separators_is_no_id() {
        let folder = Path::new("C:\\models");
        for id in [
            "local:..\\x.bin",
            "local:../x.bin",
            "local:a/b.bin",
            "local:C:x.bin",
            "local:",
            "whisper-base-q8_0",
        ] {
            assert_eq!(path(folder, id), None, "{id}");
        }
    }
}
