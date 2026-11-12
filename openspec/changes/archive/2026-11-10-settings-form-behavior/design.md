# Design

## Context

See proposal.md. In today's `apps/desktop/ui/main.js`, `renderStatus` writes every
`[data-path]` control from `status.settings` and calls `applyWakeLists`, which empties and
refills the wake lists; `refresh` runs every 5 s and on every event; `loadLists` empties and
refills the Codex lists on every assistant event; the only guard is
`document.activeElement === control`, and for the Codex lists it guards the hidden text
input. After change `ui-layout-port` the same logic sits in the seven-tab layout.

## Goals / Non-Goals

**Goals:** a form that never fights the user; one visible saving model; a way for the agent
to check the UI without the app.

**Non-Goals:** new settings; the look of the cards (change `ui-layout-port`).

## Decisions

**D1. Form state apart from the status.** The page keeps, per field, the stored value, the
current input and a state (clean, dirty, saving, saved, error). `renderStatus` touches no
form field. A field is written from the stored settings only when it is clean and not
focused, at load, after its own save and on `settings_changed`. The status fields
(tiles, status bar, counters) keep refreshing on events and every 5 s.

**D2. Lists keep their selection.** A list is rebuilt only when its source changes (the
wake catalog at load; Codex models and voices when the capabilities differ from the last
ones), never while it is open (a `pointerdown` or `focus` on the select marks it busy until
`change` or `blur`); a rebuild re-selects the current value, adding it as "(not in the
list)" if needed.

**D3. Saving model.** Settings tab: text and number fields save on `change` (leaving the
field or Enter), sliders on `change` (release), selects and switches on `change`; each save
is one `save_settings` patch for that field. Config tab: fields become dirty on input; Save
changes sends one patch with every dirty field and the token through `set_secret` when one
was typed; Reset restores the stored values. *Alternative:* a Save button per field.
Rejected: the user did not know what was saved.

**D4. Field state display.** A small status line under each field: "Saving…", "Saved"
(cleared after 2 s), or the error text (kept until the next successful save of that field).
Dirty Config fields get a marker; the Save changes button is enabled only while something is
dirty.

**D5. The command mock.** `apps/desktop/ui-dev/index.html` loads a script that defines
`window.__TAURI__.core.invoke` and `window.__TAURI__.event.listen` over an in-memory settings
document with the app's validation messages for the common cases, emits status events every
second, and then loads the real `../ui/` page files. It is opened from disk in a browser
(the agent uses the built-in browser pane). It lives outside `frontendDist`, so it never
ships.

## Risks / Trade-offs

- [The mock drifts from the real commands] → it implements only the commands the page calls,
  with the same names and payload shapes, and the page logs any unknown command it calls.
- [Auto-save of a half-typed value] → text saves on leaving the field or Enter, not on every
  keystroke.
