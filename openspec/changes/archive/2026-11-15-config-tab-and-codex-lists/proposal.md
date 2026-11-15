# Proposal

## Why

The Codex model, reasoning and voice lists exist only in memory: they are empty after every
start until the user presses Check Codex on the Status tab or the bot joins a voice channel,
so changing the model before a join means typing its id by hand. The Settings tab also mixes
what belongs to the window (appearance, diagnostics) with what configures the bot, holds
five long cards in one scroll, and its on/off switches are 15 px checkboxes that are hard to
hit (the user's request of 2026-11-15).

## What Changes

- **Saved Codex lists.** Every time the app reads the models, reasoning efforts and voices
  from Codex (Check Codex, a join, the new Refresh lists button, the read at start), it saves
  them to `codex-lists.json` next to the executable. At start the window's Model, Reasoning
  and Voice lists are filled from that file, so they are there before any Codex process
  runs. When no saved lists exist yet and Codex is installed, the app reads them once at
  start.
- **"Refresh the lists at start" switch** (`codex.refresh_lists_at_start`, off by default) in
  the Codex card: while on, every start reads the lists from Codex and saves them; Codex
  then stops a minute later, as after Check Codex. Next to it the card says when the lists
  were read and has a **Refresh lists** button.
- **Codex at app start.** The app-server may now start at app start, for the read above
  only; it never starts at app start when saved lists exist and the switch is off.
- **Bot settings move to the Config tab.** The Audio, Assistant, Codex and Hotkey cards move
  from Settings to Config, under the "Discord and Codex" card. Settings keeps Appearance and
  Diagnostics, the window's own settings. Fields keep their behaviour: the moved cards save on
  change, the "Discord and Codex" card still waits for Save changes.
- **Collapsible cards.** Every card of the Settings tab (Appearance, Diagnostics) and of the
  Config tab ("Discord and Codex", Audio, Assistant, Codex, Hotkey) collapses and expands
  from its title; the state is kept in `ui.collapsed_cards` and survives a restart. All are
  expanded by default. (The Settings cards were added at the user's request of 2026-11-15,
  after the first trial build: folding on one tab only was inconsistent.)
- **No settings JSON on the Config tab.** The read-only view of `settings.json` is removed;
  the file's location stays, as one line at the bottom of the tab.
- **Large switches.** Every on/off setting (Earcons, Save chat history, the new refresh
  switch, Performance monitoring on Settings and on the Performance tab, Memory measurement,
  Verbose log) becomes a toggle switch with an On/Off label, at least 44 × 24 px, clickable on
  the switch and its label.
- Texts that name the old places ("Settings → Assistant", "Settings → Hotkey", …) name the
  Config tab: in the window, the README and the user guide.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings-ui`: the split between the Settings and Config tabs, collapsible cards, a
  feedback line per tab, large switches, and the Codex lists (saved lists, the refresh
  switch, the read time and Refresh lists).
- `app-window`: the Config tab without the settings view and with the bot's cards; the
  install dialog's scenario names Config → Assistant.
- `codex-assistant`: the app-server may start at app start to read the lists; the lists are
  saved and loaded; a join and Check read them again.
- `app-settings`: the new settings `codex.refresh_lists_at_start` and `ui.collapsed_cards`.

## Impact

- `crates/voicebro-core`: `settings/mod.rs` (two fields, validation), `assistant/mod.rs`
  (`ModelInfo` deserializable, `Capabilities` gains the read time),
  `assistant/codex/mod.rs` (saved lists, forced re-read on Check, once-per-run read on a
  join, start refresh), a new `assistant/codex/lists.rs`, `paths.rs`.
- `apps/desktop`: `src/main.rs` (start refresh after `runtime.start`), `ui/index.html`,
  `ui/main.js`, `ui/styles.css`; `ui-dev/mock.js` and the headless checks (an updated
  `check-wake-dialog.mjs`, a new `check-config-tab.mjs`).
- Docs: `docs/ARCHITECTURE.md` (settings schema, data folder, window, start-up),
  `AGENTS.md` (data folder files, "Nothing of Codex runs before /join"), `README.md`,
  `docs/user-guide.md`.
- No new dependency. A new file `codex-lists.json` in the data folder. A settings file from an
  earlier build loads with both new fields at their defaults; an earlier build keeps them as
  unknown fields.
