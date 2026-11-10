//! First-use download of recognizer assets: pinned hashes, temp-and-rename, extraction
//! with the system `tar.exe`, a marker file, cleanup of partial files. Downloads use the
//! system `curl.exe`; no HTTP crate.

use super::catalog::Asset;
use super::WakeError;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = &command;
}

pub fn is_installed(assets_dir: &Path, asset: &Asset) -> bool {
    assets_dir
        .join(asset.directory)
        .join(asset.marker)
        .is_file()
}

/// Removes partial downloads left by an interrupted run. Called at start.
pub fn cleanup_partial(assets_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(assets_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("part")
            && std::fs::remove_file(&path).is_ok()
        {
            log::info!("removed a partial download: {}", path.display());
        }
    }
}

pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Downloads `url` to `target` through `curl.exe`, calling `progress` with a percentage
/// (0..=100, in steps of at least 20) while the file grows.
fn download(
    url: &str,
    target: &Path,
    expected_size: u64,
    progress: &dyn Fn(u8),
) -> Result<(), WakeError> {
    let _ = std::fs::remove_file(target);
    let mut command = Command::new(system_tool("curl.exe"));
    command
        .args(["-L", "--fail", "--silent", "--show-error", "-o"])
        .arg(target)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    hide_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| WakeError::Assets(format!("running curl.exe: {error}")))?;
    progress(0);
    let mut last_reported = 0u8;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    let _ = std::fs::remove_file(target);
                    return Err(WakeError::Assets(format!(
                        "download of {url} failed ({status}): {}",
                        stderr.trim()
                    )));
                }
                break;
            }
            Ok(None) => {
                let size = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
                if let Some(ratio) = (size * 100).checked_div(expected_size) {
                    let percent = ratio.min(99) as u8;
                    if percent >= last_reported + 20 {
                        last_reported = percent - percent % 20;
                        progress(last_reported);
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(error) => {
                let _ = std::fs::remove_file(target);
                return Err(WakeError::Assets(format!("waiting for curl.exe: {error}")));
            }
        }
    }
    progress(100);
    Ok(())
}

fn extract(archive: &Path, destination: &Path) -> Result<(), WakeError> {
    let mut command = Command::new(system_tool("tar.exe"));
    command.arg("-xf").arg(archive).arg("-C").arg(destination);
    hide_window(&mut command);
    let output = command
        .output()
        .map_err(|error| WakeError::Assets(format!("running tar.exe: {error}")))?;
    if !output.status.success() {
        return Err(WakeError::Assets(format!(
            "extracting {}: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Makes sure `asset` is downloaded, verified and extracted under `assets_dir`. A hash
/// mismatch deletes the archive and extracts nothing.
pub fn ensure(asset: &Asset, assets_dir: &Path, progress: &dyn Fn(u8)) -> Result<(), WakeError> {
    if is_installed(assets_dir, asset) {
        return Ok(());
    }
    std::fs::create_dir_all(assets_dir).map_err(|error| {
        WakeError::Assets(format!("creating {}: {error}", assets_dir.display()))
    })?;
    let file_name = asset.url.rsplit('/').next().unwrap_or("asset.zip");
    let archive = assets_dir.join(file_name);
    let part = assets_dir.join(format!("{file_name}.part"));
    if !archive.is_file() {
        download(asset.url, &part, asset.size, progress)?;
        std::fs::rename(&part, &archive)
            .map_err(|error| WakeError::Assets(format!("renaming the download: {error}")))?;
    }
    let digest = sha256_file(&archive)
        .map_err(|error| WakeError::Assets(format!("hashing {}: {error}", archive.display())))?;
    if digest != asset.sha256 {
        let _ = std::fs::remove_file(&archive);
        return Err(WakeError::Assets(format!(
            "{} failed its integrity check (SHA-256 {digest}); the file was deleted",
            asset.label
        )));
    }
    extract(&archive, assets_dir)?;
    let _ = std::fs::remove_file(&archive);
    if !is_installed(assets_dir, asset) {
        return Err(WakeError::Assets(format!(
            "the {} archive did not contain {}",
            asset.label, asset.marker
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_url(path: &Path) -> String {
        format!("file:///{}", path.display().to_string().replace('\\', "/"))
    }

    #[test]
    fn a_wrong_hash_deletes_the_download_and_extracts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.zip");
        std::fs::write(&source, b"not really a zip").unwrap();
        let url: &'static str = Box::leak(file_url(&source).into_boxed_str());
        let asset = Asset {
            label: "test asset",
            url,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            directory: "test-dir",
            marker: "marker.txt",
            size: 16,
        };
        let assets_dir = dir.path().join("assets");
        let error = ensure(&asset, &assets_dir, &|_| {}).unwrap_err();
        assert!(error.to_string().contains("integrity"), "{error}");
        assert!(!assets_dir.join("source.zip").exists(), "archive deleted");
        assert!(!assets_dir.join("test-dir").exists(), "nothing extracted");
    }

    #[test]
    fn partial_files_are_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.zip.part"), b"x").unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
        cleanup_partial(dir.path());
        assert!(!dir.path().join("model.zip.part").exists());
        assert!(dir.path().join("keep.txt").exists());
    }

    #[test]
    fn sha256_matches_a_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.txt");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
