//! The memory of the app's processes as Task Manager shows it: the private working set of
//! `voicebro.exe`, of the WebView2 processes it started and of its Codex process (with that
//! process's helpers). Read every 3 s on a thread of its own while the `diagnostics.memory`
//! setting is on; the window asks for the latest reading, which is `None` while it is off.

use serde::Serialize;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often the reading is taken.
pub const INTERVAL: Duration = Duration::from_secs(3);

/// Where the latest reading is kept; `None` while no reader runs.
pub type Slot = Arc<Mutex<Option<MemoryReading>>>;

/// One entry of the system's process list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
}

/// What a process of the app's tree is counted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    App,
    WebView,
    Codex,
}

/// The app's process and its descendants that count: WebView2 processes and the Codex
/// process with everything it started. Other descendants are not counted.
pub fn app_tree(root: u32, all: &[ProcessInfo]) -> Vec<(u32, Role)> {
    let mut found = vec![(root, Role::App)];
    let mut index = 0;
    while index < found.len() {
        let (parent, parent_role) = found[index];
        for child in all
            .iter()
            .filter(|process| process.parent == parent && process.pid != parent)
        {
            if found.iter().any(|(pid, _)| *pid == child.pid) {
                continue;
            }
            let name = child.name.to_ascii_lowercase();
            let role = if parent_role == Role::Codex || name.starts_with("codex") {
                Some(Role::Codex)
            } else if name == "msedgewebview2.exe" {
                Some(Role::WebView)
            } else {
                None
            };
            if let Some(role) = role {
                found.push((child.pid, role));
            }
        }
        index += 1;
    }
    found
}

/// The window's memory reading, in megabytes of private working set.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MemoryReading {
    pub total_mb: f64,
    pub app_mb: f64,
    pub webview_mb: f64,
    pub webview_processes: usize,
    /// `None` while Codex is not running.
    pub codex_mb: Option<f64>,
    /// "private working set", the metric Task Manager's Memory column shows.
    pub metric: &'static str,
}

/// Sums the private working sets of the app's tree; `read` gives one process's bytes.
pub fn reading(tree: &[(u32, Role)], read: impl Fn(u32) -> Option<u64>) -> MemoryReading {
    let mb = |bytes: u64| (bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0;
    let mut result = MemoryReading {
        metric: "private working set",
        ..MemoryReading::default()
    };
    let mut codex = None::<u64>;
    let (mut app, mut webview) = (0u64, 0u64);
    for (pid, role) in tree {
        let Some(bytes) = read(*pid) else {
            continue;
        };
        match role {
            Role::App => app += bytes,
            Role::WebView => {
                webview += bytes;
                result.webview_processes += 1;
            }
            Role::Codex => codex = Some(codex.unwrap_or(0) + bytes),
        }
    }
    result.app_mb = mb(app);
    result.webview_mb = mb(webview);
    result.codex_mb = codex.map(mb);
    result.total_mb = mb(app + webview + codex.unwrap_or(0));
    result
}

/// Takes a reading of this process's tree now.
pub fn read_now() -> MemoryReading {
    let tree = app_tree(std::process::id(), &processes());
    reading(&tree, private_working_set)
}

/// The reading thread: one reading at once, then one per interval, until stopped.
pub struct Reader {
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
    slot: Slot,
}

impl Reader {
    /// Starts the thread writing into `slot`. `Err` when the thread could not be spawned.
    pub fn start(slot: Slot) -> std::io::Result<Reader> {
        let (stop, stopped) = mpsc::channel::<()>();
        let writer = Arc::clone(&slot);
        let thread = std::thread::Builder::new()
            .name("voicebro-memory".to_owned())
            .spawn(move || loop {
                let next = read_now();
                *writer.lock().expect("memory slot lock") = Some(next);
                // A stop, or the sender dropped, ends the loop at once; only a timeout
                // takes the next reading.
                if stopped.recv_timeout(INTERVAL) != Err(RecvTimeoutError::Timeout) {
                    break;
                }
            })?;
        Ok(Reader {
            stop,
            thread: Some(thread),
            slot,
        })
    }

    /// Stops the thread, waits for it (at most one reading, a few milliseconds) and clears
    /// the slot, so the window sees no stale number.
    pub fn stop(mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        *self.slot.lock().expect("memory slot lock") = None;
    }
}

#[cfg(windows)]
fn processes() -> Vec<ProcessInfo> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut list = Vec::new();
    // SAFETY: the snapshot handle is checked before use and closed once; the entry is a
    // plain struct whose size field is set as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return list;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(entry.szExeFile.len());
                list.push(ProcessInfo {
                    pid: entry.th32ProcessID,
                    parent: entry.th32ParentProcessID,
                    name: String::from_utf16_lossy(&entry.szExeFile[..length]),
                });
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    list
}

#[cfg(windows)]
fn private_working_set(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the process handle is checked for null and closed once; the counters struct
    // is passed with its own size, which is how the API selects the EX2 layout.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        let ok = GetProcessMemoryInfo(
            handle,
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        );
        CloseHandle(handle);
        (ok != 0).then_some(counters.PrivateWorkingSetSize as u64)
    }
}

#[cfg(not(windows))]
fn processes() -> Vec<ProcessInfo> {
    Vec::new()
}

#[cfg(not(windows))]
fn private_working_set(_pid: u32) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: u32, name: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent,
            name: name.to_owned(),
        }
    }

    #[test]
    fn the_tree_counts_webview2_and_codex_under_the_app_only() {
        let all = vec![
            process(10, 1, "voicebro.exe"),
            process(11, 10, "msedgewebview2.exe"),
            process(12, 11, "msedgewebview2.exe"),
            process(13, 10, "codex.exe"),
            process(14, 13, "codex-code-mode-host.exe"),
            process(15, 13, "conhost.exe"),
            process(16, 10, "conhost.exe"),
            process(20, 1, "msedgewebview2.exe"),
            process(21, 1, "codex.exe"),
        ];
        let tree = app_tree(10, &all);
        assert_eq!(
            tree,
            vec![
                (10, Role::App),
                (11, Role::WebView),
                (13, Role::Codex),
                (12, Role::WebView),
                (14, Role::Codex),
                (15, Role::Codex),
            ],
            "another app's WebView2 and Codex are not counted"
        );
        let reading = reading(&tree, |pid| Some(u64::from(pid) * 1024 * 1024));
        assert_eq!(reading.app_mb, 10.0);
        assert_eq!(reading.webview_mb, 23.0);
        assert_eq!(reading.webview_processes, 2);
        assert_eq!(reading.codex_mb, Some(42.0));
        assert_eq!(reading.total_mb, 75.0);
    }

    #[test]
    fn without_codex_its_share_is_none_and_the_total_excludes_it() {
        let all = vec![
            process(10, 1, "voicebro.exe"),
            process(11, 10, "msedgewebview2.exe"),
        ];
        let reading = reading(&app_tree(10, &all), |_| Some(5 * 1024 * 1024));
        assert_eq!(reading.codex_mb, None);
        assert_eq!(reading.total_mb, 10.0);
        assert_eq!(reading.metric, "private working set");
    }

    #[test]
    fn this_process_is_read_on_windows() {
        if !cfg!(windows) {
            return;
        }
        let reading = read_now();
        assert!(reading.app_mb > 0.0, "{reading:?}");
    }

    #[test]
    fn a_reader_fills_the_empty_slot_and_stop_clears_it_within_100_ms() {
        let slot: Slot = Arc::new(Mutex::new(None));
        assert!(
            slot.lock().unwrap().is_none(),
            "nothing before a reader runs"
        );
        let reader = Reader::start(Arc::clone(&slot)).expect("thread spawns");
        let started = std::time::Instant::now();
        while slot.lock().unwrap().is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "no first reading"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let stopping = std::time::Instant::now();
        reader.stop();
        assert!(
            stopping.elapsed() < Duration::from_millis(100),
            "stop waited {:?}, not the 3 s interval",
            stopping.elapsed()
        );
        assert!(slot.lock().unwrap().is_none(), "stopped means no reading");
    }
}
