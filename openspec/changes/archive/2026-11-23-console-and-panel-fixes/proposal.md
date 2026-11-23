# Proposal

## Why

On 2026-11-23 the user ran the live checks of the changes applied overnight and found two
things to fix:

- **The debug build opens a console of its own.** The desktop app's debug build is a console
  program. Started from Explorer, it opens a console window that writes the log, and closing
  that window ends the app. The user's verdict: the console is only meant to show the log.
  It must not appear at a normal start, and it must not hold the app's life.
- **The panel's Realtime voice and Whisper mode buttons are not needed** (the user: they are
  useless). Text answers stays, moved up into the row of Mute assistant and Leave channel.

The user accepted the rest of the panel, the owner link by paste and the console binary on
Windows the same day.

## What Changes

- **The desktop app is a window program in every build.** Starting it never opens a
  console, in a debug build as in a release build, which already worked this way.
- **A debug build started from a terminal writes its log to that terminal**: `cargo run`, or
  the exe run from a PowerShell or cmd prompt. The terminal only shows the log. Ctrl+C and
  Ctrl+Break there do not stop the app, and closing the terminal leaves the app running.
  Output redirected to a file or a pipe goes there, as before. A release build writes no log
  to a terminal, as before.
- **The panel has one row of buttons**: Mute assistant (Unmute assistant while muted), Text
  answers and Leave channel. Realtime voice and Whisper mode are gone. Text answers works as
  before: the owner's press switches to the text answers mode through the window's save,
  and while that mode is current the button is marked and cannot be pressed.
- **Without the Realtime voice button, the panel no longer switches the engine.** The window's
  Realtime voice choice still switches the engine to Codex (change
  `realtime-switches-to-codex`).
- **Panels posted before this change keep their old buttons until pressed.** A press on any
  of them, Realtime voice and Whisper mode included, is answered by the old-panel rule: the
  panel loses its buttons and says that `/join` posts a new one.

Assumptions recorded here:

- The row's order is Mute assistant, Text answers, Leave channel, so the red Leave channel
  stays at the end.
- Text answers stays a one-way switch. Back from the text answers mode is in the window. A
  toggle back to the previous mode was not asked for, and the report asks the user about it.

Not in this change: a terminal for the release build, a log window of any kind, a toggle on
Text answers, and any other change to the panel or to the console binary.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `diagnostic-logging`: a new requirement, "The app opens no console of its own": a window
  program in every build, and a debug build's log in the terminal it was started from, with
  that terminal unable to stop the app.
- `discord-panel-controls` (added by the unarchived change `discord-panel-controls`; this
  change is archived after it): "The panel's buttons" becomes one row with Text answers;
  "A mode button switches the assistant mode" keeps only Text answers; "A mode whose files
  are missing is refused" becomes "Text answers is refused while Whisper's files are
  missing"; "Realtime voice switches the engine to Codex" is removed.
- `voice-channel-session`: requirement "Member panel" (as `discord-panel-controls` modifies
  it): Mute assistant, Text answers and Leave channel instead of the three modes.

## Impact

- Window app: `apps/desktop/src/main.rs` (`windows_subsystem = "windows"` in every build;
  the terminal attach at start in a debug build), a new `apps/desktop/src/console.rs` (the
  attach and the console signal handler), `apps/desktop/Cargo.toml` (the `windows-sys`
  feature `Win32_System_Console`).
- Core: `discord/panel.rs` (one row; the custom ids and their parsing are unchanged, so old
  panels still parse). `discord/handler.rs` and `Runtime::change_mode` stay mode-generic and
  change nothing.
- Probe: `apps/probe/src/panel.rs` (`probe discord-panel` checks the one row).
- Docs: `docs/user-guide.md` (the panel), `docs/ARCHITECTURE.md` (the member panel; the debug
  build's terminal log), `AGENTS.md` (local: the panel, the probe, `cargo run`'s log).
- The console binary `voicebro-server` is unchanged; it is a console program by design. No
  new dependency, no settings change, no version change.
