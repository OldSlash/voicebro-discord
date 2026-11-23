//! What the file log holds, shared by the desktop and the headless app (change
//! `cross-platform-headless`, design D7): this app's targets at Info and every other crate
//! at Warn, songbird's receive task at Error, and in the file only this app's event lines
//! and third-party errors unless a debug build or the verbose log writes everything.

/// The file log rotates at 1 MB and keeps one archived file: at most about 2 MB.
pub const LOG_FILE_LIMIT: u64 = 1_000_000;
pub const LOG_FILES_KEPT: usize = 1;
/// This app's own log targets; every other crate logs at Warn and above.
pub const APP_TARGETS: &[&str] = &[
    "voicebro",
    "voicebro_core",
    "voicebro_desktop",
    "voicebro_server",
    "detail",
];
/// songbird's receive task logs an error for every packet it cannot decode; the window
/// keeps them, the release file does not.
pub const RECEIVE_TASK: &str = "songbird::driver::tasks::udp_rx";

pub fn is_app_target(target: &str) -> bool {
    APP_TARGETS.iter().any(|name| {
        target == *name
            || target
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with("::"))
    })
}

/// Whether a record that passed the levels also goes to the file. A debug build and the
/// verbose log write everything; otherwise the file keeps this app's event lines and
/// third-party errors, without detail lines and the receive task's per-packet errors.
pub fn keep_in_file(target: &str, level: log::Level, debug: bool, verbose: bool) -> bool {
    if debug || verbose {
        return true;
    }
    if target.starts_with(crate::DETAIL_TARGET_PREFIX) {
        return false;
    }
    if is_app_target(target) {
        return true;
    }
    level <= log::Level::Error && !target.starts_with(RECEIVE_TASK)
}

/// The levels on top of the global Warn: this app at Info, songbird's receive task at
/// Error (it warns about every RTCP packet it cannot decrypt, about 700 lines in 2.5
/// minutes of a DAVE call, which carries no action).
pub fn log_levels() -> Vec<(&'static str, log::LevelFilter)> {
    APP_TARGETS
        .iter()
        .map(|name| (*name, log::LevelFilter::Info))
        .chain(std::iter::once((RECEIVE_TASK, log::LevelFilter::Error)))
        .collect()
}

/// The level a record of `target` passes at: the most specific entry of [`log_levels`]
/// that names the target or a parent module of it, else the global Warn.
pub fn level_for(target: &str) -> log::LevelFilter {
    log_levels()
        .into_iter()
        .filter(|(name, _)| {
            target == *name
                || target
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with("::"))
        })
        .max_by_key(|(name, _)| name.len())
        .map(|(_, level)| level)
        .unwrap_or(log::LevelFilter::Warn)
}
