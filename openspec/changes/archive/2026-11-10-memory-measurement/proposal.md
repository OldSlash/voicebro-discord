# Proposal

## Why

The plan reports 434 MB for the app's process in wake-word mode and concludes it is "well
above" the expected 150 to 200 MB; the user read that as a defect. Measured the way Task
Manager counts (private working set), this app's release build uses 169 MB. The 434 MB was
private bytes of a debug build. The measurement and the conclusion are wrong, and the window
shows no memory at all: the memory slots in its status bar and System tab are empty.

## What Changes

- `scripts\measure.ps1` reports the private working set (Task Manager's "Memory") next to
  private bytes and the working set, for the app, its `codex.exe` and its WebView2
  processes, and refuses to measure a debug build unless asked.
- `docs/MVP-PLAN.md` gets the corrected measurement table and conclusion, with release
  numbers for idle always mode and idle wake-word mode, three runs each.
- The window shows measured memory: the status bar's "Memory" slot shows the total private
  working set of the app's processes; the System tab's "Native host" card shows the split
  between the Rust process, WebView2 and Codex, refreshed every few seconds.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-window`: the status bar and the System tab show measured memory.

## Impact

- `apps/desktop/src/main.rs` (a memory reading through `windows-sys`:
  `GetProcessMemoryInfo` with `PROCESS_MEMORY_COUNTERS_EX2` for the private working set,
  the process tree for WebView2 and Codex), `apps/desktop/ui` (status bar and System tab),
  `scripts\measure.ps1`, `docs/MVP-PLAN.md`.
- `windows-sys` features `Win32_System_ProcessStatus`, `Win32_System_Diagnostics_ToolHelp`,
  `Win32_System_Threading` if not yet enabled.
