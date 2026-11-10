//! What the System tab and the status bar show about the running process: version, build
//! stamp, process id, uptime, runtime, and the operating system's platform, architecture
//! and CPU cores. Everything is read from the process itself.

use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemFacts {
    pub version: String,
    pub build_stamp: String,
    pub process_id: u32,
    /// Whole seconds since the app started.
    pub uptime_seconds: u64,
    pub runtime: String,
    pub platform: String,
    pub architecture: String,
    pub cpu_cores: usize,
}

/// The facts as of `now`, for an app that started at `started`.
pub fn facts(build_stamp: &str, started: Instant, now: Instant) -> SystemFacts {
    SystemFacts {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        build_stamp: build_stamp.to_owned(),
        process_id: std::process::id(),
        uptime_seconds: now.saturating_duration_since(started).as_secs(),
        runtime: "Rust / Tauri 2 / WebView2".to_owned(),
        platform: platform(),
        architecture: std::env::consts::ARCH.to_owned(),
        cpu_cores: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
    }
}

/// "Windows" with the build number when the system reports it.
fn platform() -> String {
    match std::env::consts::OS {
        "windows" => windows_build()
            .map(|build| format!("Windows (build {build})"))
            .unwrap_or_else(|| "Windows".to_owned()),
        other => other.to_owned(),
    }
}

#[cfg(windows)]
fn windows_build() -> Option<String> {
    // `ver` prints "Microsoft Windows [Version 10.0.26200.xxxx]".
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find("Version ")? + "Version ".len();
    let version = text[start..].trim().trim_end_matches(']');
    (!version.is_empty()).then(|| version.to_owned())
}

#[cfg(not(windows))]
fn windows_build() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn uptime_counts_whole_seconds_from_the_start() {
        let started = Instant::now();
        let facts = facts(
            "0.2.0+test",
            started,
            started + Duration::from_millis(65_900),
        );
        assert_eq!(facts.uptime_seconds, 65);
        assert_eq!(facts.build_stamp, "0.2.0+test");
        assert_eq!(facts.process_id, std::process::id());
        assert!(facts.cpu_cores >= 1);
        assert!(!facts.platform.is_empty() && !facts.architecture.is_empty());
        assert_eq!(
            super::facts("x", started + Duration::from_secs(5), started).uptime_seconds,
            0,
            "a clock that went back gives zero, not a panic"
        );
    }
}
