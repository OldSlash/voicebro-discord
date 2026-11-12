# Design

## Context

See proposal.md. Measured on 2026-11-09 at 22:02 with the wake word loaded: this app
(`voicebro.exe`, release 0.2.0) 689 MB private bytes, 195 MB working set, 169 MB private
working set. `scripts\measure.ps1` reports `PrivateMemorySize64` (private bytes, the commit
charge) and the working set. After change `ui-layout-port` this app has the status bar and
the System tab with an empty memory slot.

## Goals / Non-Goals

**Goals:** one metric everywhere, the one the user reads in Task Manager; correct numbers
in the plan; memory visible in the window.

**Non-Goals:** reducing memory (nothing shows a leak); measuring during a call (the user's
step with the window's readings).

## Decisions

**D1. Private working set as the headline.** It is what Task Manager's "Memory" column
shows; private bytes and the working set stay in the script's output for diagnosis.
`GetProcessMemoryInfo` with `PROCESS_MEMORY_COUNTERS_EX2` gives `PrivateWorkingSetSize`
(present in `windows-sys` 0.61 `Win32::System::ProcessStatus`); the script uses the
`Working Set - Private` performance counter. *Alternative:* private bytes. Rejected: it
counts committed memory the process never touches (689 MB for the release build above) and
it is not what the user sees.

**D2. Which processes.** The app's own process, its descendants whose image is
`msedgewebview2.exe`, and the Codex child (`codex.exe` and its `codex-code-mode-host.exe`),
found by walking the process tree from the app's id with a ToolHelp snapshot. The reading
runs in the shell every 3 s on a blocking thread, never on the UI thread.

**D3. The script measures release builds.** It refuses a `voicebro.exe` whose path contains
`\debug\` unless `-AllowDebug` is given, and prints the build stamp from the first log line
of the data folder it finds next to the executable.

## Risks / Trade-offs

- [WebView2 shares pages between its processes] → the private working set is per process
  and does not double-count shared pages; the sum matches Task Manager's per-process column.
- [Older Windows without `PROCESS_MEMORY_COUNTERS_EX2`] → fall back to the working set and
  label it; the target is Windows 10 and 11 where the structure exists.
