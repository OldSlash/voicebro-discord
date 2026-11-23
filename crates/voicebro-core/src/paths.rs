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

/// An engine's program on Linux (change `cross-platform-headless`, design D5; not tested):
/// the configured file, only that one when set, or else `name` on `PATH`, with no Windows
/// fallback. A launcher script is fine there: its process group is stopped as a whole.
pub fn find_executable(configured: &str, name: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = std::path::absolute(configured).ok()?;
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter_map(|dir| std::path::absolute(dir.join(name)).ok())
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
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

/// Whisper models the user adds (change `whisper-mode`); the app only reads it.
pub fn whisper_models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(crate::stt::local::FOLDER)
}

pub fn codex_workspace_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("codex-workspace")
}

/// The empty working folder of Claude Code's processes (change `claude-and-opencode-engines`).
pub fn claude_workspace_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("claude-workspace")
}

/// What the app hands Claude Code as files: the user's instruction per guild and the empty
/// MCP configuration.
pub fn claude_config_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("claude")
}

/// The empty working folder of OpenCode's server (change `claude-and-opencode-engines`).
pub fn opencode_workspace_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("opencode-workspace")
}

/// The configuration the app writes for OpenCode (`opencode.json`).
pub fn opencode_config_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("opencode")
}

/// The Codex models, reasoning efforts and voices from the last read, so the window offers
/// them before any Codex process runs (change `config-tab-and-codex-lists`).
pub fn codex_lists_path(data_dir: &Path) -> PathBuf {
    data_dir.join("codex-lists.json")
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
        assert_eq!(codex_lists_path(dir), Path::new("X:/data/codex-lists.json"));
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
