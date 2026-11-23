# Proposal

## Why

On 2026-11-22 the user reported that the engine and mode switch is lopsided. Picking
Claude Code or OpenCode in the realtime voice mode switches to the Whisper mode in one
step, but the way back is locked: while Claude Code or OpenCode is the engine, the
"Realtime voice" choice of Config → Assistant → Assistant mode is disabled ("Realtime
voice (works only with Codex)"), so going back to the ChatGPT voice call takes two steps,
first Codex in the Engine list and then the mode. The user wants no lock: choosing the
realtime voice should simply switch back from Claude Code to Codex.

## What Changes

- **"Realtime voice" is never disabled.** The Assistant mode list offers it with every
  engine, under its usual label "Realtime voice (Codex voice call, Vosk wake word)".
- **Choosing it with Claude Code or OpenCode switches the engine to Codex** in the same
  save (`assistant.mode = realtime` and `assistant.engine = codex`), the mirror of what the
  Engine list does today the other way. The engine card becomes the Codex card, the Engine
  list shows Codex, and the Claude Code or OpenCode settings stay stored for the next time.
- **The window says so before the choice:** while Claude Code or OpenCode is the engine,
  the note under the mode reads that the voice mode works only with Codex and that choosing
  it switches the engine to Codex; the mode's help line says the same.
- **A refused save changes nothing:** the mode and the engine stay as they were and the
  reason shows under the mode list.
- **Unchanged:** the settings validation still refuses a document with `realtime` and an
  engine other than Codex (the window sends both values in one save); the Engine list
  behaves as today; the Whisper and text answers modes still work with every engine; the
  switch in a voice channel follows the engine switch as it is today (the session ends, the
  bot stays, the old engine's process stops, Codex is prepared and attached).

Assumption, recorded here: "the switch locks" means the disabled "Realtime voice" choice.
The Engine list itself already lets the user pick Codex while the mode is Whisper, and
the core already accepts that.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings-ui`: requirement "Engine choice in the Assistant card": "Realtime voice" stays
  selectable with every engine, and choosing it with Claude Code or OpenCode saves the
  engine Codex with the realtime mode in one save.
- `assistant-engines`: requirement "The engine switches at once" gets a scenario for one
  save that changes both the engine and the mode back to Codex and the realtime voice while
  the bot is in a channel.

## Impact

- The window: `apps/desktop/ui/main.js` (`applyEngine` no longer disables the option; a
  mode choice handler that adds `assistant.engine = codex`), `apps/desktop/ui/index.html`
  (the mode's help line); `apps/desktop/ui-dev/mock.js` (a way to refuse the next save) and
  `apps/desktop/ui-dev/check-engine-config.mjs` (scenarios updated and added).
- The core: no behaviour change; a settings test and a runtime test for the combined
  save from Claude Code in the Whisper mode to Codex in the realtime mode.
- Docs: `docs/user-guide.md` (section 4), `docs/ARCHITECTURE.md` (Config → Assistant and
  the engine-switch flow), `AGENTS.md` (the mock helper and the check's scenarios; local).
- No new setting, no settings schema change, no new dependency, no version change.
