# Tasks

## 1. A way to check the UI

- [ ] 1.1 Create the branch `change/settings-form-behavior` from the tip of `change/ui-layout-port`; verify `git log -1` shows the previous tip as parent
- [ ] 1.2 Add `apps/desktop/ui-dev/` with the command and event mock of design D5 (settings document, validation messages for the timeout, phrase overlap, gains and theme, status events every second); verify the page opens from disk in the built-in browser pane, every tab renders, and the page's console shows no unknown command

## 2. Form state and lists (spec `settings-ui`)

- [ ] 2.1 Keep per-field stored value, input and state; stop `renderStatus` from writing form fields; fill fields only when clean and not focused at load, after their save and on `settings_changed`; verify with the mock: a typed but unsaved Config value survives 30 s of events
- [ ] 2.2 Build lists only when their source changes, never while open, and keep the selection; verify with the mock: open the wake model list, let 30 s of events and one capabilities change arrive, the list stays open with the same highlighted choice

## 3. Saving model and field states (spec `settings-ui`)

- [ ] 3.1 Settings tab auto-save per design D3 and the field status line ("Saving…", "Saved" for 2 s, error kept); verify with the mock: a model choice saves without a button and shows "Saved", the timeout 3 stays in the field with the range message and the stored value unchanged, a phrase overlap names the phrase and both lists
- [ ] 3.2 Config tab dirty markers, Save changes for all dirty fields and the token, Reset to the stored values; verify with the mock: editing the executable marks it, Reset clears the marker and restores the value, Save changes sends one patch
- [ ] 3.3 Check the same three flows in the running app with a scratch data dir by reading `settings.json` after each (the clicking itself is the user's step in the real window); verify the file holds the saved values and not the rejected one

## 4. Docs and checks

- [ ] 4.1 Update `docs/ARCHITECTURE.md` 4.8 (form state, saving model), `AGENTS.md` and `LOCAL-TESTING.md` (how to open `ui-dev` in a browser); verify the documented path opens the page
- [ ] 4.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output

## Workflow follow-up

- Checkpoint commits per task group on `change/settings-form-behavior`, with the user's OK.
- The user checks the form in the real window.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
