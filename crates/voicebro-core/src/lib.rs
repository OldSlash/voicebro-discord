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
pub mod owner;
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

/// The test binary's one logger: it keeps every record, of every test, so a test finds its
/// own lines by a marker only it logs.
#[cfg(test)]
pub(crate) mod test_log {
    use std::sync::{Mutex, Once};

    pub type Record = (String, log::Level, String);

    static RECORDS: Mutex<Vec<Record>> = Mutex::new(Vec::new());

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

    /// Installs the capturing logger at info level, once per test binary.
    pub fn install() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let _ = log::set_boxed_logger(Box::new(Capture));
            log::set_max_level(log::LevelFilter::Info);
        });
    }

    /// The records whose text contains `marker`, in the order they were logged.
    pub fn containing(marker: &str) -> Vec<Record> {
        RECORDS
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, _, text)| text.contains(marker))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod detail_tests {
    #[test]
    fn a_detail_line_logs_at_info_with_its_own_target() {
        crate::test_log::install();
        let marker = "detail-test-marker-17";
        crate::detail!("wake", "[wake] 1/2 heard: {marker}");
        let records = crate::test_log::containing(marker);
        let line = records.first().expect("the line was logged");
        assert_eq!(line.0, "detail::wake");
        assert!(line.0.starts_with(crate::DETAIL_TARGET_PREFIX));
        assert_eq!(line.1, log::Level::Info);
    }
}
