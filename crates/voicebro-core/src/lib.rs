//! VoiceBro application core.
//!
//! Everything the app does lives here behind types and traits; `apps/desktop` only wires
//! these to Tauri commands and events. This crate has no Tauri dependency, so its tests run
//! without a window. See `docs/ARCHITECTURE.md`.

/// The log target prefix of detail lines: recognized speech, per-turn latency, call and
/// delivery counters, speaker mapping, copies of chat entries. The window shows them; a
/// release build writes them to the file only while the verbose log is on
/// (`docs/ARCHITECTURE.md` 7).
pub const DETAIL_TARGET_PREFIX: &str = "detail::";

/// Logs a detail line at Info with the target `detail::<area>`.
#[macro_export]
macro_rules! detail {
    ($area:literal, $($arg:tt)+) => {
        ::log::info!(target: concat!("detail::", $area), $($arg)+)
    };
}

pub mod assistant;
pub mod audio;
pub mod discord;
pub mod events;
pub mod gate;
pub mod latency;
pub mod paths;
pub mod runtime;
pub mod secrets;
pub mod settings;
pub mod status;
pub mod wake;

pub use events::{AppEvent, EventBus};
pub use runtime::Runtime;
pub use secrets::{SecretKey, SecretStore};
pub use settings::Settings;
pub use status::Status;

#[cfg(test)]
mod detail_tests {
    use std::sync::Mutex;

    static RECORDS: Mutex<Vec<(String, log::Level, String)>> = Mutex::new(Vec::new());

    struct Capture;
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            RECORDS.lock().unwrap().push((
                record.target().to_owned(),
                record.level(),
                record.args().to_string(),
            ));
        }
        fn flush(&self) {}
    }

    #[test]
    fn a_detail_line_logs_at_info_with_its_own_target() {
        // The only test that installs a logger; other tests' records land here too.
        let _ = log::set_boxed_logger(Box::new(Capture));
        log::set_max_level(log::LevelFilter::Info);
        let marker = "detail-test-marker-17";
        crate::detail!("wake", "[wake] 1/2 heard: {marker}");
        let records = RECORDS.lock().unwrap();
        let line = records
            .iter()
            .find(|(_, _, text)| text.contains(marker))
            .expect("the line was logged");
        assert_eq!(line.0, "detail::wake");
        assert!(line.0.starts_with(crate::DETAIL_TARGET_PREFIX));
        assert_eq!(line.1, log::Level::Info);
    }
}
