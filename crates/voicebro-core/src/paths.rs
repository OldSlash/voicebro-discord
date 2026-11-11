//! Where the app keeps its files: next to its own executable, so the installation folder
//! holds everything (settings, logs, downloaded assets). Overridable for tests.

use std::path::{Path, PathBuf};

/// The environment variable that redirects the whole data directory.
pub const DATA_DIR_ENV: &str = "VOICEBRO_DATA_DIR";

/// The folder the executable runs from, or `VOICEBRO_DATA_DIR` when set.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|value| !value.is_empty()) {
        return PathBuf::from(dir);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

pub fn assets_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("assets")
}

/// Vosk models the user adds for other languages; the app only reads it.
pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models")
}

pub fn codex_workspace_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("codex-workspace")
}

/// The window's WebView2 data (cache, local storage), kept with the app instead of under
/// `%LOCALAPPDATA%`.
pub fn webview_data_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("webview-data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_paths_hang_off_the_data_dir() {
        let dir = Path::new("X:/data");
        assert_eq!(settings_path(dir), Path::new("X:/data/settings.json"));
        assert_eq!(logs_dir(dir), Path::new("X:/data/logs"));
        assert_eq!(assets_dir(dir), Path::new("X:/data/assets"));
        assert_eq!(
            codex_workspace_dir(dir),
            Path::new("X:/data/codex-workspace")
        );
        assert_eq!(webview_data_dir(dir), Path::new("X:/data/webview-data"));
    }

    #[test]
    fn default_data_dir_is_the_executable_folder_unless_overridden() {
        if std::env::var_os(DATA_DIR_ENV).is_some() {
            return;
        }
        let exe_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert_eq!(data_dir(), exe_dir);
    }
}
