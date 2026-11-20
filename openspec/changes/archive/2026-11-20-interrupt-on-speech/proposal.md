# Proposal

## Why

In the Whisper mode a spoken answer stops only after the person's phrase has ended (700 ms
of silence) and Whisper has written it down. On 2026-11-20 the user asked for a separate
setting that stops the answer when the person starts talking: someone says "wait" and goes
on talking, and the assistant should not wait for the end of the phrase and its
recognition before it stops. A sneeze or a short sound must not stop it, so the speech has
to last a sensible time first.

## What Changes

- **A new setting, "Stop the answer when someone starts talking"** (`assistant.interrupt_on_speech`,
  off by default), in Config → Assistant, shown in the Whisper mode.
- **While it is on**, in the Whisper mode during a session, when one person's speech has
  lasted 600 ms (600 ms of frames louder than the speech threshold the app already uses,
  in one utterance), the voice stops at once and the engine's running turn is interrupted.
  What the person says is then written down by Whisper and becomes the next question, as
  today. A sound shorter than 600 ms (a sneeze, a click, a short "yes") stops nothing.
- **While it is off**, nothing changes: the answer stops when a new question has been
  recognized.
- **With "One speaker at a time" on** the new setting has no effect, because in that mode a
  question does not stop the answer by design; the switch is disabled with a note saying
  so.
- The realtime voice mode is not touched: there the Codex call stops its own answer when
  someone speaks. In the text answers mode nothing is spoken, and the setting is not shown.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `whisper-assistant-mode`: the answer stops at the start of speech while the setting is
  on; barge-in on words stays the rule while it is off.
- `app-settings`: `assistant.interrupt_on_speech`.
- `settings-ui`: the switch in the Assistant card.

## Impact

- The core: `settings/mod.rs` (the field), `gate.rs` (counting each speaker's speech in an
  utterance and asking the session to stop the answer), the assistant session trait and the
  text session (`assistant/mod.rs`, `assistant/text.rs`: stop the voice and interrupt the
  turn without starting a new one), the log line.
- The window: `index.html`, `main.js`; the command mock; `check-config-tab.mjs`.
- Docs: `docs/ARCHITECTURE.md`, `docs/user-guide.md`, `AGENTS.md` ("Audio facts").
- No new dependency. Starts from `main` after `text-answers-mode` is merged.
