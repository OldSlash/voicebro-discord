//! First-use download of recognizer assets: pinned hashes, temp-and-rename, extraction
//! with the system `tar.exe`, a marker file, cleanup of partial files. Downloads use the
//! system `curl.exe`; no HTTP crate. An archive the user downloaded by hand and put into
//! the assets folder under its own name is checked and extracted instead of downloaded.

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

/// What [`ensure`] is doing, for the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// The download, in percent (0..=100, in steps of at least 20).
    Downloaded(u8),
    /// The archive is being checked against its pinned hash and extracted.
    Unpacking,
}

pub fn is_installed(assets_dir: &Path, asset: &Asset) -> bool {
    assets_dir
        .join(asset.directory)
        .join(asset.marker)
        .is_file()
}

/// Where the archive lands after a download, and where a file downloaded by hand is
/// looked for.
pub fn archive_path(assets_dir: &Path, asset: &Asset) -> PathBuf {
    assets_dir.join(asset.file_name())
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

/// Makes sure `asset` is downloaded, verified and extracted under `assets_dir`. An archive
/// already in the folder under its own name (one the user downloaded by hand) is used
/// instead of a download. Either way `progress` hears [`Progress::Unpacking`] before the
/// hash check. A hash mismatch deletes the archive and extracts nothing.
pub fn ensure(
    asset: &Asset,
    assets_dir: &Path,
    progress: &dyn Fn(Progress),
) -> Result<(), WakeError> {
    if is_installed(assets_dir, asset) {
        return Ok(());
    }
    std::fs::create_dir_all(assets_dir).map_err(|error| {
        WakeError::Assets(format!("creating {}: {error}", assets_dir.display()))
    })?;
    let file_name = asset.file_name();
    let archive = archive_path(assets_dir, asset);
    let part = assets_dir.join(format!("{file_name}.part"));
    let found = archive.is_file();
    if !found {
        download(asset.url, &part, asset.size, &|percent| {
            progress(Progress::Downloaded(percent))
        })?;
        std::fs::rename(&part, &archive)
            .map_err(|error| WakeError::Assets(format!("renaming the download: {error}")))?;
    }
    progress(Progress::Unpacking);
    let digest = sha256_file(&archive)
        .map_err(|error| WakeError::Assets(format!("hashing {}: {error}", archive.display())))?;
    if digest != asset.sha256 {
        let _ = std::fs::remove_file(&archive);
        let route = if found {
            "found in the assets folder"
        } else {
            "downloaded"
        };
        return Err(WakeError::Assets(format!(
            "{file_name} ({route}) does not match the pinned SHA-256 ({digest}); it was deleted"
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

/// For tests: a zip of `<folder>\<marker>` made with the system `tar.exe` at
/// `<dir>\<name>`, with its SHA-256.
#[cfg(test)]
pub(crate) fn test_zip(dir: &Path, folder: &str, marker: &str, name: &str) -> (PathBuf, String) {
    let source = dir.join(format!("zip-source-{name}"));
    let file = source.join(folder).join(marker);
    std::fs::create_dir_all(file.parent().expect("a folder")).expect("source folder");
    std::fs::write(&file, b"model").expect("source file");
    let zip = dir.join(name);
    let status = Command::new(system_tool("tar.exe"))
        .arg("-a")
        .arg("-cf")
        .arg(&zip)
        .arg("-C")
        .arg(&source)
        .arg(folder)
        .status()
        .expect("running tar.exe");
    assert!(status.success(), "tar.exe made {name}");
    let digest = sha256_file(&zip).expect("hashing the test zip");
    (zip, digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    fn file_url(path: &Path) -> String {
        format!("file:///{}", path.display().to_string().replace('\\', "/"))
    }

    fn leak(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    fn make_zip(dir: &Path, name: &str) -> (PathBuf, String) {
        test_zip(dir, "test-dir", "marker.txt", name)
    }

    /// An asset whose download would fail: its URL points at a file that does not exist.
    fn unreachable_asset(dir: &Path, sha256: &'static str) -> Asset {
        Asset {
            label: "test asset",
            page: "https://example.test/models",
            url: leak(file_url(&dir.join("nowhere").join("test-dir.zip"))),
            sha256,
            directory: "test-dir",
            marker: "marker.txt",
            size: 16,
        }
    }

    fn recorder() -> (Arc<Mutex<Vec<Progress>>>, impl Fn(Progress)) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        (seen, move |step| sink.lock().unwrap().push(step))
    }

    #[test]
    fn a_wrong_hash_deletes_the_download_and_extracts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.zip");
        std::fs::write(&source, b"not really a zip").unwrap();
        let asset = Asset {
            label: "test asset",
            page: "https://example.test/models",
            url: leak(file_url(&source)),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            directory: "test-dir",
            marker: "marker.txt",
            size: 16,
        };
        let assets_dir = dir.path().join("assets");
        let error = ensure(&asset, &assets_dir, &|_| {}).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("source.zip (downloaded) does not match the pinned SHA-256"),
            "{error}"
        );
        assert!(!assets_dir.join("source.zip").exists(), "archive deleted");
        assert!(!assets_dir.join("test-dir").exists(), "nothing extracted");
    }

    #[test]
    fn an_archive_put_into_the_folder_is_used_instead_of_a_download() {
        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        let (zip, digest) = make_zip(dir.path(), "made.zip");
        std::fs::copy(&zip, assets_dir.join("test-dir.zip")).unwrap();
        let asset = unreachable_asset(dir.path(), leak(digest));
        let (seen, progress) = recorder();
        ensure(&asset, &assets_dir, &progress).expect("used from the folder");
        assert_eq!(*seen.lock().unwrap(), [Progress::Unpacking], "no download");
        assert!(is_installed(&assets_dir, &asset));
        assert!(!assets_dir.join("test-dir.zip").exists(), "archive removed");
    }

    #[test]
    fn a_wrong_archive_in_the_folder_is_deleted_and_nothing_is_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("test-dir.zip"), b"another file").unwrap();
        let asset = unreachable_asset(
            dir.path(),
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        let (seen, progress) = recorder();
        let error = ensure(&asset, &assets_dir, &progress).unwrap_err();
        assert!(
            error.to_string().starts_with(
                "test-dir.zip (found in the assets folder) does not match the pinned SHA-256"
            ),
            "{error}"
        );
        assert!(error.to_string().ends_with("it was deleted"), "{error}");
        assert_eq!(*seen.lock().unwrap(), [Progress::Unpacking], "no download");
        assert!(!assets_dir.join("test-dir.zip").exists(), "file deleted");
        assert!(!assets_dir.join("test-dir").exists(), "nothing extracted");
    }

    #[test]
    fn an_extracted_folder_counts_as_installed_without_an_archive() {
        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(assets_dir.join("test-dir")).unwrap();
        std::fs::write(assets_dir.join("test-dir").join("marker.txt"), b"").unwrap();
        let asset = unreachable_asset(
            dir.path(),
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        let (seen, progress) = recorder();
        ensure(&asset, &assets_dir, &progress).expect("already installed");
        assert!(seen.lock().unwrap().is_empty(), "nothing to do");
        assert!(is_installed(&assets_dir, &asset));
    }

    #[test]
    fn a_download_reports_its_percentages_and_then_unpacking() {
        let dir = tempfile::tempdir().unwrap();
        let served = dir.path().join("served");
        std::fs::create_dir_all(&served).unwrap();
        let (zip, digest) = make_zip(dir.path(), "made.zip");
        std::fs::copy(&zip, served.join("test-dir.zip")).unwrap();
        let asset = Asset {
            url: leak(file_url(&served.join("test-dir.zip"))),
            size: std::fs::metadata(&zip).unwrap().len(),
            ..unreachable_asset(dir.path(), leak(digest))
        };
        let assets_dir = dir.path().join("assets");
        let (seen, progress) = recorder();
        ensure(&asset, &assets_dir, &progress).expect("downloaded");
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.first(), Some(&Progress::Downloaded(0)), "{seen:?}");
        assert_eq!(seen.last(), Some(&Progress::Unpacking), "{seen:?}");
        assert_eq!(seen[seen.len() - 2], Progress::Downloaded(100), "{seen:?}");
        assert!(is_installed(&assets_dir, &asset));
        assert!(!assets_dir.join("test-dir.zip").exists(), "archive removed");
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
