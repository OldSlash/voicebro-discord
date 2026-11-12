# Proposal

## Why

A review of the app's technical decisions found several that do not work as the user needs:
the Codex process runs from app start, mute keeps the call open, overlapping voices clip,
the recognizer's results never reach the window, an unnamed speaker stays unnamed, and the
bot does not connect at launch for this user. The user requires each of them changed as
listed below.

## What Changes

- **Codex at `/join`.** Nothing of Codex runs at app start. `/join` starts the app-server,
  checks the sign-in (and refuses the join with the reason when Codex is missing or signed
  out) and opens the guild's thread; the first activation only places the call. The
  app-server stops when the bot leaves its last voice channel, one minute after an on-demand
  Sign in or Check when the bot is in no channel, and at exit.
- **Mute ends the session**: the call closes, nothing is spent while muted; in always mode
  unmute starts a new session.
- **Mute and unmute tones** next to the session start and end tones.
- **Overlapping voices are averaged** instead of summed and clamped.
- **The window gets the whole app log** (Info and above from this app's crates), so what the
  recognizer heard and every phrase decision are visible, not only in the file.
- **Unknown speaker fallback**: the only non-bot member in the channel is taken as the
  speaker of an unnamed audio source, as the existing `voice-channel-session` spec already
  requires and the code does not do.
- **Slash commands** are registered in the configured guild and globally.
- **Discord always starts** when a token is stored, at launch and after a token is saved;
  the `auto_connect` switch is removed.
- **A small Discord cache**: guilds, voice states and the members needed for names only.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `codex-assistant`: the app-server lifecycle moves to `/join`; the join is refused when
  Codex cannot serve it.
- `mute-control`: mute ends the session and its call; mute and unmute tones.
- `assistant-session-gate`: in always mode a session is wanted only while not muted.
- `audio-transport`: overlapping speakers are averaged; mute no longer sends a silent stream.
- `discord-connection`: the bot always connects when a token is stored; commands are
  registered in the guild and globally.
- `diagnostic-logging`: the app log reaches the window.
- `settings-ui`: the auto-connect control is gone.

## Impact

- `assistant/codex/mod.rs` (start and stop of the app-server, join preparation),
  `runtime.rs` and `discord/handler.rs` (join flow, start at launch, command registration),
  `discord/receive.rs` (unknown-speaker fallback), `audio/frame.rs` (`mix_into`),
  `gate.rs` and `audio/earcon.rs` (mute ends the session, mute tones), `discord/mod.rs`
  (cache settings), `settings` (`auto_connect` dropped and ignored when present),
  `apps/desktop` (log forwarding to the window, the Settings tab without auto-connect).
- Docs: `docs/ARCHITECTURE.md` 4.2, 4.3, 4.5, 4.7, 4.8 and `AGENTS.md`.
