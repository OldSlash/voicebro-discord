//! Build script: the Common-Controls v6 manifest for every linked target, and the build
//! stamp the UI and the log show.

use std::process::Command;

fn main() {
    println!("cargo:rustc-env=VOICEBRO_BUILD_STAMP={}", build_stamp());
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");

    let mut attributes = tauri_build::Attributes::new();
    #[cfg(windows)]
    {
        // tauri-build only embeds its Common-Controls v6 manifest into the app binary.
        // The test executables also import comctl32 v6 entry points (TaskDialogIndirect),
        // so without a manifest they exit with STATUS_ENTRYPOINT_NOT_FOUND before any test
        // runs. Embed the same manifest into every linked target instead.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        embed_windows_manifest();
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}

#[cfg(windows)]
fn embed_windows_manifest() {
    let manifest = std::env::current_dir()
        .expect("build script has a working directory")
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}

/// `<version>+<short hash>[.dirty] <YYYY-MM-DD>`; `nogit` when git is unavailable.
fn build_stamp() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "nogit".to_owned());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|out| !out.is_empty())
        .unwrap_or(false);
    let date = utc_date();
    format!(
        "{version}+{hash}{} {date}",
        if dirty { ".dirty" } else { "" }
    )
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Today's date in UTC from the system clock, without a date crate.
fn utc_date() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}
