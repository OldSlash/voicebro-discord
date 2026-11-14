# Proposal

## Why

While the bot is in a voice channel, Discord shows it as speaking all the time: the green
ring never goes out, even when the assistant says nothing. The output track sends one Opus
packet every 20 ms from the join to the leave. When there is nothing to play, that packet is
encoded silence, because the source plays silence so that an underrun never ends the track
(`Output source lifetime`). The user asked that the bot send nothing toward Discord while the
assistant (the Codex voice) has nothing to say, so that Discord shows it silent. This applies
only to the bot's own voice toward Discord. What the bot hears from the channel and sends to
the assistant does not change.

## What Changes

- The bot sends audio toward Discord only while there is something to hear: the assistant's
  voice or an earcon. A frame counts as voice at the same peak the idle timeout already uses
  for the assistant's audio (256). After the last such frame and a short hangover
  (500 ms), songbird sends Discord's five silence frames and then sends no packets at all.
  The first audible frame turns sending back on.
- The output keeps running as before. songbird's mixer still pulls one frame every 20 ms,
  so the call's jitter buffer, the adaptive playout, the gains, the mute, the earcons and
  the latency lines keep their clock. Only sending stops: the driver is muted through
  `songbird::Driver::mute`, which processes audio but submits none. `Call::mute` is never
  used for this, because it also sets the bot's self-mute in Discord's voice state and
  Discord would show the bot as muted.
- The start of an answer is never cut. The call's jitter buffer has up to 100 ms of decoded
  audio ahead of the frame being played, and the output turns sending on as soon as that
  audio holds voice, before the voice plays. An earcon that has to turn sending on waits
  two frames (40 ms) before it plays.
- The loss measurement and the Opus FEC (change `discord-voice-delivery`) are not touched:
  every frame is encoded as before, and the muted driver discards what it does not send.
- New counters in the channel status and the leave log line: how many times sending turned
  on, how long nothing was sent, and voiced frames played before sending was back on (a cut
  start; expected to stay 0).
- `docs/ARCHITECTURE.md` (4.1, 4.2, 5, 6) and `AGENTS.md` (audio facts) describe this.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `audio-transport`:
  - A new requirement "The bot sends audio only while there is something to hear": sending
    stops after the hangover, resumes before the first audible frame plays, and the bot's
    voice state in Discord does not change.
  - "Output source lifetime": the source still plays silence when it has nothing and only
    a leave ends the track. That silence is no longer sent to Discord.
  - "Delivery counters": the new counters.

## Impact

- Code: `crates/voicebro-core/src/audio/output.rs` (the sending decision in `PcmSource`, the
  voice-ahead query on `FrameProvider`, the counters, a watch channel for the decision),
  `assistant/codex/jitter.rs` and `playout.rs` (voice ahead in the decoded samples),
  `discord/session.rs` (driver mute at the join, a task per join that applies the
  decision, stopped at the leave), `discord/delivery.rs` (the leave line).
- No new dependency (tokio's `watch` is already in the workspace), no setting, no UI change.
  The new counters appear in the status JSON only.
- Behavior seen by people in the channel: the bot's ring lights only while it speaks or
  plays an earcon. The voice itself, its latency and its FEC stay as they are.
- The user checks the result in Discord after the implementation: that the ring goes out
  once packets stop (songbird keeps its speaking flag set while the track stays), and that
  the reported loss shows no jump right after an answer starts (songbird's RTP sequence
  numbers move on while it sends nothing).
