//! The app log for the window: every record the log file gets (the same level filters and
//! the same formatting) is kept in a ring of the last 500 lines and emitted to the window as
//! `app-log`. The window asks for the ring when it loads, so it also shows what was logged
//! before it existed.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

/// Lines kept for the window.
pub const CAPACITY: usize = 500;

thread_local! {
    /// Set while a line is being emitted, so a record logged by the emit itself is kept but
    /// not emitted again.
    static EMITTING: Cell<bool> = const { Cell::new(false) };
}

#[derive(Default)]
pub struct LogRing {
    lines: Mutex<VecDeque<String>>,
    app: OnceLock<AppHandle>,
}

impl LogRing {
    /// Keeps `line` and sends it to the window once one is attached.
    pub fn push(&self, line: String) {
        {
            let mut lines = self.lines.lock().expect("log ring lock");
            if lines.len() >= CAPACITY {
                lines.pop_front();
            }
            lines.push_back(line.clone());
        }
        let Some(app) = self.app.get() else {
            return;
        };
        EMITTING.with(|emitting| {
            if emitting.replace(true) {
                return;
            }
            let _ = app.emit("app-log", &line);
            emitting.set(false);
        });
    }

    /// The kept lines, oldest first.
    pub fn history(&self) -> Vec<String> {
        self.lines
            .lock()
            .expect("log ring lock")
            .iter()
            .cloned()
            .collect()
    }

    /// From now on every line is also emitted to the window.
    pub fn attach(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_the_last_lines_in_order() {
        let ring = LogRing::default();
        for index in 0..CAPACITY + 20 {
            ring.push(format!("line {index}"));
        }
        let history = ring.history();
        assert_eq!(history.len(), CAPACITY);
        assert_eq!(history[0], "line 20");
        assert_eq!(history.last().unwrap(), &format!("line {}", CAPACITY + 19));
    }
}
