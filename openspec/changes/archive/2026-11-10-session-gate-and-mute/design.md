# Design

## Context

See proposal.md. Target: `docs/ARCHITECTURE.md` 4.3 and 4.5 (the trait half). The gate's
constants: pre-roll 250 frames, replay cap 300, lookback 1 s + 80 ms, peak 700, utterance
gap 700 ms, concurrent window 400 ms. The audio ports, the receive path's `TickSink`, the
wake engine's `PhraseSink` and the hotkey probe exist. `Runtime::toggle_mute` exists and
publishes `MuteChanged`.

## Goals / Non-Goals

**Goals:** the session rules on the audio ports; the assistant interface the Codex change
implements; mute from all three sources; everything testable with a stub assistant.

**Non-Goals:** no assistant implementation; no change to the detection rules.

## Decisions

**D1. `gate.rs` with its tests, over `AudioPorts` only.** The gate sends frames with
`FrameSender::send(frame)`, reads the assistant's last signal from
`OutputHandle::last_signal()`, mutes with `OutputHandle::set_muted` plus `flush`, takes
`Earcon` from `audio::earcon` and publishes through `EventBus`.

**D2. The gate is the `TickSink` and the `PhraseSink`.** The receive path calls
`gate.process_tick(mixed, per_speaker)`; the wake worker calls `gate.on_phrase(kind, phrase,
ssrc, since_end)` through the registry. The gate forwards per-speaker frames to `WakeFeed`
only in wake-word mode when not muted.

**D3. `AssistantSession` trait as in the architecture** (`session_wanted`, `ready`,
`call_context`, `speakers_changed`); the gate holds a `Weak<dyn AssistantSession>`.
`StubAssistant` becomes ready after a configurable delay and records frames and calls; it
is also what runs when the assistant engine setting is unknown, so the app never panics.

**D4. Mute lives in `RelayRegistry`, `Runtime::toggle_mute` is the only entry.** The
registry pushes the flag into every gate: input silence, output mute + flush. `/status` and
the panel read `Runtime::is_muted()`.

**D5. Hotkey in the desktop shell.** `tauri-plugin-global-shortcut` registered in `setup` and
re-registered from the `SettingsChanged` event when the `hotkey` section changed; a failed
registration is written to `Runtime::set_hotkey_status`. Permissions
`global-shortcut:allow-register`, `allow-unregister`, `allow-is-registered` in the
capability file. The probe showed the plugin fires on synthetic presses; the game test is
the user's.

**D6. Session end on replay timeout.** If the assistant is not ready when the replay cap is
reached, the gate drops the held audio, logs it, ends the session and sets a connection
error "assistant did not become ready"; nothing waits forever.

## Risks / Trade-offs

- [Replay hold makes the first answer late by the connect time (~1–2 s)] → inherent; the
  rising earcon tells the user the session started.
- [Hotkey eaten by a game or anti-cheat] → the panel and the app button remain; a raw
  `RegisterHotKey` fallback is the recorded option.
- [Utterance timing without recognizer word times] → the 1 s + 80 ms lookback.

## Open Questions

None.
