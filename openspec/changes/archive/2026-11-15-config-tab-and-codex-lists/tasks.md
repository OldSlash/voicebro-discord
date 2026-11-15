# Tasks

Decisions the request left open are in design.md, marked **[decided alone]** (D1, D4, D5,
D6, D8, D9). Questions that come up while building are decided the same way and added to
design.md under the decision they touch, so the user can review them all at the end.

## 1. Branch and settings fields

- [x] 1.1 Create `change/config-tab-and-codex-lists` from `main` and check `git status` shows that branch with only the untracked `docs/HANDOFF-MODES-REPORT.md` and this change's folder
- [x] 1.2 Add `codex.refresh_lists_at_start: bool` (default `false`, doc comment) to `CodexSettings` and `ui.collapsed_cards: Vec<String>` (default empty) to `UiSettings` in `crates/voicebro-core/src/settings/mod.rs`; verify with new tests that a 1.0.1 file loads both defaults and a save writes them, a patch with `refresh_lists_at_start: "yes"` or `collapsed_cards: 3` is rejected with the stored settings unchanged, and `["audio", "later-card"]` survives a load/save round trip (`cargo test -p voicebro-core settings::`)
- [x] 1.3 Check that a save changing only `codex.refresh_lists_at_start` or `ui.collapsed_cards` restarts nothing (a runtime test that saves each while a fake gate is registered and asserts no gate rebuild and no wake reload), and update the settings schema block in `docs/ARCHITECTURE.md` 4.6 with both fields

## 2. Saved Codex lists in the core

- [x] 2.1 Add `paths::codex_lists_path(data_dir)` (`codex-lists.json`), derive `Deserialize` for `ModelInfo`, and add `lists_read_at_ms: Option<u64>` to `Capabilities` (and `Default`); verify `cargo build -p voicebro-core` and the existing capabilities tests pass
- [x] 2.2 Create `crates/voicebro-core/src/assistant/codex/lists.rs` with `SavedLists` (version 1, `read_at_ms`, `codex_version`, models, voices, default voice), `load` (missing → `None`; unreadable or invalid → `None` plus one warning log line naming the path) and `save` (temp file plus rename); verify unit tests for round trip, missing file, invalid JSON, a leftover `.tmp` replaced
- [x] 2.3 In `CodexAssistant::new` load the file into `models`, `voices` and the read time with `lists_fresh = false`; after every successful `model/list` set `lists_fresh`, the read time and save the file with the voices held (D2); a failed read changes neither memory nor file; verify with the Node fake that a saved file fills `capabilities()` with its read time and no app-server is running, and that `check()` writes the file
- [x] 2.4 Give `refresh_voices` a `force` flag (Check forces) and make `prepare_inner` read models and voices when `!lists_fresh`; verify with the fake that the first `prepare_guild` after loading a file reads `model/list` once, a second one does not, and two `check()` calls read `listVoices` twice (count the requests the fake answers, extending `fake_codex.js` with a request log file if it has none)
- [x] 2.5 Document the saved lists in `docs/ARCHITECTURE.md` 4.5 (lists, freshness, file) and 4.6 (the data folder now also holds `codex-lists.json`), and add `codex-lists.json` to the data folder list in `AGENTS.md`

## 3. Read at start

- [x] 3.1 Add `CodexAssistant::read_lists_at_start(&self, enabled: bool) -> bool` (D3: no executable → `false`, nothing started, no error; `enabled` or no saved lists → spawn `check()`, log "Codex: reading the lists at start (<reason>)", `true`; otherwise `false`); verify with the fake: saved lists and `false` start nothing, no lists reads and writes the file, `true` reads, a missing executable returns `false` with no last error set
- [x] 3.2 Call it from `apps/desktop/src/main.rs` `setup` right after `runtime.start()` with `settings.codex.refresh_lists_at_start`; verify `cargo build -p voicebro-desktop` and, with a scratch `VOICEBRO_DATA_DIR`, that the first start logs the read and writes `codex-lists.json`, a second start with the switch off has no `codex.exe` child of `voicebro.exe` after 10 s, and with the switch on a child appears and is gone about 70 s after start
- [x] 3.3 Update the start-up text in `docs/ARCHITECTURE.md` 4.7 and the "Nothing of Codex runs before `/join`" line in `AGENTS.md` (Audio facts) to name the read at start and when it happens

## 4. Config tab layout

- [x] 4.1 Move the Audio, Assistant, Codex and Hotkey cards in `apps/desktop/ui/index.html` from the Settings panel into the Config panel under "Discord and Codex", in that order, one column of the Config card's width; Settings keeps Appearance and Diagnostics; remove `#config-preview` and its writes in `main.js`; put "Settings file: <path>" (`#config-path`) at the bottom of the Config tab outside every card; verify in the mock (`node apps/desktop/ui-dev/serve.mjs`) that both tabs show the right cards and no JSON
- [x] 4.2 Give each tab with auto-saved fields its own feedback line (`#settings-feedback`, new `#config-autosave-feedback`, both `data-autosave`) and make `markSaved` write the line of the control's tab panel only (D7); verify in the mock that saving the font size changes only the Settings line and saving the mute hotkey only the Config line
- [x] 4.3 Change the texts that name moved cards to "Config → …" in `main.js`, the Config card's note on Save changes versus the cards below, `README.md` and `docs/user-guide.md` (tab table split into Settings and Config rows); verify `rg "Settings → (Assistant|Audio|Codex|Hotkey)" apps docs README.md` finds nothing
- [x] 4.4 Update `apps/desktop/ui-dev/check-wake-dialog.mjs` for the feedback line now on the Config tab and run it: `node apps/desktop/ui-dev/check-wake-dialog.mjs` passes every scenario
- [x] 4.5 Create `apps/desktop/ui-dev/check-config-tab.mjs` (same harness as the wake check) with the layout scenarios of 4.1 and 4.2 and the Config tab's two saving ways (gain saved at once while an unsaved guild id stays marked); verify it passes
- [x] 4.6 Update the window section of `docs/ARCHITECTURE.md` 4.8 (Settings and Config contents, the feedback lines, no settings view) and `AGENTS.md` where it describes the tabs

## 5. Collapsible cards

- [x] 5.1 Add `data-card` ids and a title button (`aria-expanded`, `aria-controls`, chevron) to the five Config cards, a body container toggled by `hidden`, and the styles in `styles.css` (D8); the "Discord and Codex" badge stays in the title row
- [x] 5.2 Apply `ui.collapsed_cards` in `fillForm` on load and on every settings change, save toggles through `save_settings` keeping unknown ids, with no saved mark or feedback line; add the mock's handling of the field
- [x] 5.3 Extend `check-config-tab.mjs`: collapse by click and by Enter and Space, state saved and restored after a reload of the page, first start all expanded, an unknown id kept, an unsaved guild id kept through collapse and expand, the settings path shown with every card collapsed, the install dialog's Not now with the Assistant card collapsed; verify it passes
- [x] 5.4 Describe the collapsible cards and `ui.collapsed_cards` in `docs/ARCHITECTURE.md` 4.8 and the user guide

## 6. Large switches

- [x] 6.1 Replace every `data-kind="bool"` checkbox (Earcons, Save chat history, Performance monitoring in Diagnostics and on the Performance tab, Memory measurement, Verbose log) with the switch markup of D9 and its styles in all four themes; `percentLabel` writes "On"/"Off" next to the switch; the name-line `<output>` for switches goes
- [x] 6.2 Extend `check-config-tab.mjs`: every switch ≥ 44 × 24 px at font size 13, a click on its On/Off label toggles and saves it, Space on the focused switch toggles and saves it, the Earcons switch and the Earcon volume slider share their name line and their control line, the two Performance monitoring switches stay mirrored; verify it passes
- [x] 6.3 Screenshot the Settings and Config tabs in the Dark and Light themes from the mock in the browser pane and check the switches, focus marks and collapsed cards by eye

## 7. Codex lists controls in the window

- [x] 7.1 Add the Codex card's first row (D6): the "Refresh the lists at start" switch (`codex.refresh_lists_at_start`) with its help line, and the "Lists" field with the read time ("Read <date time>", "Never read", "Reading…") and the Refresh lists button calling `codex_check` then `capabilities`; the read time is rewritten on every capabilities load, the lists rebuild only when they changed
- [x] 7.2 Mock: `capabilities.lists_read_at_ms`, `?nolists` (no lists, never read), `__mock.failNextCheck()`, `codex_check` setting the read time to now
- [x] 7.3 Extend `check-config-tab.mjs`: lists filled at load from saved capabilities with the stored model, effort and voice selected; `?nolists` shows free text and "Never read"; Refresh lists disables the button, then shows the new time and a model added by `__mock.changeCapabilities()` with the stored choice still selected; a failed check shows the error and keeps the lists; turning the switch on saves it and calls no command but `save_settings`; verify it passes
- [x] 7.4 Document the controls in `docs/ARCHITECTURE.md` 4.8 (Codex card, the `codex_check` reuse) and in the user guide's Codex part

## 8. Integration checks

- [x] 8.1 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` (CMake on PATH as `AGENTS.md` says) and record what each returned
- [x] 8.2 Run the real app (`cargo run -p voicebro-desktop`, scratch `VOICEBRO_DATA_DIR`, no Discord token, WebView2 debugging port): Settings shows Appearance and Diagnostics only; Config shows the five cards; collapse two, restart, they stay collapsed; the Codex lists show at the second start without a `codex.exe` child; Refresh lists updates the read time; switches toggle by label; screenshot both tabs
- [x] 8.3 `openspec validate config-tab-and-codex-lists --strict` passes, and the report to the user lists what was not tested: everything with Discord (a join reading the lists, a join during the read at start, two people and a game), and Codex signed out at start

## 9. Collapsible Settings cards (the user's request after the first trial build, 2026-11-15)

- [x] 9.1 Give the Appearance and Diagnostics cards of the Settings tab the same title button and body as the Config cards (`data-card="appearance"`, `"diagnostics"`); verify in the mock that both fold and the Settings tab's feedback line stays outside them
- [x] 9.2 Extend `check-config-tab.mjs`: every card of both tabs is expanded at first start, a Settings card folds, saves its id and comes back folded after a restart, and the Diagnostics switches still toggle after it is expanded again; verify it and `check-wake-dialog.mjs` pass
- [x] 9.3 Update the doc comment of `UiSettings::collapsed_cards`, the settings schema line and the window section in `docs/ARCHITECTURE.md`, the user guide and `AGENTS.md` to say both tabs fold; verify `cargo test -p voicebro-core settings::` and `openspec validate config-tab-and-codex-lists --strict` pass
- [x] 9.4 Rebuild the trial (`scripts\build-release.ps1 -Trial`) and hand its zip to the user

## Workflow follow-up

- When the user says to commit: archive the change into `openspec/changes/archive/<date>-config-tab-and-codex-lists` with its delta specs merged into `openspec/specs/`, and make the branch's one commit with the code, docs and archive, after `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`.
- When the user says to merge: `git merge --no-ff --log change/config-tab-and-codex-lists` into `main` with "Merge change/config-tab-and-codex-lists: <result>", then delete the branch.
- A build for the user comes only from `main` at a new `v<version>` tag through `scripts\build-release.ps1`.
