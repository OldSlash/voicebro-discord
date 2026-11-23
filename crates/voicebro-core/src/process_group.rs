//! The engines' child processes on Linux run in their own process group (change
//! `cross-platform-headless`, design D6; not tested): a Ctrl+C in the terminal reaches only
//! VoiceBro, and stopping an engine stops the whole group, so an npm launcher script never
//! leaves its native program running. On Windows nothing changes: the children have no
//! console (`CREATE_NO_WINDOW`) and the callers' own kill stays.

/// How long a group gets after SIGTERM before SIGKILL.
#[cfg(unix)]
const GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Starts the child in a process group of its own on Linux.
pub(crate) fn own_group(command: &mut tokio::process::Command) {
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(not(unix))]
    let _ = command;
}

/// Stops the group led by the child `pid` on Linux: SIGTERM now, SIGKILL after [`GRACE`]
/// when anything of the group is still there. Returns at once; the caller still kills the
/// child itself.
pub(crate) fn stop_group(pid: Option<u32>) {
    #[cfg(unix)]
    {
        let Some(group) = pid.and_then(|pid| libc::pid_t::try_from(pid).ok()) else {
            return;
        };
        // SAFETY: killpg only sends a signal to the group this process started with the
        // child as its leader; the child is not reaped yet, so its id is still that group's.
        unsafe { libc::killpg(group, libc::SIGTERM) };
        let _ = std::thread::Builder::new()
            .name("voicebro-stop-group".to_owned())
            .spawn(move || {
                let deadline = std::time::Instant::now() + GRACE;
                while std::time::Instant::now() < deadline {
                    // SAFETY: signal 0 only asks whether the group still has a member.
                    if unsafe { libc::killpg(group, 0) } != 0 {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                // SAFETY: as above; whatever of the group ignored SIGTERM ends now.
                unsafe { libc::killpg(group, libc::SIGKILL) };
            });
    }
    #[cfg(not(unix))]
    let _ = pid;
}
