# Design

## Context

See proposal.md for the motivation. What the code does today:

- `apps/desktop/ui/index.html` holds, inside most `.form-grid label` elements, a
  `small.field-message[data-for]` that `fieldMessage()` in `main.js` fills with "Saving…",
  "Saved" (cleared after 2 s) or the validation error. `.field-message:empty` is
  `display: none`, so a message adds a line to its grid cell when it appears and removes it
  when it goes: the row grows and shrinks. The Appearance card has its two messages as
  `p.field-message` after the card's note, outside the grid. The Earcons switch is a
  `label.inline-check` (name and checkbox on one line), so its row is shorter than the
  Earcon volume slider's and the two never line up (seen in the mock on 2026-11-10).
- `main.js` calls `status` every second (`setInterval(refresh, 1000)`), keeps five minutes
  of full status snapshots in `samples` for the Performance tab, calls `memory_status`
  every 3 s and `renderUptime` every second. The status spec asks for a refresh on events
  and at least every 5 s; the 1 s rate exists only for the Performance sampling.
- `apps/desktop/src/memory.rs::start()` spawns the `voicebro-memory` thread in `.manage()`
  before the runtime has loaded the settings; it loops forever (ToolHelp snapshot of every
  process on the machine, then `OpenProcess` and `GetProcessMemoryInfo` per process of the
  app's tree, every 3 s). `memory_status` returns the latest reading.
- The desktop shell already watches `AppEvent::SettingsChanged` in its event bridge task to
  re-register the hotkey; `Runtime::apply` reconfigures one section at a time by name.
- The counters the Performance cards show (Discord ticks and frames, the call's audio
  counters, the gates' drops, the gains' peaks and clips, the delivery counters, the last 50
  turns per guild) are atomics or small vectors written on the audio paths; they cost nothing
  measurable and the Opus encoder reads the delivery counters for its expected loss. They
  are not part of this change.

Constraints: the form state stays apart from the status (change `settings-form-behavior`);
no bundler, no framework; the core never depends on the UI and never on Tauri; settings
apply without a restart.

## Goals / Non-Goals

**Goals:**

- A save that is visible but changes no geometry; rows of a card that line up.
- One help line per control, grounded in what the code actually does with the value and
  when (the table in D3).
- The two measurements off by default, switchable at runtime, with nothing of them running
  while off: no 1 s poll, no samples, no memory thread.

**Non-Goals:**

- Throttling the window's remaining timers while it is minimized or hidden (uptime, the
  5 s status refresh). Worth a measurement first; not in this change.
- Turning off the audio counters, the latency log lines, the delivery counters or the
  Console log forwarding (see Context).
- Any change to `scripts\measure.ps1`, which reads the processes itself.

## Decisions

**D1. The saved mark is a colour on the field's name, the text goes to the tab's line.**
`setState(id, "saved")` adds `is-saved` to the field's `label` for 2 s; CSS colours the
label's own text with `--success` (`.form-grid label.is-saved { color: var(--success) }`,
the control and the help line keep their colours). `#settings-feedback` shows "Saved:
<field name>" for the same 2 s (the name is the label's first text node) and then returns
to "Changes save automatically." The "Saving…" state keeps its border class only
(`is-saving`, muted border), no text. `small.field-message` stays in every label for the
validation error only; the two `p.field-message` of the Appearance card move inside their
labels like everywhere else. *Alternative:* a fixed-height line under every field.
Rejected: 18 px of empty space under each of twenty fields. *Alternative:* a border colour
instead of the name colour. Rejected: sliders and checkboxes have no border, so the mark
would differ by control type.

**D2. The Earcons switch takes the column layout.** `<label>Earcons<input type="checkbox">
<small class="field-help">…</small><small class="field-message">…</small></label>`;
`.inline-check` and its rules go. The checkbox sits under its name, left-aligned, at the
same height as the slider next to it. *Alternative:* keep the inline
layout and stretch the row. Rejected: the name would sit on a different line than the
neighbouring name.

**D3. One help line per control, from the code.** Each control gets a `small.field-help`
under it (the existing ones are kept and completed). What each line says comes from the
settings schema and the `apply` branches, not from guesses:

| Control | Affects | Applies | Default |
|---|---|---|---|
| Theme | the window's colours | at once | Dark |
| Font size | the window's text, 11 to 20 px | at once | 13 |
| What the assistant hears | multiplies what the assistant receives during a session; the wake word and the speech detection hear the channel unchanged | at once, also in a running call | 100 % |
| How loud the assistant speaks | multiplies the assistant's voice played into Discord; earcons keep their own volume | at once | 100 % |
| Earcons | the short tones at session start and end, mute and unmute; they play while muted too | at once | on |
| Earcon volume | the tones' volume, independent of the output gain | at once | 18 % |
| Engine | Codex CLI through its app-server with the ChatGPT sign-in, no API key; the only engine | at once | Codex |
| Activation | wake word: a session starts at an activation phrase, ends at a deactivation phrase or the auto-stop, nothing is sent between sessions; always listening: the assistant hears the channel and the call runs the whole time the bot is in it | at once | wake word |
| Activation phrases | existing text | at once | none |
| Deactivation phrases | existing text | at once | none |
| Auto-stop | existing text | at once, for the running session too | 60 s |
| Wake engine | the offline recognizer that listens for the phrases; Vosk is the only one | at once (the engine reloads) | Vosk |
| Wake model | existing text, plus: a change downloads and loads the new model at once; the phrases stay | at once | the catalog's default |
| Model | existing text | from the next call (a new thread) | account default |
| Reasoning | existing text | from the next call (a new thread) | model default |
| Voice | the voice the realtime call speaks with; empty is the server's default | from the next call | server default |
| Answer handoff | the hint per mode (kept) | from the next call | Conversation items |
| Speaker names | existing text | from the next session | at call start |
| Instructions | sent to the voice at every call start and to the Codex agent as its developer instructions | the voice from the next call; the agent from the next /join | empty |
| Mute hotkey | existing text | at once | Ctrl+Alt+M |
| Performance monitoring (on the Performance tab, mirrored in Settings) | samples the status once a second and keeps five minutes for the Performance tab's charts and cards; costs a status read per second and the kept samples in the window | at once | off |
| Memory measurement | reads the private working set of the app, its WebView2 and Codex processes every 3 s for the status bar and the System tab; costs a scan of the process list every 3 s | at once | off |
| Discord bot token | existing text | the connection restarts | — |
| Discord guild ID | existing text | at the next connection | empty (global only) |
| Codex executable | existing text | at the next start of Codex (the next /join after the bot has left every channel) | automatic |

**D4. `diagnostics` is a settings section, not a build feature or an environment
variable.** `DiagnosticsSettings { performance: bool, memory: bool }` with `#[serde(default)]`
and both `false`; `changed_sections` reports `"diagnostics"`; `apply` logs the change and
touches nothing (no subsystem of the core uses the section). *Alternative:* a Cargo feature
or `VOICEBRO_DIAGNOSTICS=1`. Rejected: the user asked for the measurements to stay in the
build and be switchable; a field report needs them turned on without a rebuild, and the
Settings tab is where every other switch is.

**D5. The memory reader starts and stops with the setting.** `memory.rs` gets a `Reader`
with `start() -> Reader` (the slot plus a stop channel: the loop waits on
`mpsc::Receiver::recv_timeout(INTERVAL)`, so a stop returns at once instead of after up to
3 s) and `stop(self)` (sends, joins, clears the slot). `AppState.memory` becomes
`Mutex<Option<Reader>>` next to the shared slot; `setup` starts it only when the loaded
settings say so; the event bridge task compares `diagnostics.memory` on every
`SettingsChanged`, as it does for the hotkey, and starts or stops. `memory_status` returns
`None` while off. The log says "memory reading started" / "stopped". *Alternative:* keep the
thread and skip the reading while off. Rejected: a sleeping thread is cheap but the user
asked for nothing of it to run, and the stop is one channel.

**D6. The window polls by setting.** `applyDiagnostics()` in `main.js` runs at load and on
`settings_changed`: it re-arms the status timer at 1 s while `diagnostics.performance` is on
and 5 s while off (events still refresh at once), clears `samples` when monitoring turns
off, shows either the tab's content or its off note (a `.hidden` class on the two), and
starts or clears the 3 s `memory_status` timer, hiding the status bar's memory item and
writing the off text into the "Native host" card while `diagnostics.memory` is off.
`samplePerformance` returns at once while off. *Alternative:* keep the 1 s poll and only
skip the rendering. Rejected: the poll is the cost (a status snapshot serialized per
second, a WebView2 wake-up per second).

**D6a. The switch is on the Performance tab, as one more form control.** The tab's
toolbar gets `label.checkbox.compact` with
`<input type="checkbox" data-path="diagnostics.performance" data-kind="bool">`, next to
the window selector. It is an ordinary `[data-path]` control: the existing form code saves
it on `change` with one `save_settings` patch (so it lands in `settings.json` and comes back
at the next start through `get_settings`), fills it from the stored settings at load and on
`settings_changed`, and never overwrites it while focused. The Diagnostics card on the
Settings tab keeps a second control with the same `data-path`, so the user finds it with
the other settings; `fillForm` iterates every `[data-path]`, so both show the stored value,
and a save from either reaches the other through `settings_changed`. The saved mark (D1) is
applied to the control's own `label`, which works for both. The tab's off note sits under
the toolbar and states the cost, not a location. *Alternative:* a switch on the tab only,
no card entry. Rejected: the settings spec lists every setting on the Settings or Config
tab, and the memory switch has no tab of its own to go to.

**D7. The mock follows.** `ui-dev/mock.js` adds the section to its default document,
returns `null` from `memory_status` while `diagnostics.memory` is off, and counts `status`
calls in `window.__mock.calls`, so the poll rate is checkable from the page.

## Risks / Trade-offs

- [The user looks for the memory number and finds none] → the status bar item is simply
  absent, and the System tab's card says where the switch is; `AGENTS.md` and
  `LOCAL-TESTING.md` say the measuring commands need the switch on.
- [A validation error still grows its row] → accepted: the message must be readable, and it
  grows only that row.
- [The Status tab is slower between events while monitoring is off] → every change the tab
  shows is also an event; 5 s is what the status spec promises.
- [A reading thread stopped mid-`OpenProcess`] → the stop is checked between readings only;
  one reading takes milliseconds and the join waits for it.
- [Help text drifts from the code] → each line's "applies" column comes from a named `apply`
  branch or call-start path (D3); a later change to those updates the line in the same
  change, as `ARCHITECTURE.md` already requires for the docs.

## Migration Plan

- Branch `change/settings-help-and-optional-diagnostics` from `main`.
- A settings file without `diagnostics` loads with both switches off and gains the section
  at the next save; an older build reading the newer file keeps the unknown section through
  its `extra` map. No data to migrate, nothing to roll back beyond the branch.
- A build handed over gets a tag on `main` as every build does; the user checks the Settings
  tab in the real window and, with both switches on, that the numbers are what they were.
