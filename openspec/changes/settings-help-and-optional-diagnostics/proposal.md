# Proposal

## Why

Three things the user saw in the window after the first merged chain. The per-field
"Saving…" and "Saved" lines added by `settings-form-behavior` appear inside the two-column
form grid, so a save pushes the neighbouring cell down and, in the Appearance card, the line
shows up under the card's note instead of at the field; the rows of a card stop lining up
(the Earcons row most visibly). Several settings carry no description of what they do or
when they apply. And the diagnostic measurements built for the tuning work (the status
sampled once a second with five minutes kept in the window, the Performance tab's charts and
forty stat cards, the process-memory reading every 3 s) run for every user all the time,
spending CPU and WebView2 memory on a PC that is running a game.

## What Changes

- **Save feedback that moves nothing.** The per-field "Saving…" and "Saved" lines are
  removed. A field that was just saved shows it on its own border (the success colour for
  about two seconds), and the Settings tab's single feedback line at the bottom names the
  last saved field ("Saved: Font size") before returning to "Changes save automatically".
  Validation errors stay at the field, as the spec requires, in the row of that field only.
  The Audio card's Earcons switch takes the column layout of every other field, so the
  Audio rows line up again.
- **Every setting is described.** Each control on the Settings and Config tabs gets its own
  help line under it: what the setting affects, when a change applies (at once, from the
  next call, at the next connection) and the default. Today ten fields have none of their
  own: Theme, Font size, What the assistant hears, How loud the assistant speaks, Earcons,
  Earcon volume, Engine, Activation, Wake engine and Voice. Help that exists but does not
  say when a change applies (Instructions, Speaker names, Codex executable) is completed.
- **The measurements become optional, off by default.** A new settings section
  `diagnostics` with two switches, shown in a Diagnostics card on the Settings tab:
  - `performance` (default off): its switch sits in the Performance tab's own toolbar
    (the Diagnostics card mirrors it; both are the one setting, saved in `settings.json`
    and so kept across restarts). While off, the window polls the status every 5 s and on
    events only, as the status spec says, keeps no samples, draws no charts, and the
    Performance tab shows the switch and one note saying the monitoring is off. While
    on, the tab works as it does today (one sample per second, five minutes kept).
  - `memory` (default off): while off, the memory-reading thread does not run, the status
    bar shows no memory item, and the System tab's "Native host" card says the
    measurement is off. While on, as today.
  Both apply at once when saved, without a restart. The audio counters, the per-turn
  latency lines, the delivery counters and the call's end-of-call log line stay as they
  are: they are atomics and one log line per turn, cost nothing measurable, the expected
  loss needs the delivery counters for the Opus encoder, and they are what makes a user's
  log diagnosable.
- The command mock (`apps/desktop/ui-dev`) learns the new section so the UI can still be
  checked without the app.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings-ui`: how a save is shown (no per-field text line; border and the tab's feedback
  line), every field has a help line, the Settings tab has a Diagnostics card.
- `app-settings`: the `diagnostics` section (`performance`, `memory`), defaults, migration
  of older files, applied at once.
- `performance-view`: the monitoring switch on the tab, kept across restarts; sampling and
  charts only while it is on; the tab's off state.
- `app-window`: memory is measured and shown only while the memory switch is on; the status
  bar and the System tab's off state.

## Impact

- `apps/desktop/ui/index.html` (help lines, Diagnostics card, Earcons layout, the removed
  per-field lines), `main.js` (save feedback, polling interval by setting, Performance and
  memory off states), `styles.css` (the saved-border state, the removed `.field-message`
  "saved"/"pending" styles).
- `crates/voicebro-core/src/settings/mod.rs` (`DiagnosticsSettings`, `changed_sections`),
  `runtime.rs` (the `diagnostics` branch of `apply`: a log line; nothing in the core uses
  the section).
- `apps/desktop/src/memory.rs` (a thread that can be started and stopped), `main.rs`
  (start or stop on `SettingsChanged`, `memory_status` returns `None` while off).
- `apps/desktop/ui-dev/mock.js` (the new section and its validation).
- Docs: `docs/ARCHITECTURE.md` 4.8 (Settings, Performance, System, form feedback) and 7
  (memory reading is optional), `AGENTS.md` (the switches the measuring commands need) and
  `LOCAL-TESTING.md` (the Measurements row).
- No new dependencies. A settings file without the section loads with both switches off.
