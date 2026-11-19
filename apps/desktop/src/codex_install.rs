//! The install offers on the Status tab (Codex, and since change
//! `claude-and-opencode-engines` Claude Code and OpenCode): whether winget is on this PC,
//! and winget run in a console window the user sees. Nothing is installed without the
//! user's click, and no agreement is accepted on their behalf: winget asks in the console
//! if it needs to. The same console runs Claude Code's own sign-in.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A winget package the Status tab offers, with the sizes its notice names (labels, not
/// checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Package {
    pub id: &'static str,
    pub source: &'static str,
    pub download_mb: u32,
    pub disk_mb: u32,
}

/// Codex 0.167.1.
pub const CODEX: Package = Package {
    id: "OpenAI.Codex",
    source: "OpenAI's GitHub releases (github.com/openai/codex)",
    download_mb: 200,
    disk_mb: 370,
};

/// Claude Code 2.1.318: one portable `claude.exe` from Anthropic (sizes of 2026-11-19).
pub const CLAUDE_CODE: Package = Package {
    id: "Anthropic.ClaudeCode",
    source: "Anthropic (downloads.claude.ai)",
    download_mb: 327,
    disk_mb: 327,
};

/// OpenCode 2.3.2: a portable zip from the OpenCode project (sizes measured on 2026-11-19:
/// a 119.8 MB zip, 247.7 MB in winget's package folder).
pub const OPENCODE: Package = Package {
    id: "Anomaly.OpenCode",
    source: "the OpenCode project (opencode.ai, github.com/anomalyco/opencode)",
    download_mb: 120,
    disk_mb: 250,
};

/// What runs in the console: `command`, then a pause so its output stays readable, then
/// the window closes with the command's exit code.
pub fn console_args(command: &str) -> String {
    console_args_with(command, true)
}

/// As [`console_args`]; without the pause for the tests, which have no one to press a key.
fn console_args_with(command: &str, pause: bool) -> String {
    let pause = if pause { " & echo. & pause" } else { "" };
    format!("/V:ON /C \"{command} & set rc=!errorlevel!{pause} & exit !rc!\"")
}

/// `executable` with `args` as the console runs it. The line starts with the quoted path,
/// so it holds more than two quotes and cmd drops only the outer pair of `/C "…"`.
pub fn program_line(executable: &Path, args: &str) -> String {
    format!("\"{}\" {args}", executable.display())
}

/// winget for `package` in the console.
pub fn winget_args(package: &Package) -> String {
    console_args(&format!(
        "winget install --id {} --exact --source winget",
        package.id
    ))
}

/// `CREATE_NEW_CONSOLE`: winget gets a console window of its own.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallState {
    Idle,
    Running,
    Finished {
        code: i32,
    },
    /// The console could not be started at all.
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallInfo {
    pub winget: bool,
    pub package: &'static str,
    pub source: &'static str,
    pub download_mb: u32,
    pub disk_mb: u32,
    pub state: InstallState,
}

/// `winget.exe`: the App Execution Alias in `%LOCALAPPDATA%\Microsoft\WindowsApps` (a
/// reparse point with no size, so `symlink_metadata`, not `is_file`), else on the PATH.
pub fn winget_path(local_app_data: Option<&Path>, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    let exists = |path: &Path| std::fs::symlink_metadata(path).is_ok();
    local_app_data
        .map(|local| local.join("Microsoft/WindowsApps/winget.exe"))
        .filter(|alias| exists(alias))
        .or_else(|| {
            path_dirs
                .iter()
                .map(|dir| dir.join("winget.exe"))
                .find(|candidate| exists(candidate))
        })
}

pub fn winget_on_this_pc() -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    winget_path(
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .as_deref(),
        &path_dirs,
    )
}

/// One install (or console) at a time per package; the state the window shows.
#[derive(Clone)]
pub struct Installer {
    package: Package,
    state: Arc<Mutex<InstallState>>,
}

impl Default for Installer {
    fn default() -> Self {
        Self::new(CODEX)
    }
}

impl Installer {
    pub fn new(package: Package) -> Self {
        Self {
            package,
            state: Arc::new(Mutex::new(InstallState::Idle)),
        }
    }

    pub fn state(&self) -> InstallState {
        self.state.lock().expect("install state lock").clone()
    }

    pub fn info(&self) -> InstallInfo {
        InstallInfo {
            winget: winget_on_this_pc().is_some(),
            package: self.package.id,
            source: self.package.source,
            download_mb: self.package.download_mb,
            disk_mb: self.package.disk_mb,
            state: self.state(),
        }
    }

    /// Starts winget in its own console window. Returns at once; `done` gets the final
    /// state from a thread that waits for the console.
    pub fn start_winget(
        &self,
        done: impl FnOnce(InstallState) + Send + 'static,
    ) -> Result<(), String> {
        self.start_with("cmd.exe", &winget_args(&self.package), true, done)
    }

    /// Runs `executable` with `args` in its own console window, as winget runs (Claude
    /// Code's sign-in, change `claude-and-opencode-engines`).
    pub fn start_program(
        &self,
        executable: &Path,
        args: &str,
        done: impl FnOnce(InstallState) + Send + 'static,
    ) -> Result<(), String> {
        self.start_with(
            "cmd.exe",
            &console_args(&program_line(executable, args)),
            true,
            done,
        )
    }

    fn start_with(
        &self,
        program: &str,
        raw_args: &str,
        new_console: bool,
        done: impl FnOnce(InstallState) + Send + 'static,
    ) -> Result<(), String> {
        {
            let mut state = self.state.lock().expect("install state lock");
            if *state == InstallState::Running {
                return Err("an install is already running in its console window".to_owned());
            }
            *state = InstallState::Running;
        }
        let mut command = std::process::Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.raw_arg(raw_args);
            if new_console {
                command.creation_flags(CREATE_NEW_CONSOLE);
            }
        }
        #[cfg(not(windows))]
        let _ = (raw_args, new_console);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let failed = InstallState::Failed {
                    message: format!("could not start {program}: {error}"),
                };
                *self.state.lock().expect("install state lock") = failed.clone();
                return Err(format!("could not start {program}: {error}"));
            }
        };
        let state = Arc::clone(&self.state);
        std::thread::Builder::new()
            .name("voicebro-codex-install".to_owned())
            .spawn(move || {
                let finished = match child.wait() {
                    Ok(status) => InstallState::Finished {
                        code: status.code().unwrap_or(-1),
                    },
                    Err(error) => InstallState::Failed {
                        message: format!("waiting for the install: {error}"),
                    },
                };
                *state.lock().expect("install state lock") = finished.clone();
                done(finished);
            })
            .map_err(|error| format!("could not watch the install: {error}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn the_console_runs_winget_with_a_pause_and_its_exit_code() {
        assert_eq!(
            winget_args(&CODEX),
            r#"/V:ON /C "winget install --id OpenAI.Codex --exact --source winget & set rc=!errorlevel! & echo. & pause & exit !rc!""#
        );
        // Change `claude-and-opencode-engines`: the same console for the two new engines.
        assert_eq!(
            winget_args(&CLAUDE_CODE),
            r#"/V:ON /C "winget install --id Anthropic.ClaudeCode --exact --source winget & set rc=!errorlevel! & echo. & pause & exit !rc!""#
        );
        assert!(winget_args(&OPENCODE).contains("--id Anomaly.OpenCode --exact"));
        for package in [CODEX, CLAUDE_CODE, OPENCODE] {
            assert!(
                !winget_args(&package).contains("--accept"),
                "no agreement accepted for the user"
            );
        }
    }

    /// Claude Code's sign-in runs in the console as `start_program` builds the line: the
    /// path quoted (here one with a space), its arguments, and its exit code back.
    #[test]
    fn a_program_runs_in_the_console_with_its_exit_code() {
        let dir = scratch("console program");
        let program = dir.join("fake claude.cmd");
        std::fs::write(&program, "@exit /b %2\r\n").unwrap();
        let line = program_line(&program, "auth 5");
        assert_eq!(line, format!("\"{}\" auth 5", program.display()));
        assert!(console_args(&line).starts_with(&format!("/V:ON /C \"{line} & ")));
        let installer = Installer::new(CLAUDE_CODE);
        let (sender, receiver) = mpsc::channel();
        installer
            .start_with(
                "cmd.exe",
                &console_args_with(&line, false),
                false,
                move |state| sender.send(state).unwrap(),
            )
            .unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
            InstallState::Finished { code: 5 },
            "the quoted path ran with its arguments"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_state_ends_with_the_exit_code_and_a_second_start_is_refused() {
        let installer = Installer::default();
        let (sender, receiver) = mpsc::channel();
        // A stand-in for winget that runs about a second and ends with code 3.
        installer
            .start_with(
                "cmd.exe",
                "/C ping -n 2 127.0.0.1 >nul & exit 3",
                false,
                move |state| sender.send(state).unwrap(),
            )
            .unwrap();
        assert_eq!(installer.state(), InstallState::Running);
        assert!(
            installer
                .start_with("cmd.exe", "/C exit 0", false, |_| {})
                .is_err(),
            "one install at a time"
        );
        let finished = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(finished, InstallState::Finished { code: 3 });
        assert_eq!(installer.state(), InstallState::Finished { code: 3 });
        installer
            .start_with("cmd.exe", "/C exit 0", false, |_| {})
            .expect("a new start after the end");
    }

    /// A fresh folder next to the test executable, inside `target\`.
    fn scratch(name: &str) -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap().join("test-scratch").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn winget_is_found_as_an_alias_or_on_the_path() {
        let dir = scratch("winget");
        let local = dir.join("local");
        let bin = dir.join("bin");
        std::fs::create_dir_all(local.join("Microsoft/WindowsApps")).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let path_dirs = std::slice::from_ref(&bin);
        assert_eq!(winget_path(Some(&local), path_dirs), None);
        std::fs::write(bin.join("winget.exe"), b"").unwrap();
        assert_eq!(
            winget_path(Some(&local), path_dirs),
            Some(bin.join("winget.exe"))
        );
        let alias = local.join("Microsoft/WindowsApps/winget.exe");
        std::fs::write(&alias, b"").unwrap();
        assert_eq!(winget_path(Some(&local), &[bin]), Some(alias));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
