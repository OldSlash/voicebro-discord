# Proposal

## Why

Between Discord and the assistant sits the decision of when the assistant listens: a wake
phrase opens a session, a deactivation phrase or an idle timeout closes it, and mute
overrides everything. The first version's gate (`gate.rs`, ours) encodes the rules that
came out of real use: a 5 s pre-roll replayed from the end of the phrase, holding the replay
while the call connects, counting the assistant's own voice as activity, muting the output
the moment a session ends. This change ports it onto the new audio ports and adds the three
mute sources (app button, global hotkey, Discord panel) sharing one state.

## What Changes

- `voicebro-core::gate`: `RelayGate` per guild and `RelayRegistry`, ported from v1 over
  `AudioPorts`; implements the receive path's `TickSink` and the wake engine's
  `PhraseSink`; talks to the assistant through `AssistantSession`.
- `voicebro-core::assistant`: the `Assistant` and `AssistantSession` traits and a
  `StubAssistant` for tests and for running without Codex.
- Mute: `Runtime::toggle_mute` drives the registry; the Discord panel button and the app
  button already call it; the global hotkey (`tauri-plugin-global-shortcut`) is added in the
  desktop, registered at start and on settings save, status `{shortcut, registered, error}`.
- `AppEvent::Gate` and `GateStatus` in the status snapshot; `/status` and the panel show the
  assistant state from the gate.

## Capabilities

### New Capabilities

- `assistant-session-gate`: when the assistant hears the channel, how a session starts and
  ends, the pre-roll replay, the idle timeout, and what the assistant is told about the
  call.
- `mute-control`: one app-wide mute from three sources, what it silences, and the hotkey.

### Modified Capabilities

None. `hotkey.mute` already exists in `app-settings`.

## Impact

- New modules `crates/voicebro-core/src/gate.rs`, `crates/voicebro-core/src/assistant/mod.rs`;
  `runtime`, `status`, `events` extended; the receive path and the wake worker get their real
  sinks.
- Desktop: `tauri-plugin-global-shortcut` 2.4 and its permission in the capability file.
- No new external services.
