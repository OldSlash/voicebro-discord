# Proposal

## Why

The assistant is the product: the people in the channel ask, ChatGPT answers in voice. The
Codex CLI app-server is the only sanctioned path with a ChatGPT sign-in, and the live probes
on 2026-11-09 proved the whole chain from Rust: `thread/realtime/start` over WebRTC, v3,
the canary instruction in `initialItems`, 9 s of voice decoded, the data channel's events.
This change turns that probe into the assistant engine, with conversation continuity across
calls, speaker labels and a chat log.

## What Changes

- `voicebro-core::assistant::codex`: the app-server client (JSON-RPC over stdio, every
  server request declined), account (sign in, check), one read-only ephemeral thread per
  guild with the configured model and reasoning effort, the WebRTC call inside the process
  (`webrtc` 0.21 + `opus2`), the session state machine (idle → starting → live → stopping),
  conversation continuity across calls, speaker label modes, and the chat log (memory, event,
  daily file).
- `CodexAssistant` implements `Assistant`; `Runtime` attaches it instead of the stub when
  `assistant.engine` is `codex`; the app-server starts when the bot connects and stops at
  exit.
- `AppEvent::Assistant`, `AppEvent::Chat`, `AssistantStatus` (process, account, call state,
  last error, model and effort in use, voice in use).
- Desktop: `codex_sign_in`, `codex_check`, `codex_models`, `chat_log`, `open_log_folder`
  commands; Status tab shows the account and call state; the Chat tab lists the log.

## Capabilities

### New Capabilities

- `codex-assistant`: the app-server process, sign-in, thread settings, the call lifecycle
  and its safety rules.
- `assistant-conversation`: what the assistant is told and how the conversation continues
  from one call to the next.
- `assistant-chat-log`: what the user can see of the conversation and the delegated work.

### Modified Capabilities

None. `codex.*` and `assistant.speaker_labels` already exist in `app-settings`.

## Impact

- New module `crates/voicebro-core/src/assistant/codex/`; `runtime`, `status`, `events`
  extended.
- New dependencies: `webrtc` 0.21, `rtc` 0.21, `opus2` 0.4 (shared libopus with songbird),
  `bytes`, `async-trait`, `rand`, `chrono` (daily file names).
- Needs an installed Codex CLI signed in with a ChatGPT account; the app finds `codex.exe`
  or uses `codex.executable`.
- Every call uses the signed-in account's ChatGPT limits; one account for the whole channel
  is the user's decision (plan, risks).
