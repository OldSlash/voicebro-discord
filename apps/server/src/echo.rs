//! Echo off while `--set-token` reads the token from a terminal (change
//! `cross-platform-headless`, design D5); the terminal's mode comes back on drop.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE, ENABLE_ECHO_INPUT,
        STD_INPUT_HANDLE,
    };

    pub struct EchoOff {
        handle: HANDLE,
        mode: CONSOLE_MODE,
    }

    impl EchoOff {
        /// `None` when standard input is not a console whose mode can change.
        pub fn new() -> Option<Self> {
            // SAFETY: plain console calls on this process's standard input handle; the
            // mode is read before it is changed and restored on drop.
            unsafe {
                let handle = GetStdHandle(STD_INPUT_HANDLE);
                let mut mode: CONSOLE_MODE = 0;
                if GetConsoleMode(handle, &mut mode) == 0 {
                    return None;
                }
                if SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT) == 0 {
                    return None;
                }
                Some(Self { handle, mode })
            }
        }
    }

    impl Drop for EchoOff {
        fn drop(&mut self) {
            // SAFETY: restores the mode read in `new` on the same handle.
            unsafe {
                SetConsoleMode(self.handle, self.mode);
            }
        }
    }
}

#[cfg(unix)]
mod imp {
    pub struct EchoOff {
        saved: libc::termios,
    }

    impl EchoOff {
        /// `None` when standard input is not a terminal whose attributes can change.
        pub fn new() -> Option<Self> {
            // SAFETY: termios calls on standard input; the attributes are read before they
            // are changed and restored on drop.
            unsafe {
                let mut saved: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(libc::STDIN_FILENO, &mut saved) != 0 {
                    return None;
                }
                let mut quiet = saved;
                quiet.c_lflag &= !libc::ECHO;
                if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &quiet) != 0 {
                    return None;
                }
                Some(Self { saved })
            }
        }
    }

    impl Drop for EchoOff {
        fn drop(&mut self) {
            // SAFETY: restores the attributes read in `new`.
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
            }
        }
    }
}

pub use imp::EchoOff;
