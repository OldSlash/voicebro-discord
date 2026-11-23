//! A debug build's log on the terminal it was started from.
//!
//! The app is a window program in every build, so starting it never opens a console of its
//! own. A debug build started from a terminal (`cargo run`, or the exe typed at a prompt)
//! attaches to that terminal's console so its log lines appear there. The console only
//! receives output: Ctrl+C and Ctrl+Break in it are ignored, and when the terminal is
//! closed the app detaches and keeps running.

/// Attaches to the console of the process that started the app, if it has one, and takes
/// over that console's signals. Returns whether it attached; started from Explorer or a
/// shortcut there is no console and nothing changes.
#[cfg(windows)]
pub fn attach_to_parent_terminal() -> bool {
    use windows_sys::Win32::System::Console::{
        AttachConsole, SetConsoleCtrlHandler, ATTACH_PARENT_PROCESS,
    };
    // SAFETY: plain Win32 calls without pointers; a failure only means there is no terminal.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
    if attached {
        // SAFETY: `on_console_event` has the handler signature and lives as long as the app.
        unsafe { SetConsoleCtrlHandler(Some(on_console_event), 1) };
    }
    attached
}

#[cfg(not(windows))]
pub fn attach_to_parent_terminal() -> bool {
    false
}

/// Runs on a thread the system starts for each console signal. Returning from a close
/// signal ends the process whatever the handler answers, so the close detaches from the
/// console and ends only this thread. Checked on 2026-11-23 with a classic console window
/// and with Windows Terminal: a handler that returns is ended with the window, one that
/// detaches and ends its thread keeps running.
#[cfg(windows)]
unsafe extern "system" fn on_console_event(kind: u32) -> windows_sys::core::BOOL {
    use windows_sys::Win32::System::Console::{
        FreeConsole, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
    };
    match kind {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => 1,
        CTRL_CLOSE_EVENT => {
            // SAFETY: the thread belongs to the system's signal routine and holds nothing of
            // the app; after `FreeConsole` the log's terminal output is dropped silently.
            unsafe {
                FreeConsole();
                windows_sys::Win32::System::Threading::ExitThread(0)
            }
        }
        _ => 0,
    }
}
