//! The headless app's log (change `cross-platform-headless`, design D7): the desktop's
//! filters from the core, every kept line on stderr and in `logs/voicebro.log` of the data
//! folder, rotating at 1 MB with one old file kept.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use voicebro_core::logging;

pub struct ServerLog {
    file: Mutex<LogFile>,
    /// `diagnostics.verbose_log`, read once the settings are loaded.
    verbose: AtomicBool,
}

struct LogFile {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    /// The file could not be opened; only stderr gets the lines from then on.
    failed: bool,
}

impl ServerLog {
    pub fn set_verbose(&self, on: bool) {
        self.verbose.store(on, Ordering::Relaxed);
    }
}

/// Installs the logger for the process; the reference lives as long as the process.
pub fn install(data_dir: &Path) -> &'static ServerLog {
    let logger: &'static ServerLog = Box::leak(Box::new(ServerLog {
        file: Mutex::new(LogFile {
            path: voicebro_core::paths::logs_dir(data_dir).join("voicebro.log"),
            file: None,
            size: 0,
            failed: false,
        }),
        verbose: AtomicBool::new(false),
    }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    logger
}

impl log::Log for ServerLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= logging::level_for(metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata())
            || !logging::keep_in_file(
                record.target(),
                record.level(),
                cfg!(debug_assertions),
                self.verbose.load(Ordering::Relaxed),
            )
        {
            return;
        }
        let line = format!(
            "[{}][{}][{}] {}\n",
            chrono::Local::now().format("%Y-%m-%d][%H:%M:%S"),
            record.target(),
            record.level(),
            record.args()
        );
        eprint!("{line}");
        self.file.lock().expect("log file lock").write(&line);
    }

    fn flush(&self) {
        if let Some(file) = self.file.lock().expect("log file lock").file.as_mut() {
            let _ = file.flush();
        }
    }
}

impl LogFile {
    fn write(&mut self, line: &str) {
        if self.failed {
            return;
        }
        if self.file.is_none() {
            if let Err(error) = self.open() {
                self.failed = true;
                eprintln!(
                    "the log file {} cannot be written ({error}); the log goes to stderr only",
                    self.path.display()
                );
                return;
            }
        }
        if self.size + line.len() as u64 > logging::LOG_FILE_LIMIT {
            self.rotate();
        }
        if let Some(file) = self.file.as_mut() {
            if file.write_all(line.as_bytes()).is_ok() {
                self.size += line.len() as u64;
            }
        }
    }

    fn open(&mut self) -> std::io::Result<()> {
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.size = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        self.file = Some(file);
        Ok(())
    }

    /// `voicebro.log` becomes `voicebro.log.1` (the one old file kept), and a new file
    /// starts.
    fn rotate(&mut self) {
        self.file = None;
        let mut old = self.path.clone().into_os_string();
        old.push(".1");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(&self.path, &old);
        if self.open().is_err() {
            self.failed = true;
        }
    }
}
