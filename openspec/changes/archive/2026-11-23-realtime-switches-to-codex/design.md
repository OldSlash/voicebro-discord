# Design

## Context

See proposal.md for the request. Today, in `apps/desktop/ui/main.js`:

- `applyEngine()` sets `disabled = engine !== "codex"` on the `realtime` option of
  `#assistant-mode`, renames it "Realtime voice (works only with Codex)" and fills
  `#assistant-mode-note` with "The voice mode works only with Codex; <product> answers in
  the Whisper mode." A disabled `<option>` cannot be picked, which is the lock.
- `#assistant-mode` has no handler of its own: its change goes through `saveControl`, which
  saves `{ assistant: { mode } }`.
- `chooseEngine(select)` handles the Engine list: it refuses the change while the install
  dialog or the Claude dialog is open, adds `assistant.mode = "whisper"` to the patch when
  Claude Code or OpenCode is picked in the realtime mode, saves once, then runs
  `fillForm()`, `markSaved()`, clears `capabilitiesKey`, awaits `loadCapabilities()` and
  calls `refresh()`.

In the core, `Settings::validate` refuses `realtime` with an engine other than Codex, and
`Runtime::apply` handles a save that changes both `engine` and `mode`: the `engine` part
wins the path (`rebuild_gates` and `switch_engine`), the `mode` part still configures the
wake engine, Whisper and the voice. That path is tested from Codex in the realtime mode to
Claude Code in the Whisper mode (`switching_the_engine_in_a_channel_keeps_the_bot_and_moves_the_session`),
not the other way.

## Goals / Non-Goals

**Goals:**
- One step from Claude Code or OpenCode back to the Codex realtime voice, from the mode
  list.
- The same save, refresh and failure handling as the Engine list's choice.

**Non-Goals:**
- No change to the core's rules or to the settings document: the window sends a valid
  document.
- The Engine list stays as it is; picking Codex there still keeps the Whisper mode.
- No memory of the previous engine or mode for a later switch back.

## Decisions

### D1. The window adds the engine to the patch; the core stays strict

A new `chooseMode(select)` handles `#assistant-mode`'s change (next to `chooseEngine` in the
field's `change` listener). When the new value is `realtime` and `engineOf() !== "codex"`,
the patch is `{ assistant: { mode: "realtime", engine: "codex" } }`; otherwise it goes
through `saveControl` as today, so a mode change with Codex, or between Whisper and text
answers, is untouched.

Alternative: let `Settings::validate` (or the save path) turn `realtime` with Claude Code
into Codex by itself. Rejected: a save would then store something the patch did not say,
the window would have to guess what changed, and the refusal that protects hand-edited
files and other callers would go. The mirror direction is already done in the window
(`chooseEngine`), so this keeps both directions in one place.

### D2. One shared step after an engine change

`chooseEngine` and `chooseMode` share the steps after a save that changed the engine:
`fillForm()`, `markSaved()`, a cleared `capabilitiesKey`, `await loadCapabilities()` and
`refresh()`. They move into one helper (for example `afterEngineSaved(select)`), so the
Codex card's model and voice lists are rebuilt for Codex exactly as after picking Codex in
the Engine list. `fillForm()` sets the Engine list to Codex and `applyEngine()` (called from
it) retitles the card, swaps its rows, updates the Status tab's engine buttons and hides the
Claude subscription notice.

`chooseMode` refuses the change while the install dialog or the Claude dialog is open, as
`chooseEngine` does. On a refused save the select goes back to the stored mode, the field
state is `error` and the message shows under the mode list; nothing else is touched,
because nothing else was saved.

### D3. The option is never disabled; the note warns instead

`applyEngine()` no longer touches the option's `disabled` or text: it keeps its label
"Realtime voice (Codex voice call, Vosk wake word)", which already names Codex. While the
engine is Claude Code or OpenCode, `#assistant-mode-note` reads "The voice mode works only
with Codex: choosing Realtime voice switches the engine to Codex." and is empty with Codex.
The mode's help line in `index.html` ends its Realtime part with "Codex only: choosing it
while Claude Code or OpenCode is the engine switches the engine to Codex."

Alternative: keep a different option label for the other engines ("Realtime voice (switches
to Codex)"). Rejected: one label is simpler to read and test, and the note sits right
under the list.

### D4. What follows the engine stays as with the Engine list

The switch is an ordinary engine change for everything downstream, so the behaviour is the
one the user already gets by picking Codex in the Engine list and then the mode:

- In a voice channel the core's engine path runs (D1 of `claude-and-opencode-engines`):
  sessions end with the falling tone, Claude Code's or OpenCode's process stops, Codex is
  prepared and attached in the realtime mode, or the `assistant` connection error says why
  it could not start.
- Consent records follow `consent::recipient_key`, as for any engine change: records given
  for Claude Code's or OpenCode's provider are dropped when the recipient becomes Codex.
- Missing Vosk files show the existing notice under the wake model, and a `/join` with the
  wake word is refused for them as today.
- The `claude.*` and `opencode.*` sections are not part of the patch and stay as stored.

## Risks / Trade-offs

- [The user picks the realtime voice by mistake and loses consent records given for
  Claude Code or OpenCode] → The same happens today with the Engine list; the note under
  the mode says beforehand that the engine switches to Codex.
- [The engine changes from a list labelled "Assistant mode"] → The note and the help line
  say so before the choice, the Engine list shows Codex right after it, and it mirrors the
  existing switch in the other direction that the user asked to keep.
