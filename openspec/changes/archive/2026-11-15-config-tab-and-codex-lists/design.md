# Design

## Context

See proposal.md for the motivation. The state this design starts from:

- `CodexAssistant` (`crates/voicebro-core/src/assistant/codex/mod.rs`) keeps the account,
  the voices (`Option<Voices>`) and the models (`Vec<ModelInfo>`) in memory only. They are
  filled by `check()` (Status → Check Codex) and by `prepare_inner()` on `/join`, which reads
  the models only when the list is empty; `refresh_voices()` reads the voices only when none
  are held, so a Check never re-reads them. `capabilities()` hands both to the window.
- The window (`apps/desktop/ui/main.js`) builds the Model, Reasoning and Voice selects from
  `capabilities` (`buildCodexLists`, `fillListed`); with empty lists it shows the text inputs
  with a datalist. It reloads `capabilities` on every `assistant` event.
- `on_demand` use (Sign in, Check) schedules an app-server stop 60 s later
  (`after_on_demand_use`, `IDLE_STOP_DELAY`) unless a guild is attached or a join is being
  prepared. The spec and `AGENTS.md` say nothing of Codex runs before `/join`, Sign in or
  Check.
- Settings: `save_settings` patches the document, validates, writes atomically and applies
  only the changed sections; a `codex` change calls `assistant.configure`, a `ui` change
  applies nothing in Rust. The Settings tab saves each field on change and writes one
  feedback line (`#settings-feedback`); the Config tab waits for Save changes and shows the
  stored document as JSON (`#config-preview`).
- Every on/off setting is an `<input type="checkbox">` 15 × 15 px (`styles.css`), with an
  `<output>` "on"/"off" floated right on the name line.
- `ui-dev/mock.js` stands in for every command; `check-wake-dialog.mjs` drives the install
  dialog in a headless Edge and reads `#settings-feedback`.

## Goals / Non-Goals

**Goals:**

- The Model, Reasoning and Voice lists are there at every start after the first read, with
  no Codex process when the user does not want one.
- One place for the bot's settings (Config), cards that fold away, switches that are easy to
  hit, without changing how any field saves or validates.

**Non-Goals:**

- No change to how Codex threads, calls or the model and voice settings themselves work.
- The cards of the Status, Performance and System tabs do not fold; only the settings
  cards of the Settings and Config tabs do.
- The toolbar check boxes that are view filters, not settings (Console and Codex chat
  Auto-scroll, the chat filters) keep their look.
- No new Tauri command for the refresh: Refresh lists reuses `codex_check`.

## Decisions

The user asked for this change to be planned and built without questions and reviewed at
the end (2026-11-15). Every choice below that the request left open is marked **[decided
alone]** so the review can find them.

### D1. Saved lists in their own file, `codex-lists.json` [decided alone]

The lists are written to `<data dir>\codex-lists.json`:

```json
{ "version": 1, "read_at_ms": 1763141400000, "codex_version": "0.167.0",
  "models": [ { "id": "...", "display_name": "...", "description": "...",
                "reasoning_efforts": ["low", "medium"], "default_reasoning_effort": "medium",
                "is_default": true } ],
  "voices": ["juniper", "cove"], "default_voice": "juniper" }
```

A new module `assistant/codex/lists.rs` owns `SavedLists { read_at_ms, codex_version,
models, voices, default_voice }` with `load(path) -> Option<SavedLists>` and
`save(path, &SavedLists) -> io::Result<()>` (temp file plus rename, like
`SettingsStore::save`). `paths::codex_lists_path(data_dir)` names the file. `ModelInfo`
gains `Deserialize`.

Alternative: a section in `settings.json`. Rejected: the settings file is the user's
choices, validated and diffed on every save; a server-provided cache there would trigger
`settings_changed` and `codex` applies on every read, and would sit in the file the user
edits. A separate file can be deleted to forget the lists, and a broken one never blocks the
settings.

### D2. What counts as "read in this run"

`Inner` gets `lists_read_at_ms: Mutex<Option<u64>>` and `lists_fresh: AtomicBool`.
`new()` loads the file into `models`, `voices` and `lists_read_at_ms`, with
`lists_fresh = false`. A successful `model/list` sets `lists_fresh = true`, the read time to
now, and saves the file with the voices held at that moment. `refresh_voices` takes a
`force` flag: Check (and so Refresh lists and the start read) forces; a join reads the voices
when none are held or `lists_fresh` is false. `prepare_inner` reads the models when
`!lists_fresh` (today: when the list is empty). So the first join after a start without a
read refreshes the lists once, later joins reuse them, and Check always re-reads.

The file is written only after a successful `model/list` (D1 needs the models; a voices-only
read would save half a picture). A failed read leaves the memory and the file as they were.

### D3. The read at start lives in the core, triggered by the desktop shell

`CodexAssistant::read_lists_at_start(&self, enabled: bool) -> bool` decides and spawns:
when `find_codex(executable)` finds nothing it returns `false` and does nothing (no error,
so a PC without Codex sees only the usual "codex not found" status); when `enabled` or no
lists were loaded it spawns `self.check()` on the runtime handle and returns `true`.
`check()` already starts the server, reads the account, voices and models, logs the account,
sets or clears the last error, publishes the status and schedules the stop a minute later.
The desktop shell calls it in `setup` right after `runtime.start()` with
`settings.codex.refresh_lists_at_start`, and logs "Codex: reading the lists at start
(<reason>)" with the reason "the setting is on" or "no saved lists".

Alternatives: in `Runtime::start` through the `Assistant` trait (adds a trait method only
Codex needs), or in the window at load (would run once per window reload, not per start).

A `/join` during the read uses the same app-server: `server()` holds the slot lock, and
`prepare_guild` bumps the stop epoch, so the read's scheduled stop does nothing while a guild
is attached or a join is being prepared (existing behaviour).

### D4. Read once at start when nothing is saved, whatever the switch [decided alone]

The request says the lists must be there "by default". With the switch off and no file
(first start, or the file deleted), the app reads them once at start when Codex is found.
Until a read succeeds (for example while Codex is not signed in and `model/list` fails) each
start tries again; that costs one app-server start of about a minute per app start, the same
as a Check.

### D5. `codex.refresh_lists_at_start` defaults to off [decided alone]

The request gives the switch without a default. Off keeps the old rule (no Codex at start)
for everyone who has saved lists; the lists still renew at the first join of each run (D2),
at Check and at Refresh lists. Turning it on is one click in the Codex card. Changing the
default later is a one-line change in `CodexSettings::default`.

### D6. Codex card: switch, read time and Refresh lists [decided alone: the button]

The card's first row: left the switch "Refresh the lists at start"; right a "Lists" field
that reads "Read 14.11.2026 18:30" (the window's locale, from `read_at_ms`), "Never read" or
"Reading…", with a **Refresh lists** button. The button calls `codex_check`, then
`capabilities`; while it runs the button is disabled and the field reads "Reading…"; an error
goes to the field in the error colour, and the lists stay. `Capabilities` gains
`lists_read_at_ms: Option<u64>`; the window rebuilds the lists only when models or voices
changed (`capabilitiesKey`) but rewrites the read time every time.

The button is an addition to the request: without it the only way to renew the lists before
a join is Status → Check Codex, which is the detour the request complains about. It can be
removed at review without touching anything else.

### D7. Config tab layout

Order: "Discord and Codex", Audio, Assistant, Codex, Hotkey, then the tab's feedback line
("Changes save automatically." / "Saved: <field>") and the line "Settings file: <path>".
The cards keep their markup and ids; they move from the Settings section into the Config
section, all in one column of the Config card's width (920 px). `#config-preview` and the
code that fills it go; `#config-path` stays, outside every card, so it shows with every card
collapsed. The "Discord and Codex" card keeps its own Save changes, Reset and feedback.

Feedback lines: `markSaved` writes the `.form-feedback[data-autosave]` line of the tab panel
that holds the control; a control in a panel without one (the Performance tab's switch) gets
only the colour mark. `#settings-feedback` stays on the Settings tab; the Config tab gets
`#config-autosave-feedback`.

### D8. Collapsing: a header button per card, state in `ui.collapsed_cards` [decided alone: where the state lives]

Each collapsible card gets `data-card="appearance|diagnostics|discord|audio|assistant|codex|hotkey"`
(the first two on the Settings tab, added at the user's request after the first trial
build, 2026-11-15: folding on one tab only was inconsistent); its `<h2>` is
wrapped in a `<button class="card-toggle" aria-expanded aria-controls>` with a chevron, and
its body in one container that `hidden` hides. Enter and Space work because it is a
button. The "Discord and Codex" card's badge stays in the title row.

The state is `ui.collapsed_cards: Vec<String>` in `settings.json` (default empty, serde
default, no validation beyond the type), saved with `save_settings` like the theme, so it
travels with the portable folder like every other window setting. A toggle saves the stored
list with the card's id added or removed, keeping ids the window does not know. It is not a
form field: no saved mark, no feedback line, and `fillForm` applies it from the stored
settings on load and on every settings change.

Alternative: `localStorage` in `webview-data\`. Rejected: the rest of the window's state
(theme, font size) is in `settings.json`, and the mock and the checks read the stored
document, not the browser's storage.

### D9. Switches: a styled native checkbox with `role="switch"` [decided alone: switch, not a button]

Every `data-kind="bool"` input becomes `<input type="checkbox" role="switch" class="switch">`
inside a `<span class="switch-row">` with the On/Off text as its own `<label for>`: a
46 × 26 px track with a 20 px knob (`appearance: none`, theme colours, the accent when on),
a 2 px focus outline, `cursor: pointer` on the whole row. The `<output>` "on"/"off" on the
name line goes; the On/Off text sits right of the switch and is written by `percentLabel`
as today. The field's name line stays as it is, so a switch and a slider in one row keep
their names and controls on one line (`A save moves no other field`). The Performance tab's
toolbar switch uses the same markup.

Alternative: a toggle button (`aria-pressed`). Rejected: the native checkbox keeps the
existing `change` handling, `controlValue`, `writeControl` and Space key behaviour unchanged.

### D10. Texts that name places

"Settings → Assistant", "Settings → Codex", "Settings → Audio" and "Settings → Hotkey"
become "Config → …" in `main.js` (the install dialog note and the waiting status line),
`README.md` and `docs/user-guide.md` (its tab table splits into Settings and Config rows).
"Settings → Diagnostics" and "Settings → Appearance" stay. The archived changes are not
touched.

### D11. Tests

- Rust (`cargo test`): settings defaults and round trip for both fields, a non-boolean and a
  non-list rejected; `lists.rs` save and load, a broken file is `None`, a missing file is
  `None`; with the Node fake (`testdata/fake-codex`, skipped without Node as today): a
  loaded file fills `capabilities` with no process started; `read_lists_at_start(false)` with
  saved lists starts nothing; with no saved lists it reads and writes the file;
  `read_lists_at_start(true)` reads; a missing executable returns `false`; a join after
  loading re-reads once, a second join does not; Check re-reads the voices.
- Window, headless against the mock: a new `ui-dev/check-config-tab.mjs` (cards on the right
  tabs, collapse by click and by key, state saved and restored after a reload, every switch
  ≥ 44 × 24 px and toggled by its label, the Config tab's own feedback line, no settings
  JSON, the path shown with every card collapsed, lists from saved capabilities at load,
  Refresh lists success and failure, free text when never read). `check-wake-dialog.mjs`
  runs unchanged in its scenarios, with its feedback read moved to the Config tab.
- The mock gains `capabilities.lists_read_at_ms`, `?nolists` (no lists, never read),
  `__mock.failNextCheck()`, and `codex_check` moving the read time to now.
- The real app, through the WebView2 debugging port, with a scratch `VOICEBRO_DATA_DIR`:
  first start reads and writes `codex-lists.json`; second start with the switch off shows
  the lists and has no `codex.exe` child; with the switch on a child appears and is gone
  about a minute later. Discord is not started; the user runs the Discord checks.

### D12. Decided while building (2026-11-15) [decided alone]

- **An empty model list replaces nothing.** D2 counts a successful `model/list` as a read.
  An answer with no model (an account that lists none) is now treated like a failed read:
  the lists held and the file stay, nothing is marked as read in this run, and the next
  join tries again. Otherwise one empty answer would wipe good saved lists.
- **What the load found is logged later.** `CodexAssistant::new` runs in `main()` before
  the desktop shell's log exists, so a log line written there was lost (seen in the first
  real start). The load keeps its note ("N models and M voices saved from the read of
  <time>", or the parse warning) and `read_lists_at_start` logs it. `lists::load` returns
  `Result<Option<_>, String>` for that.
- **Earcons lines up with its slider.** A switch has no value line, so next to the Earcon
  volume slider it sat 20 px higher. `label.beside-slider` moves its switch down by one
  value line (`calc(1.4em + 4px)`), which keeps the existing requirement "Switch next to a
  slider".
- **Feedback lines.** The Config tab's resting text names what saves on its own: "Audio,
  Assistant, Codex and Hotkey save each change automatically." "Open models folder" and
  "Open assets folder" write their result to the Config tab's line for six seconds (before:
  until the next save, on the Settings tab).
- **The user guide's screenshot** `docs/images/wake-settings.png` showed the Settings tab;
  it was taken again from the mock in the same frame with the Config tab.
- **Fake Codex.** `testdata/fake-codex/fake_codex.js` now lists two models and two voices
  (it listed none) and takes `--log <file>` and `--models a,b|none`, so the tests can count
  the reads.

Seen while testing and left as it was: a `settings.json` saved with a UTF-8 byte order
mark does not load ("expected value at line 1 column 1"); this change does not touch the
settings loader.

## Risks / Trade-offs

- [The saved lists go stale: a model the account lost is still offered] → the first join of
  every run, Check and Refresh lists re-read them; a model Codex rejects already falls back
  to the defaults with a log line (existing requirement "Model and reasoning settings").
- [A PC with Codex installed but signed out starts the app-server at every start until a
  read succeeds (D4)] → it stops a minute later as after Check; the status then says "not
  signed in", which is the next step the user needs anyway.
- [Two ways of saving on one tab confuse] → the "Discord and Codex" card keeps its "Unsaved
  changes" feedback and "Changed, not saved yet" marks; the auto-saved cards have their own
  line at the bottom of the tab.
- [Every collapse writes `settings.json` and logs "settings: ui changed"] → one small atomic
  write per click; acceptable for a user action.
- [Scripts and docs that find fields on the Settings tab break] → ids do not change, only
  their tab; `check-wake-dialog.mjs` and the docs are updated in this change.

## Migration Plan

Nothing to migrate: a settings file from 1.0.1 loads with `refresh_lists_at_start = false`
and `collapsed_cards = []`. The first start of the new build has no `codex-lists.json` and
reads the lists once (D4). Going back to 1.0.1 keeps both fields as unknown fields and
ignores `codex-lists.json`.
