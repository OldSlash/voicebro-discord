//! Where the app keeps its files: one directory, overridable for tests and self-tests.

use std::path::{Path, PathBuf};

/// The environment variable that redirects the whole data directory.
pub const DATA_DIR_ENV: &str = "VOICEBRO_DATA_DIR";

/// `%LOCALAPPDATA%\VoiceBro`, or `VOICEBRO_DATA_DIR` when set.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|value| !value.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("VoiceBro")
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

pub fn codex_workspace_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("codex-workspace")
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
        assert_eq!(codex_workspace_dir(dir), Path::new("X:/data/codex-workspace"));
    }

    #[test]
    fn default_data_dir_ends_with_the_product_name_unless_overridden() {
        let dir = data_dir();
        if std::env::var_os(DATA_DIR_ENV).is_none() {
            assert!(dir.ends_with("VoiceBro"), "{}", dir.display());
        }
    }
}
