# Proposal

## Why

On 2026-11-20 the user asked for a separate mode without the voice: exactly the Whisper
mode, except that the answer is not played into the voice channel and is posted as text in
a Discord chat instead.

## What Changes

- **A third assistant mode, `text`** ("Whisper, text answers"). Everything works as in the
  Whisper mode: Whisper on this PC writes down what is said and spots the phrases, the
  selected engine (Codex, Claude Code or OpenCode) answers each question in a text turn,
  barge-in, one speaker at a time, the session rules, the access filter and the earcons
  stay as they are. The difference: no local voice is loaded or played, and each completed
  answer is posted as text.
- **Where the answer goes**: the existing "Post answers to Discord" setting. While it is
  Off in this mode, the answer goes to the voice channel's chat, so that it is never lost.
- **A join in this mode needs only Whisper's files**: the voice is neither loaded nor
  required, and the refused join's dialog and the Config → Assistant note list only
  Whisper's files.
- Where the window and Discord name the mode (the Assistant mode list, the Status tab,
  `/status`, the member panel) the new mode is named; the Local voice card says its voice
  is not used in this mode; the latency line leaves out the voice's times.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `whisper-assistant-mode`: the assistant modes include `text`; the text answers mode
  follows every Whisper mode rule except the voice; its answers are posted, also with
  posting off; its latency line has no voice times.
- `app-settings`: `assistant.mode` takes `text`.
- `assistant-answer-posting`: in the text answers mode an answer is posted while posting is
  off, to the voice channel's chat.
- `assistant-engines`: Claude Code and OpenCode work in the text answers mode too.
- `codex-assistant`: a join in the text answers mode prepares Whisper without the voice.
- `settings-ui`: the mode's third choice; the Local voice note, the posting help and the
  Assistant card's note in the new mode.
- `app-window`: the refused join's dialog in the text answers mode lists Whisper's files
  only.
- `status-view`: the Status tab names the mode and says no voice is used.

## Impact

- The core: `settings/mod.rs` (`AssistantMode::Text`), every place that checks for the
  Whisper mode (`gate.rs`, `runtime.rs`, `stt/manager.rs`, `tts/manager.rs`,
  `wake/mod.rs`, the engines' `mod.rs`), the text session (`assistant/text.rs`: no speech),
  answer posting (`discord/answers.rs` or its caller), the latency line, the mode names
  (`discord/mod.rs`).
- The window: `index.html`, `main.js`; the mock (`?text_answers`); `check-whisper-config.mjs`
  and `check-engine-config.mjs`.
- Docs: `docs/ARCHITECTURE.md`, `docs/user-guide.md`, `AGENTS.md`.
- Depends on `models-in-settings` (the dialog and the Config note it adds); starts from
  `main` after that change and `claude-notice-hide` are merged.
