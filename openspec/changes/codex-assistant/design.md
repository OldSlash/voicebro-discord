# Design

## Context

See proposal.md. Target: `docs/ARCHITECTURE.md` 4.5. Sources: v1 `codex.rs` (ours; the
app-server client, thread handling, conversation, chat log, session state machine, tests)
and the probe's `codex.rs` and `rtc.rs` (the JSON-RPC client that already matches the
0.167.0 schema, the working WebRTC call). Confirmed on 2026-11-09: `outputModality: audio`
required; the answer arrives in `thread/realtime/sdp`; `thread/realtime/started` carries the
realtime session id; `transcript/done {role, text}`; decline list includes
`item/permissions/requestApproval`; `on_track` fires with the first packet; Opus codec
registered by hand; the data channel delivers timed JSON events. Not confirmed: continuity
across two calls (the "remember a number" test) and `changes`-mode notes.

## Goals / Non-Goals

**Goals:** the v1 engine without the websocket path, on the in-process WebRTC transport,
behind `Assistant`; canary, continuity and label tests runnable from the probe binary.

**Non-Goals:** no API-key path, no other provider, no standalone Codex download, no use of
the data channel beyond logging.

## Decisions

**D1. The probe's JSON-RPC client becomes `app_server.rs`.** It already speaks the 0.167.0
protocol and declines the right requests; v1's generation counter, request timeouts and
`find_codex` are merged in. `tauri::async_runtime::spawn` → the runtime's tokio `Handle`.

**D2. `rtc.rs` from the probe's call code.** `RtcCall::open(offer_exchange)` builds the peer,
one Opus track, the `oai-events` channel, waits for ICE gathering (≤ 2 s), returns the
offer; the session exchanges it through `thread/realtime/start` and sets the answer. Send
side: gated frame → mono → Opus (VOIP, complexity 5, FEC) → `sample_writer`. Receive side:
`on_track` → decode → duplicate to stereo → `OutputHandle::push`. Data channel messages are
logged at Debug with their `type`.

**D3. `session.rs` is v1's `CodexSession` state machine** (idle → starting → live →
stopping, `wanted`, restarts ≤ 3, stop completes before restart, flush on not-wanted) with
`rtc.rs` as the only transport. `ready()` = live and the peer connected.

**D4. `conversation.rs` is v1's `Conversation`** with English text: the history hint, the
call context ("In the channel: A, B. X has just addressed you."), the `changes` note.
Instructions and context go last in `initialItems`; `prompt` is also set; history capped at
100 items / 24 000 bytes; one retry without history.

**D5. `thread.rs` opens one thread per guild** with `cwd` = `codex-workspace\` next to the
exe, `sandbox: read-only`, `approvalPolicy: never`, `ephemeral: true`,
`developerInstructions` = the user's instructions, optional `model` and
`config.model_reasoning_effort`; a change reopens the thread and remaps the session; a
rejected model falls back with `status::log_used`.

**D6. Voices.** `thread/realtime/listVoices` once per process; a configured voice outside
the v1 list for the account falls back to `defaultV1` with `log_used`.

**D7. Chat log** as in v1: bounded `VecDeque` (600), `AppEvent::Chat`, daily file through
`chrono` local date; delegated task text unwrapped from `<realtime_delegation>`.

**D8. Process lifecycle.** `CodexAssistant::start()` when Discord connects (the thread needs
the guild), `stop()` at app exit kills the child (`kill_on_drop`) after `thread/realtime/stop`
for any live call.

**D9. Tests in the probe binary**: `rtc-call` stays as the canary regression; a new
`codex-conversation <q1.wav> <q2.wav>` runs two calls with a stop between and checks the
second answer refers to the first; `codex-labels` checks a `changes` note is sent once per
speaker change.

## Risks / Trade-offs

- [The realtime model answers the greeting before the question ends (barge-in)] → known
  from v1 and the probe; instructions may reduce it; not solved here.
- [Delegation latency 0.4–9.5 s] → inherent to the voice model; the instructions ask it to
  answer simple turns directly.
- [Experimental protocol changes with a Codex update] → the probe `codex-schema` is the
  regression check; the app logs the Codex version at start.

## Open Questions

None.
