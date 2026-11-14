# Design

## Context

See proposal.md (Why). The facts below were checked on 2026-11-14 in the songbird 0.6.1
source in the cargo registry and in seven recorded Codex calls from 2026-11-10 to 11-12
(`*.rec` and their replayed `*.rec.wav`, kept outside the repository).

songbird's mixer, per 20 ms tick (`driver/scheduler/live.rs`, `driver/tasks/mixer`):

- Mixer messages, `SetMute` among them, are handled at the start of the tick, before
  mixing. Every playing track is then pulled: our `OpusOutput::next_packet` runs every
  tick, muted or not.
- `mix_and_build_packet`: when the driver is muted the mix counts as empty. songbird then
  sends five `F8 FF FE` silence frames and after that sends nothing (`Ok(0)`). The first
  tick that is not muted sends our packet again and re-arms the five frames.
- A mixer is demoted to idle only when its track list is empty. With our track attached it
  stays live while muted, so the pull clock never stops.
- `advance_rtp_counters` moves the RTP sequence number and timestamp on every tick,
  whether a packet was sent or not.
- `Driver::mute(bool)` stores the flag and sends `CoreMessage::Mute`, which the core task
  forwards to the mixer as `SetMute`. A driver restart applies the stored flag again, and
  reconnects keep the mixer. `Call::mute` does the same and also sends a gateway voice
  state update with `self_mute`, so Discord would show the bot muted.
- The speaking flag (op 5) is set when the live mixer gets its websocket and cleared only
  on demotion. There is no public API to change it.
- A track cannot skip a tick. Any error from `next_packet` ends the track, and an empty
  packet counts as a passthrough strike that eventually blocks passthrough.

The Codex call's audio:

- Codex sends packets about every 20 ms for the whole call, between answers too (for
  example 2,397 packets in 48.8 s, longest gap 139 ms). Packet arrival therefore says
  nothing about voice. Packet size does not either: one call has only 136 packets of 3
  bytes or less, and the rest are 61 bytes or more, in pauses as well.
- Decoded, the audio between answers is digital zero or has a peak of about 1 (median),
  and no quiet frame reaches 256. Quiet runs inside answers last at most 11 to 15 frames
  (300 ms). The runs between answers are over 50 frames.
- The playout keeps up to `LOOKAHEAD` (5 frames, 100 ms) decoded ahead of the frame being
  played (`Playout::wants_more`). The rest of the depth waits as packets.

The pushed path (the stub assistant and the tests) primes to three frames before it plays.

## Goals / Non-Goals

**Goals:**

- No packet goes to Discord while nothing is audible, and no audible sample is lost when
  sending resumes.
- The voice path is unchanged: playout, jitter buffer, gains, mute, earcons, latency lines,
  the encoder, the loss measurement and the FEC. None of their code changes.

**Non-Goals:**

- Clearing songbird's speaking flag (op 5); see Risks.
- Changing what the bot sends to the assistant.
- A setting for this. The behavior is always on, and the hangover and the lead are
  constants.

## Decisions

**D1. Stop sending with the driver mute and keep the one track.** songbird already does
the right thing for a muted driver: it keeps pulling, sends five silence frames and goes
quiet. We call `songbird::Driver::mute` explicitly, through `Call`'s `DerefMut`, and never
`Call::mute`. The alternatives each fail for a reason:
- Pausing the track stops the pulls, so the playout's clock and the earcons stall, and
  resuming would need a second clock.
- Removing the track and adding a new one per answer makes songbird clear the speaking
  flag. But the source is moved into songbird, every answer would wait for input readying,
  and it reverses "one output track per join".
- `Call::mute` shows the bot muted.
- Silence cannot be signalled from the format reader, because an error ends the track.
- A volume of 0 still mixes and sends.

**D2. The decision is made in `PcmSource`, on the mixer's clock.** Each frame it works
out two things:
- *Audible now*: the frame it plays carries the assistant's audio at a peak of
  `SIGNAL_PEAK` (256) or more, measured before the gain as `last_signal` is, or an earcon
  is mixed into it.
- *Audible ahead*: the provider reports voice ahead (D3), a queued pushed frame of the
  current epoch is at `SIGNAL_PEAK` or more (the peak is stored at push), or earcon
  samples are waiting.

The assistant's frames are discarded while output is muted, so then only earcons count.
The state moves Silent → Opening → Sending → Silent:
- Silent: anything audible, now or ahead, asks for sending and enters Opening for
  `SEND_LEAD` = 2 frames. Earcon samples stay queued, not mixed, until Opening is over.
  The assistant's frames are never held, because they are on the playout's clock. A voiced
  frame played while Silent or Opening counts as a cut start.
- Sending: anything audible, now or ahead, resets the hangover. After `SEND_HANGOVER` =
  25 quiet frames (500 ms) it asks for silence and enters Silent.

The output starts Silent. Why 500 ms: the recordings have no pause inside an answer over
300 ms, and the ring goes out about 0.6 s after an answer (the hangover plus songbird's
five frames).

**D3. Voice ahead from the call.** `FrameProvider` gets `fn voice_ahead(&self) -> bool`,
which is `false` by default. `JitterBuffer` computes it at the end of each pull, under the
same lock: the peak of the playout's decoded future (`Playout::future_peak`, at most about
10,000 samples) against `SIGNAL_PEAK`. It stores the result in its state, and
`voice_ahead` reads it. With the playout full this gives a lead of up to four frames. At
the 60 ms minimum depth it still gives at least two. Two other ways were rejected:
packet size as a hint (see Context) and a delay line in the output (latency on every
answer).

**D4. Applying the decision.** The output's state holds a `tokio::sync::watch::Sender<bool>`
("sending"). `PcmSource` calls `send_replace` on transitions only. That never blocks and
needs no runtime on the mixer thread. `OutputHandle::sending()` gives a receiver.
`join_voice` mutes the driver before `play_only_input`, so nothing is sent before the
first audible frame. It then spawns one task per join that holds the call and the
receiver. On each change the task locks the call and calls `Driver::mute(!sending)`. Its
loop is a function over the receiver and an apply callback, so a test can drive it without
songbird. The `JoinHandle` goes into `VoiceSession`, and the leave aborts it before
`manager.remove`, so the call does not outlive the leave. The task also ends when the
source is dropped. The latency is one tokio wake, the core task's forward and the next
tick start: one tick normally, which is why the lead is two frames. The `RawAdapter`
fallback reads the same `PcmSource`, so it behaves the same way. The `discord-delivery`
probe has no such task, its driver is never muted, and its tone keeps it Sending.

**D5. The encoder and the loss path stay as they are.** `OpusOutput` encodes every frame
it pulls, as before. While the driver is muted songbird drops the packet. Nothing in
`opus_out.rs` or `delivery.rs` reads the sending state, except the leave line (D6). For
listeners the pause is harmless. songbird's ticks follow a 20 ms deadline and the RTP
timestamp moves 960 per tick, sent or not. So the first packet after a pause arrives on
time against its timestamp: neither a listener's jitter estimate nor the jitter in
Discord's reports changes. The skipped sequence numbers lie in the past, and nobody waits
for them. A cost is accepted: `packets_encoded` also counts the packets the muted driver
dropped. `frames_not_sent` sits next to it.

**D6. Counters.** `OutputCounters` gains:
- `sending` (bool), the state now;
- `sends_started`, counted at each Silent → Opening;
- `frames_not_sent`, the frames pulled while Silent;
- `cut_starts`.

They are in the channel status (`VoiceChannelStatus.output`) already, and no UI reads them.
`delivery::leave_line` also takes the output counters. It adds the starts, the time not
sent (frames × 20 ms, in seconds) and the cut starts.

## Risks / Trade-offs

- [Discord's ring may follow songbird's speaking flag rather than the packets, and the
  flag stays set while the track lives] → The "always speaking" seen today comes from
  packets of encoded digital silence. That points to the client lighting the ring from
  the incoming stream, not from loudness. The user's Discord test settles it: if the ring
  stays lit while `frames_not_sent` grows, the fallback is the track-per-answer variant
  of D1. That is a change of its own, because it reverses "one output track per join".
- [Discord's server counts the skipped sequence numbers as lost in its reports on our
  stream] → It would only change our own measured loss, not anything Discord plays.
  Whether it happens depends on Discord's server: an RFC 3550 receiver counts a jump below
  3,000 as loss. The user's Discord test shows it as a jump in the reported loss right
  after an answer starts. If it happens, the fix is at the source, in a change of its own:
  songbird moves the sequence number only for packets it sends, as RFC 3550 asks.
  Filtering the reports is not the fix.
- [The lead falls short: minimum depth, a priming restart, or a slow tokio wake] → Watched
  by `cut_starts`. A replay test plays the recorded calls through the jitter buffer and
  `PcmSource` with the mute applied one tick late and requires zero cut starts. Both
  constants can be raised.
- [A pause inside an answer longer than 500 ms makes the ring blink] → Accepted. None
  was seen in the recordings, and no audio is lost because the voice ahead reopens
  sending in time.
- [An earcon from silence starts 40 ms late] → Accepted.
- [The RTP timestamp jumps when sending resumes, so the listener's jitter buffer starts
  over] → The same happens for every bot that stops sending and for push-to-talk users.
  songbird sends Discord's five silence frames first, as Discord asks.
- [An output gain of 0 % still sends the inaudible voice] → Accepted. The decision uses
  the same pre-gain peak as the idle timeout.

## Migration Plan

Nothing to migrate: no setting, no stored data. Reverting the change restores constant
sending.
