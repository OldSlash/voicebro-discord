//! The one store for every file the app downloads after the user's OK: the wake word's
//! library and models, Whisper's runtimes and models, and the voice engine and voices
//! (change `whisper-mode`, design D10). Pinned hashes, temp-and-rename, extraction with
//! bsdtar, a marker file, cleanup of partial files. Downloads use curl; no HTTP crate.
//! A file the user downloaded by hand and put into the assets
//! folder under its own name is checked and installed instead of downloaded.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// How a downloaded file is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// An archive (zip, wheel, tar.bz2), extracted with bsdtar.
    Archive,
    /// A single file (a Whisper model), hash-checked and moved into `assets\<directory>\`;
    /// it is its own marker.
    File,
}

/// A downloadable file with a pinned hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub label: &'static str,
    /// The official page a person downloads the file from by hand.
    pub page: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    /// Directory the asset is installed to, relative to the assets directory.
    pub directory: &'static str,
    /// A file inside `directory` that proves a completed install (for a single file, the
    /// file's own name).
    pub marker: &'static str,
    /// Expected size in bytes, for progress reporting and the download notice.
    pub size: u64,
    pub kind: AssetKind,
    /// For an archive: the folder inside it whose contents become `directory` (`Release`
    /// of a whisper.cpp zip). Empty when the archive's top folder is `directory` itself.
    pub root: &'static str,
    /// For an archive: the files to keep, as paths relative to `root` with `*` for any
    /// characters. Empty keeps everything; everything else extracted is deleted.
    pub keep: &'static [&'static str],
}

impl Asset {
    /// The defaults of an archive whose top folder is its directory, for struct update
    /// syntax: `Asset { label: …, ..Asset::ARCHIVE }`.
    pub const ARCHIVE: Asset = Asset {
        label: "",
        page: "",
        url: "",
        sha256: "",
        directory: "",
        marker: "",
        size: 0,
        kind: AssetKind::Archive,
        root: "",
        keep: &[],
    };

    /// The file's name: the name a download is saved under and the name a file downloaded
    /// by hand must have in the assets folder. An archive's is the last segment of `url`; a
    /// single file's is its installed name, `marker`, so two models published under the same
    /// name (`ggml-model.bin`) still differ.
    pub fn file_name(&self) -> &'static str {
        match self.kind {
            AssetKind::File => self.marker,
            AssetKind::Archive => self.url.rsplit('/').next().unwrap_or(self.url),
        }
    }
}

/// The end of a "needs a download first" log line: the window's OK on the desktop, the
/// terminal command on the headless app (change `cross-platform-headless`).
pub(crate) fn waiting_for_download(events: &crate::EventBus) -> String {
    if events.terminal_guidance() {
        format!(
            "run voicebro-server {} to install it",
            crate::wake::INSTALL_ASSETS_ARG
        )
    } else {
        "waiting for the OK in the window".to_owned()
    }
}

fn system_tool(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join(name)
    }
    #[cfg(not(windows))]
    {
        // GNU tar does not handle ZIP archives, including the Linux Vosk wheel.
        PathBuf::from(match name {
            "tar.exe" => "bsdtar",
            name => name.trim_end_matches(".exe"),
        })
    }
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
    /// The file is being checked against its pinned hash and installed.
    Unpacking,
}

pub fn is_installed(assets_dir: &Path, asset: &Asset) -> bool {
    assets_dir
        .join(asset.directory)
        .join(asset.marker)
        .is_file()
}

/// Where a download lands, and where a file downloaded by hand is looked for.
pub fn archive_path(assets_dir: &Path, asset: &Asset) -> PathBuf {
    assets_dir.join(asset.file_name())
}

/// The folder an archive with a `root` or a `keep` list is first extracted into.
fn staging_dir(assets_dir: &Path, asset: &Asset) -> PathBuf {
    assets_dir.join(format!(".unpack-{}", asset.directory))
}

/// Removes partial downloads and half-done extractions left by an interrupted run. Called
/// at start.
pub fn cleanup_partial(assets_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(assets_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.extension().and_then(|e| e.to_str()) == Some("part")
            && std::fs::remove_file(&path).is_ok()
        {
            log::info!("removed a partial download: {}", path.display());
        } else if name.starts_with(".unpack-")
            && path.is_dir()
            && std::fs::remove_dir_all(&path).is_ok()
        {
            log::info!("removed a half-done extraction: {}", path.display());
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

/// Downloads `url` to `target` through curl, calling `progress` with a percentage
/// (0..=100, in steps of at least 20) while the file grows.
fn download(
    url: &str,
    target: &Path,
    expected_size: u64,
    progress: &dyn Fn(u8),
) -> Result<(), String> {
    let _ = std::fs::remove_file(target);
    let tool = system_tool("curl.exe");
    let mut command = Command::new(&tool);
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
        .map_err(|error| format!("running {}: {error}", tool.display()))?;
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
                    return Err(format!(
                        "download of {url} failed ({status}): {}",
                        stderr.trim()
                    ));
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
                return Err(format!("waiting for {}: {error}", tool.display()));
            }
        }
    }
    progress(100);
    Ok(())
}

fn extract(archive: &Path, destination: &Path) -> Result<(), String> {
    let tool = system_tool("tar.exe");
    let mut command = Command::new(&tool);
    command.arg("-xf").arg(archive).arg("-C").arg(destination);
    hide_window(&mut command);
    let output = command
        .output()
        .map_err(|error| format!("running {}: {error}", tool.display()))?;
    if !output.status.success() {
        return Err(format!(
            "extracting {}: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// Whether `path` (relative, with `/`) matches `pattern`, where `*` stands for any run of
/// characters.
fn matches_pattern(pattern: &str, path: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == path;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !path.starts_with(first) || !path[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &path[first.len()..path.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

/// Every file below `dir`, as paths relative to it with `/`.
fn files_below(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(dir) {
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// Extracts an archive with a `root` or a `keep` list: into a staging folder first, then
/// the kept files of `root` into `assets\<directory>`, and the rest is deleted.
fn extract_kept(archive: &Path, assets_dir: &Path, asset: &Asset) -> Result<(), String> {
    let staging = staging_dir(assets_dir, asset);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|error| format!("creating {}: {error}", staging.display()))?;
    let result = (|| {
        extract(archive, &staging)?;
        let source = if asset.root.is_empty() {
            staging.join(asset.directory)
        } else {
            staging.join(asset.root)
        };
        if !source.is_dir() {
            return Err(format!(
                "the {} archive has no folder {}",
                asset.label,
                if asset.root.is_empty() {
                    asset.directory
                } else {
                    asset.root
                }
            ));
        }
        let target = assets_dir.join(asset.directory);
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target)
            .map_err(|error| format!("creating {}: {error}", target.display()))?;
        for file in files_below(&source) {
            if !asset.keep.is_empty()
                && !asset
                    .keep
                    .iter()
                    .any(|pattern| matches_pattern(pattern, &file))
            {
                continue;
            }
            let to = target.join(&file);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("creating {}: {error}", parent.display()))?;
            }
            std::fs::rename(source.join(&file), &to)
                .map_err(|error| format!("moving {file}: {error}"))?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Makes sure `asset` is downloaded, verified and installed under `assets_dir`. A file
/// already in the folder under its own name (one the user downloaded by hand) is used
/// instead of a download. Either way `progress` hears [`Progress::Unpacking`] before the
/// hash check. A hash mismatch deletes the file and installs nothing.
pub fn ensure(asset: &Asset, assets_dir: &Path, progress: &dyn Fn(Progress)) -> Result<(), String> {
    if is_installed(assets_dir, asset) {
        return Ok(());
    }
    std::fs::create_dir_all(assets_dir)
        .map_err(|error| format!("creating {}: {error}", assets_dir.display()))?;
    let file_name = asset.file_name();
    let downloaded = archive_path(assets_dir, asset);
    let part = assets_dir.join(format!("{file_name}.part"));
    let found = downloaded.is_file();
    if !found {
        download(asset.url, &part, asset.size, &|percent| {
            progress(Progress::Downloaded(percent))
        })?;
        std::fs::rename(&part, &downloaded)
            .map_err(|error| format!("renaming the download: {error}"))?;
    }
    progress(Progress::Unpacking);
    let digest = sha256_file(&downloaded)
        .map_err(|error| format!("hashing {}: {error}", downloaded.display()))?;
    if digest != asset.sha256 {
        let _ = std::fs::remove_file(&downloaded);
        let route = if found {
            "found in the assets folder"
        } else {
            "downloaded"
        };
        return Err(format!(
            "{file_name} ({route}) does not match the pinned SHA-256 ({digest}); it was deleted"
        ));
    }
    match asset.kind {
        AssetKind::File => {
            let folder = assets_dir.join(asset.directory);
            std::fs::create_dir_all(&folder)
                .map_err(|error| format!("creating {}: {error}", folder.display()))?;
            std::fs::rename(&downloaded, folder.join(asset.marker))
                .map_err(|error| format!("moving {file_name}: {error}"))?;
        }
        AssetKind::Archive if asset.root.is_empty() && asset.keep.is_empty() => {
            extract(&downloaded, assets_dir)?;
            let _ = std::fs::remove_file(&downloaded);
        }
        AssetKind::Archive => {
            extract_kept(&downloaded, assets_dir, asset)?;
            let _ = std::fs::remove_file(&downloaded);
        }
    }
    if !is_installed(assets_dir, asset) {
        return Err(format!(
            "the {} download did not contain {}",
            asset.label, asset.marker
        ));
    }
    Ok(())
}

/// For tests: a zip of `<folder>\<marker>` made with the system `tar.exe` at
/// `<dir>\<name>`, with its SHA-256.
#[cfg(test)]
pub(crate) fn test_zip(dir: &Path, folder: &str, marker: &str, name: &str) -> (PathBuf, String) {
    test_archive(dir, folder, &[marker], name)
}

/// For tests: an archive of the files `<folder>\<file>` made with the system `tar.exe` at
/// `<dir>\<name>` (a zip, or a `.tar.bz2` by the name), with its SHA-256.
#[cfg(test)]
pub(crate) fn test_archive(
    dir: &Path,
    folder: &str,
    files: &[&str],
    name: &str,
) -> (PathBuf, String) {
    let source = dir.join(format!("archive-source-{name}"));
    for file in files {
        let path = source.join(folder).join(file);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("source folder");
        std::fs::write(&path, file.as_bytes()).expect("source file");
    }
    let archive = dir.join(name);
    let mut command = Command::new(system_tool("tar.exe"));
    if name.ends_with(".tar.bz2") {
        command.arg("-cjf");
    } else {
        command.arg("-a").arg("-cf");
    }
    let status = command
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg(folder)
        .status()
        .expect("running tar.exe");
    assert!(status.success(), "tar.exe made {name}");
    let digest = sha256_file(&archive).expect("hashing the test archive");
    (archive, digest)
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
            ..Asset::ARCHIVE
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
            ..Asset::ARCHIVE
        };
        let assets_dir = dir.path().join("assets");
        let error = ensure(&asset, &assets_dir, &|_| {}).unwrap_err();
        assert!(
            error.starts_with("source.zip (downloaded) does not match the pinned SHA-256"),
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
            error.starts_with(
                "test-dir.zip (found in the assets folder) does not match the pinned SHA-256"
            ),
            "{error}"
        );
        assert!(error.ends_with("it was deleted"), "{error}");
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
        std::fs::create_dir_all(dir.path().join(".unpack-whisper-cpu").join("Release")).unwrap();
        cleanup_partial(dir.path());
        assert!(!dir.path().join("model.zip.part").exists());
        assert!(!dir.path().join(".unpack-whisper-cpu").exists());
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

    /// A single file (a Whisper model) at `<dir>\served\<name>` with its hash.
    fn served_file(dir: &Path, name: &str, body: &[u8]) -> (PathBuf, &'static str) {
        let served = dir.join("served");
        std::fs::create_dir_all(&served).unwrap();
        let path = served.join(name);
        std::fs::write(&path, body).unwrap();
        let digest = sha256_file(&path).unwrap();
        (path, leak(digest))
    }

    fn file_asset(url: &'static str, sha256: &'static str, size: u64) -> Asset {
        Asset {
            label: "Whisper test model",
            page: "https://example.test/models",
            url,
            sha256,
            directory: "whisper-test-model",
            marker: "ggml-test.bin",
            size,
            kind: AssetKind::File,
            ..Asset::ARCHIVE
        }
    }

    /// Task 2.2 of change `whisper-mode`: a single file from a `file:///` URL is checked
    /// and moved into its folder, where it is its own marker.
    #[test]
    fn a_single_file_is_downloaded_checked_and_moved_into_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (path, digest) = served_file(dir.path(), "ggml-test.bin", b"a small model");
        let asset = file_asset(leak(file_url(&path)), digest, 13);
        let assets_dir = dir.path().join("assets");
        ensure(&asset, &assets_dir, &|_| {}).expect("installed");
        assert!(is_installed(&assets_dir, &asset));
        assert_eq!(
            std::fs::read(assets_dir.join("whisper-test-model").join("ggml-test.bin")).unwrap(),
            b"a small model"
        );
        assert!(
            !assets_dir.join("ggml-test.bin").exists(),
            "moved, not copied"
        );
    }

    #[test]
    fn a_single_file_put_into_the_folder_by_hand_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let (path, digest) = served_file(dir.path(), "made.bin", b"a small model");
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::copy(&path, assets_dir.join("ggml-test.bin")).unwrap();
        let asset = file_asset(
            leak(file_url(&dir.path().join("nowhere").join("ggml-test.bin"))),
            digest,
            13,
        );
        let (seen, progress) = recorder();
        ensure(&asset, &assets_dir, &progress).expect("used from the folder");
        assert_eq!(*seen.lock().unwrap(), [Progress::Unpacking], "no download");
        assert!(is_installed(&assets_dir, &asset));
    }

    #[test]
    fn a_single_file_with_a_wrong_hash_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = served_file(dir.path(), "ggml-test.bin", b"a small model");
        let asset = file_asset(
            leak(file_url(&path)),
            "0000000000000000000000000000000000000000000000000000000000000000",
            13,
        );
        let assets_dir = dir.path().join("assets");
        let error = ensure(&asset, &assets_dir, &|_| {}).unwrap_err();
        assert!(
            error.contains("does not match the pinned SHA-256"),
            "{error}"
        );
        assert!(!assets_dir.join("ggml-test.bin").exists());
        assert!(!is_installed(&assets_dir, &asset));
    }

    /// Task 2.3: an archive's kept-files list, with a root folder inside the archive.
    #[test]
    fn only_the_kept_files_of_an_archive_remain() {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            "whisper.dll",
            "ggml-cpu-haswell.dll",
            "whisper-cli.exe",
            "SDL2.dll",
            "notes/readme.txt",
        ];
        let (zip, digest) = test_archive(dir.path(), "Release", &files, "runtime.zip");
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::copy(&zip, assets_dir.join("runtime.zip")).unwrap();
        let asset = Asset {
            label: "Whisper test runtime",
            page: "https://example.test/releases",
            url: leak(file_url(&dir.path().join("nowhere").join("runtime.zip"))),
            sha256: leak(digest),
            directory: "whisper-test-runtime",
            marker: "whisper.dll",
            size: 100,
            root: "Release",
            keep: &["whisper.dll", "ggml-cpu-*.dll"],
            ..Asset::ARCHIVE
        };
        ensure(&asset, &assets_dir, &|_| {}).expect("installed");
        assert_eq!(
            files_below(&assets_dir.join("whisper-test-runtime")),
            ["ggml-cpu-haswell.dll", "whisper.dll"]
        );
        assert!(
            !assets_dir.join("Release").exists(),
            "nothing else extracted"
        );
        assert!(!assets_dir.join(".unpack-whisper-test-runtime").exists());
        assert!(!assets_dir.join("runtime.zip").exists());
    }

    /// Task 2.4: the system `tar.exe` (bsdtar) unpacks a `.tar.bz2`, as the voice archives
    /// are, so no Rust bzip2 crate is needed.
    #[test]
    fn the_system_tar_unpacks_a_tar_bz2() {
        let dir = tempfile::tempdir().unwrap();
        let (archive, digest) = test_archive(
            dir.path(),
            "vits-piper-test",
            &["model.onnx", "espeak-ng-data/phontab"],
            "vits-piper-test.tar.bz2",
        );
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::copy(&archive, assets_dir.join("vits-piper-test.tar.bz2")).unwrap();
        let asset = Asset {
            label: "Piper test voice",
            page: "https://example.test/voices",
            url: leak(file_url(
                &dir.path().join("nowhere").join("vits-piper-test.tar.bz2"),
            )),
            sha256: leak(digest),
            directory: "vits-piper-test",
            marker: "model.onnx",
            size: 100,
            ..Asset::ARCHIVE
        };
        ensure(&asset, &assets_dir, &|_| {}).expect("unpacked");
        assert!(assets_dir
            .join("vits-piper-test")
            .join("espeak-ng-data")
            .join("phontab")
            .is_file());
    }

    #[test]
    fn patterns_take_a_star_for_any_characters() {
        assert!(matches_pattern("ggml-cpu-*.dll", "ggml-cpu-alderlake.dll"));
        assert!(!matches_pattern("ggml-cpu-*.dll", "ggml-cuda.dll"));
        assert!(matches_pattern("whisper.dll", "whisper.dll"));
        assert!(!matches_pattern("whisper.dll", "whisper.dll.bak"));
        assert!(matches_pattern(
            "espeak-ng-data/*",
            "espeak-ng-data/ru_dict"
        ));
        assert!(matches_pattern("*", "anything/at/all"));
    }
}
