# Proposal

## Why

During the user's session the Chat tab stayed empty and every chat entry was printed as raw
JSON into the log pane (`docs/NEXT-CHANGES.md`, section 4). The cause is a field name
collision in the event that carries a chat entry. The user wants the first version's Codex
chat and Console tabs, which show what happens in the conversation and in the app.

## What Changes

- Chat entries reach the Codex chat tab live: the event tag no longer collides with the
  entry's own `kind` field, and a test guards every event against that collision.
- **Codex chat** as in the first version: toolbar Copy, Open log folder, filters "Names and
  notes", "Codex agent work", "Call events", "Auto-scroll"; one row per entry with time, a
  coloured source label and the text. Labels: the speaker's name for a user transcript,
  `GPT` for the voice answer, `Context → GPT` for names and notes sent to the voice model,
  `Task → Codex` for a delegated task, `Codex` for the agent's message, `Codex thinking` for
  reasoning, `Codex tool` for tool use, `Call` for call events, `Error` for errors. The voice
  answer and delegated tasks are highlighted.
- **Console** as in the first version: the app log forwarded by change
  `first-version-parity`, with Clear, Copy, Copy selection, search with a match count and
  Auto-scroll.

## Capabilities

### New Capabilities

- `app-console`: the Console tab: the app log in the window with search and copy.

### Modified Capabilities

- `assistant-chat-log`: the Chat tab becomes the first version's Codex chat with source
  labels; entries arrive live.

## Impact

- `crates/voicebro-core/src/events.rs` (tag rename) and a serialization test;
  `apps/desktop/ui/main.js` and `styles.css` (the two tabs: the first version's chat view, the owner's code,
  and a console written new).
- Docs: `docs/ARCHITECTURE.md` 4.5 (chat log event) and 4.8.
