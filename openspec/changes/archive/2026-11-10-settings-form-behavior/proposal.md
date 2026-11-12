# Proposal

## Why

The user could not choose a value in Settings: the choice reset by itself, again and again,
and it was unclear whether anything was saved. The form is rewritten from the stored
settings every 5 s and after every app event, its lists are rebuilt each time, and some
fields save at once while others need their own Save button.

## What Changes

- The form and the status are separate: periodic and event refreshes update status values
  only; a field is filled from the stored settings when the window loads, after its own save,
  and after a settings change made elsewhere, and never while it is being edited or holds an
  unsaved value.
- Lists (wake engine and model, Codex model, effort, voice) are built when their source
  changes, keeping the current selection; they are never rebuilt under an open list.
- One saving model: the Settings tab saves each field automatically (text on leaving the
  field or Enter, sliders on release, lists and switches on change); the Config tab saves
  with Save changes and discards with Reset.
- Every field shows its state: saving, saved (for two seconds), or the validation error,
  which stays until the value is corrected; a rejected value stays in the field so it can be
  fixed.
- A development page with a mock of the app's commands opens the real UI in a browser, so
  the agent can check the form without the app.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings-ui`: when values are saved and how saving is shown; edits survive refreshes.

## Impact

- `apps/desktop/ui/main.js` (form state separate from the status, save model, field states),
  `styles.css` (field state styles).
- New `apps/desktop/ui-dev/` with the mock of the commands and events (not shipped: outside
  `frontendDist`).
- Docs: `docs/ARCHITECTURE.md` 4.8, `AGENTS.md` and `LOCAL-TESTING.md` (how to open the UI
  with the mock).
