//! The pinned Vosk assets still download, verify and load.
//!
//! Downloads go through the system `curl.exe` to `<name>.part`, are hashed with SHA-256,
//! renamed, extracted with the system `tar.exe`, and `libvosk.dll` is loaded at runtime with
//! `LOAD_WITH_ALTERED_SEARCH_PATH` so its MinGW runtime DLLs resolve from the same folder.

use crate::Outcome;
use anyhow::{anyhow, bail, Context};
use sha2::{Digest, Sha256};
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::{Path, PathBuf};

pub struct Asset {
    pub label: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub directory: &'static str,
    pub marker: &'static str,
}

pub const VOSK_LIBRARY: Asset = Asset {
    label: "Vosk library",
    url: "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-win64-0.3.45.zip",
    sha256: "f1dcc9cca460630f81ea8f71794f69c80bed6556d2a4e6237b5785e1d2dff34b",
    directory: "vosk-win64-0.3.45",
    marker: "libvosk.dll",
};

pub const VOSK_MODEL: Asset = Asset {
    label: "Vosk Russian model",
    url: "https://alphacephei.com/vosk/models/vosk-model-small-ru-0.22.zip",
    sha256: "961d5ff98a17f4aa6de69864d0aa71fa5bac682301d2b5d17a3f24c5c99a46d4",
    directory: "vosk-model-small-ru-0.22",
    marker: "am/final.mdl",
};

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
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
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

/// Downloads, verifies and extracts one asset. Returns the lines to report.
fn fetch(asset: &Asset, dir: &Path) -> anyhow::Result<(bool, Vec<String>)> {
    let mut lines = Vec::new();
    let target_dir = dir.join(asset.directory);
    if target_dir.join(asset.marker).is_file() {
        lines.push(format!(
            "{}: already extracted at {}",
            asset.label,
            target_dir.display()
        ));
        return Ok((true, lines));
    }
    let file_name = asset.url.rsplit('/').next().unwrap_or("asset.zip");
    let archive = dir.join(file_name);
    let part = dir.join(format!("{file_name}.part"));
    if !archive.is_file() {
        let _ = std::fs::remove_file(&part);
        let started = std::time::Instant::now();
        let status = std::process::Command::new(system_tool("curl.exe"))
            .args(["-L", "--fail", "--silent", "--show-error", "-o"])
            .arg(&part)
            .arg(asset.url)
            .status()
            .context("running curl.exe")?;
        if !status.success() {
            bail!("{}: download failed ({status})", asset.label);
        }
        std::fs::rename(&part, &archive)?;
        let size = std::fs::metadata(&archive)?.len();
        lines.push(format!(
            "{}: downloaded {:.1} MB in {:.1?}",
            asset.label,
            size as f64 / 1e6,
            started.elapsed()
        ));
    } else {
        lines.push(format!("{}: archive already present", asset.label));
    }
    let hash = sha256_file(&archive)?;
    let ok = hash == asset.sha256;
    lines.push(format!(
        "{}: sha256 {hash} {}",
        asset.label,
        if ok {
            "matches the pin"
        } else {
            "DOES NOT MATCH the pin"
        }
    ));
    if !ok {
        return Ok((false, lines));
    }
    let status = std::process::Command::new(system_tool("tar.exe"))
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .status()
        .context("running tar.exe")?;
    if !status.success() {
        bail!("{}: tar.exe exited with {status}", asset.label);
    }
    let extracted = target_dir.join(asset.marker).is_file();
    lines.push(format!(
        "{}: extracted, marker {} present={extracted}",
        asset.label, asset.marker
    ));
    Ok((extracted, lines))
}

type SetLogLevel = unsafe extern "C" fn(c_int);
type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type ModelFree = unsafe extern "C" fn(*mut c_void);
type ModelFindWord = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;

/// Loads the DLL and the model, checks a few vocabulary words, frees everything.
fn load_model(library: &Path, model_dir: &Path) -> anyhow::Result<Vec<String>> {
    use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};
    let mut lines = Vec::new();
    let started = std::time::Instant::now();
    // SAFETY: loading a library runs its initializers; libvosk has no side effects beyond
    // thread-pool setup.
    let lib = unsafe { Library::load_with_flags(library, LOAD_WITH_ALTERED_SEARCH_PATH) }
        .with_context(|| format!("loading {}", library.display()))?;
    // SAFETY: the symbol names and signatures come from vosk_api.h.
    let (set_log_level, model_new, model_free, find_word) = unsafe {
        (
            *lib.get::<SetLogLevel>(b"vosk_set_log_level\0")?,
            *lib.get::<ModelNew>(b"vosk_model_new\0")?,
            *lib.get::<ModelFree>(b"vosk_model_free\0")?,
            *lib.get::<ModelFindWord>(b"vosk_model_find_word\0")?,
        )
    };
    lines.push(format!("libvosk.dll loaded in {:.1?}", started.elapsed()));
    let path = CString::new(model_dir.to_string_lossy().as_bytes())?;
    let started = std::time::Instant::now();
    // SAFETY: plain FFI calls with valid C strings; the model pointer is freed below.
    let model = unsafe {
        set_log_level(-1);
        model_new(path.as_ptr())
    };
    if model.is_null() {
        bail!("vosk_model_new returned null for {}", model_dir.display());
    }
    lines.push(format!("vosk_model_new ok in {:.1?}", started.elapsed()));
    for word in [
        "джи",
        "пи",
        "ти",
        "луна",
        "отбой",
        "братан",
        "джипити",
        "гпт",
    ] {
        let c_word = CString::new(word)?;
        // SAFETY: valid model pointer and C string.
        let index = unsafe { find_word(model, c_word.as_ptr()) };
        lines.push(format!(
            "  word «{word}»: {}",
            if index >= 0 { "known" } else { "unknown" }
        ));
    }
    // SAFETY: the model pointer is valid and used nowhere else.
    unsafe { model_free(model) };
    Ok(lines)
}

pub fn assets(args: &[String]) -> anyhow::Result<Outcome> {
    let dir = args
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("usage: probe vosk-assets <dir>"))?;
    std::fs::create_dir_all(&dir)?;
    let mut details = Vec::new();
    let mut pass = true;
    for asset in [&VOSK_LIBRARY, &VOSK_MODEL] {
        match fetch(asset, &dir) {
            Ok((ok, lines)) => {
                details.extend(lines);
                pass &= ok;
            }
            Err(error) => {
                details.push(format!("{}: {error:#}", asset.label));
                pass = false;
            }
        }
    }
    if pass {
        match load_model(
            &dir.join(VOSK_LIBRARY.directory).join("libvosk.dll"),
            &dir.join(VOSK_MODEL.directory),
        ) {
            Ok(lines) => details.extend(lines),
            Err(error) => {
                details.push(format!("load: {error:#}"));
                pass = false;
            }
        }
    }
    details.push(
        "criterion: both hashes match the pins and vosk_model_new returns a model".to_owned(),
    );
    Ok(Outcome { pass, details })
}
