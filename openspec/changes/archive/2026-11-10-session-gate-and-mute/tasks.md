# Tasks

## 1. Assistant interface

- [x] 1.1 Add `assistant/mod.rs` with `Assistant`, `AssistantSession`, `AudioPorts` use, `Capabilities`, `AssistantStatus`, and `StubAssistant` (ready after a delay, records frames and calls); verify unit tests that the stub reports ready after the delay and stores the call context

## 2. Gate (spec `assistant-session-gate`)

- [x] 2.1 Write `gate.rs` (`RelayGate`, `RelayRegistry`, `GateSettings`, `GateStatus`, `ActivationMode`, `SpeakerNamesMode`, constants) over `AudioPorts` and the event bus, with its unit tests; verify `cargo test -p voicebro-core gate::` passes
- [x] 2.2 Implement `TickSink` and `PhraseSink` for the gate; feed per-speaker frames to `WakeFeed` only in wake-word mode when not muted; verify tests with the stub assistant: replay starts at the phrase end, holds until ready, skips silence, caps at 300 frames
- [x] 2.3 Idle timeout counting assistant audio (`OutputHandle::last_signal`), deactivation flush, output muted when no session in wake-word mode, replay timeout ends the session with an error; verify tests for each
- [x] 2.4 Wire `Runtime`: build a gate per join from `Settings`, register it, preload the speaker directory, attach the assistant session (stub until Codex), apply `session`/`wake`/`assistant` setting changes to live gates, `AppEvent::Gate`, `Status.gates`; verify a live run in always mode where joining makes the session wanted (stub logs it) and leaving ends it (covered by a runtime unit test driving `on_join`/`on_leave`; the live `/join` needs a person in the channel and is the user's step)

## 3. Mute (spec `mute-control`)

- [x] 3.1 Route `Runtime::toggle_mute` through `RelayRegistry` to every gate (input silence, output mute + flush, phrases ignored); verify tests that a muted gate sends silence frames and drops queued output
- [x] 3.2 Desktop: `tauri-plugin-global-shortcut`, permissions, register at start and on `hotkey` change, unregister the previous, status `{shortcut, registered, error}` through `Runtime::set_hotkey_status`; verify in the running app that a synthetic press toggles mute and that an empty setting registers nothing
- [x] 3.3 `/status` and the member panel show the gate state and the mute state; verify a live run where the hotkey, the panel button and the app button each toggle and all three views agree (the hotkey path was run live; the panel button and the app button need a person in the channel and the window, the user's step)

## 4. Close-out

- [x] 4.1 Live run in wake-word mode with the stub assistant: say the activation phrase, see the session start and the rising earcon; say the deactivation phrase, see the falling earcon and the stub told "not wanted"; verify the log sequence and the gate status (the user's step: needs a person speaking in the channel)

  Closed by the user's decision of 2026-11-10 at the merge into `main` (every remaining check counts as done); not verified separately as a task.
- [x] 4.2 Update `docs/ARCHITECTURE.md` 4.3 and 4.5 where the code differs, `AGENTS.md`, `LOCAL-TESTING.md`; verify no stale statement remains
- [x] 4.3 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass

## Workflow follow-up

- Checkpoint commit per task group on `change/session-gate-and-mute`.
- Merge into `main` with `--no-ff --log` and archive the change when the user says so.
