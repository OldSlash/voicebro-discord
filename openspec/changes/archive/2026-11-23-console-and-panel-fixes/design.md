# Design

## Context

See proposal.md, Why.

The desktop binary had `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`:
a release build is a window program, and a debug build is a console program. Windows gives a
console program a console when it starts. From Explorer that is a new console window, which
Windows Terminal hosts on this PC. A console program ends with its console: closing the
window sends it the close signal, and the default handling ends the process. The log plugin
adds its stdout target only in a debug build (`log_plugin` in `apps/desktop/src/main.rs`).

The panel's layout lives in `discord/panel.rs` (`components`). The press handling
(`discord/handler.rs`) and `Runtime::change_mode` take any mode, and the custom ids
`voicebro:mode:<realtime|whisper|text>:<guild>` parse for all three.

## Goals / Non-Goals

**Goals:**

- No console window at any start of the desktop app, and no way for a terminal to end it.
- The debug build's log still reaches the terminal a developer or an agent started it from.
- The panel's one row, with the press handling, the owner rule and the old-panel rule as
  they are.

**Non-Goals:**

- Any change to the console binary `voicebro-server`, which is a console program by design.
- A terminal log for the release build.
- Removing the mode-generic code paths in the handler and the runtime.

## Decisions

### D1. A window program in every build

`#![windows_subsystem = "windows"]` without the debug condition. A window program gets no
console from Windows, so a double-click, a shortcut or the installer's "Run VoiceBro" opens
none. Rejected: keeping the console subsystem and hiding or freeing the console at start.
The window would still flash, and Windows Terminal, as the default terminal, opens a tab
before the program can act.

### D2. The debug build attaches to the terminal it was started from

At the start of `main`, a debug build calls `AttachConsole(ATTACH_PARENT_PROCESS)` (new
module `apps/desktop/src/console.rs`). When the parent has a console (a PowerShell or cmd
prompt, `cargo run`, `cargo tauri dev`), the app attaches. The log's stdout target writes
there, because Rust's stdout looks up the standard handle at each write. When the parent
has none (Explorer), the call fails and nothing changes. The stdout target then writes into
no handle, which Rust treats as success. When the parent redirected the output to a file or
a pipe, the redirected handle stays the standard output and receives the log.

All three cases were checked on 2026-11-23 with a small window program built with the same
calls:

- Run from a PowerShell prompt, its lines appeared in the terminal.
- Started with its output redirected to a file, the file received the lines.
- Run with no console, the attach failed.

Rejected: attaching, keeping the output handle and detaching at once, so that the terminal
could never signal the app. In the same test, the lines written after `FreeConsole` went
nowhere, although the writes reported success.

### D3. The terminal's signals do not reach the app's life

Once attached, the app registers a console signal handler:

- **Ctrl+C and Ctrl+Break** are answered as handled and change nothing.
- **The close signal** (the terminal window or tab is closed) is answered with `FreeConsole`
  and then `ExitThread` on the signal's own thread, which the system starts for the signal.

Windows ends the process once a handler returns from the close signal, whatever it answers.
Ending only the signal thread after detaching keeps the app alive.

Measured on 2026-11-23, with the test program in a classic console window (`conhost.exe`)
and in a Windows Terminal window, each closed with `WM_CLOSE`:

| Handler | Classic console window | Windows Terminal |
| --- | --- | --- |
| none | ended | not tried |
| returns TRUE | ended | ended |
| `FreeConsole` + `ExitThread` | ran on | ran on |

Logoff and shutdown signals keep the default handling (the handler answers FALSE).

Rejected: returning TRUE, which ends the process (measured), and blocking the signal thread,
which Windows ends after its timeout.

The handler runs on a thread the system made, outside any Rust frame of the app. It takes no
lock and allocates nothing before `ExitThread`.

The desktop app's own terminal tab in the Claude app is not a test of this: closing it ends
the processes attached to its console directly, without a signal. That was seen in the same
test, where even the Ctrl+C written to that tab reached the shell and not the program.

### D4. One row on the panel, the rest unchanged

`components` returns one action row: Mute assistant (Unmute assistant while muted, red),
Text answers (primary and disabled while the mode is text answers, grey otherwise) and Leave
channel (red). Discord allows five buttons per row.

The custom ids stay as they are, and `PanelButton::parse` keeps all three mode names. So a
press on a Realtime voice or Whisper mode button of a panel posted before this change is
still recognized as a panel press and gets the old-panel answer: its buttons are removed with
the note that `/join` posts a new panel. Rejected: parsing only `text`. An older panel's
press would then match nothing, Discord would show "This interaction failed", and the old
panel would keep its buttons.

`Runtime::change_mode` and the handler's mode press stay mode-generic. The engine-switch
reply can no longer be reached from a current panel. It stays because removing it would
change tested runtime code for no behavior, and the window's own mode list uses the same
engine rule through `save_settings`.

## Risks / Trade-offs

- [The prompt returns at once when the debug exe is run from PowerShell or cmd, since a
  window program is not waited for; the log lines then appear under the prompt] → Expected
  for a window program. `cargo run` waits for the app as before. AGENTS.md says that Ctrl+C
  no longer stops a `cargo run` of the app: close the window or stop the process.
- [`ExitThread` on a system thread is an unusual Windows technique] → It is confined to the
  close signal of a debug build. The thread holds nothing of the app. The behavior was
  measured on both console hosts of this PC.
- [A panel in the text answers mode offers no way back from Discord] → Recorded as an
  assumption in the proposal. The window switches back, and the report asks the user whether
  Text answers should toggle.
