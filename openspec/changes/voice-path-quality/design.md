# Design

## Context

See proposal.md and `docs/NEXT-CHANGES.md` section 1 for the evidence. The relevant code
today (`crates/voicebro-core/src/assistant/codex/rtc.rs`):

- Send: the gate pushes 20 ms stereo frames into a drop-oldest channel of 4 frames from
  songbird's tick, which runs on a tokio task (p99 interval about 32 ms on Windows). A
  dedicated thread wakes every 20 ms, drains the channel, keeps only the newest frame,
  encodes it and writes one sample; on an empty channel it writes nothing.
- Receive: `read_remote_track` decodes each RTP packet as it arrives and pushes 20 ms frames
  into `OutputHandle` (8 frames, drop-oldest), which songbird reads through `PcmSource`
  with its own 3 to 8 frame jitter target.
- The first version did neither by hand: an AudioWorklet FIFO played every frame in order
  into the browser's WebRTC stack, and the browser's NetEQ handled jitter, reordering and
  loss before the audio came back as a steady stream.
- The rewrite spec (`REWRITE-SPEC.md` 4.4) lists `approvalPolicy: never`; the first
  version's working code uses `untrusted` (`codex.rs` line 1906). The agent searched the
  web 48 times in 99 tasks there and never here.

## Goals / Non-Goals

**Goals:** no lost or reordered audio between the gate and the call on a clean network;
steady playout of the answer under jitter and small loss; counters that show which of the
two directions misbehaves; the first version's agent behaviour.

**Non-Goals:** echo cancellation or noise suppression (the first version turned the
browser's off too); changing the gate, the wake engine or the session rules; gains
(change `audio-gain-settings`).

## Decisions

**D1. A FIFO with a small prefill on the send side.** The send thread keeps its 20 ms
clock and sends exactly one frame per tick: the oldest waiting frame, or silence when none
waits. After the call connects and after every underrun it waits until two frames are
queued (40 ms) before sending real audio again, so a late gate tick does not cause an
underrun on the next send tick. The channel toward the call holds at most 5 frames
(100 ms); a push beyond that drops the oldest and counts it. The scheduling decision is a
pure function (queue depth, prefill state → frame or silence) so it is unit-tested with a
simulated jittery producer. *Alternative:* send from the gate's tick directly. Rejected:
the tick jitters, and the RTP timing would carry that jitter into the remote jitter buffer.

**D2. The receive jitter buffer is pulled by the output on songbird's mixer clock.**
Incoming RTP packets are stored by sequence number. `PcmSource`, which songbird reads every
20 ms on its own dedicated thread, asks the call's jitter buffer for the next frame when the
call is the active producer: the buffer starts playing once it holds the target depth
(3 frames, growing by one up to 8 after a late arrival or a playout underrun, relaxing after
a long stable run, the same policy as the existing output queue; growth on underruns was
added after the live call on 2026-11-09 showed 4 underruns in one answer with growth on late
arrivals only, and 2 with it), returns the next packet decoded, and for a missing
sequence number decodes the following packet with `fec = true` if it is buffered, else runs
the decoder's concealment (empty input). A gap in sequence numbers is loss; a pause with
contiguous sequence numbers is not (the server may stop sending in silence) and plays
silence without concealment; concealment is limited to 5 frames in a row. *Alternative:* a
separate playout thread pushing into the existing output queue. Rejected: a second clock and
a second buffer stacked on the first, adding 60 to 160 ms of latency. Before writing the
buffer, check `rtc::media::io::sample_builder` in the `rtc` 0.21 source; reuse it for
reordering only if it gives sequence numbers and does not hold frames back longer than the
target depth.

*Checked on 2026-11-09 (`rtc-media` 0.21.1, `src/io/sample_builder/mod.rs`):* own code.
`SampleBuilder` is push-driven (a sample is released only after `max_late` later packets
arrived, not on a clock), drops lost packets without handing their neighbours to the caller
(no way to decode FEC from the next packet), and allocates two 65 536-entry vectors per
stream. The buffer in `assistant/codex/jitter.rs` keeps packets in a `BTreeMap` by
extended sequence number instead.

**D3. Mute, flush and the stub still work through the same output.** A flush (mute,
deactivation, session not wanted) also clears the jitter buffer and resets its target.
Frames pushed with `OutputHandle::push` (the stub assistant, tests) keep their current path;
earcons stay mixed in at the source.

**D4. Encoder settings.** Mono VOIP, complexity 10, 32 kbit/s, in-band FEC on, expected loss
5 %. The receive-path benchmark (`receive_path_cost`, 239 µs per tick at complexity 5) is
re-run to confirm the cost stays far below the 20 ms budget. The bitrate is compared at 24,
32 and 48 kbit/s on the live call test by transcript fidelity; the lowest that matches the
spoken text is kept.

**D5. Approval policy and tools as in the first version.** `thread/start` sends
`approvalPolicy: "untrusted"`; the declines for client-side requests stay as they are. Web
search is a server-side tool of the agent and is not touched. The live call test gets a
delegated question about current facts and checks the chat log for a `webSearch` item.

**D6. Voice "server default".** An empty `codex.voice` is valid and means "send no voice";
the Settings voice list gets a "(server default)" entry; the log line at call start names
the voice used or "server default". New settings files start with the server default (what
the first version effectively used, since it omitted its configured `cedar`); existing
settings keep their voice.

**D7. Log noise.** The log plugin sets `songbird::driver::tasks::udp_rx` to Error: its
`RTCP decryption failed` warnings (about 700 in 2.5 minutes) carry no action. The Opus
decode errors in the first second after joining come from `ssrc_state` at Error and stay
visible.

**D8. Counters.** `CallStatus` gains the counters of the spec; `CodexSession` logs them in
one line when a call ends. Change `performance-statistics` shows them.

## Risks / Trade-offs

- [The server sends packets of other durations than 20 ms] → the buffer decodes whatever a
  packet holds and slices it into 20 ms frames; the test covers 10 ms and 40 ms packets.
- [Pulling decode onto songbird's mixer thread] → one Opus decode is tens of microseconds;
  the benchmark is extended with a decode per tick.
- [Concealment on a stream that paused on purpose] → only sequence gaps are concealed, and
  never more than 5 frames in a row.
- [`untrusted` lets the agent run harmless read-only commands without asking] → the shell
  tool stays disabled (`--disable shell_tool`) and the sandbox stays read-only, as in the
  first version.

## Migration Plan

No settings migration. Rollback is reverting the branch; the first version stays available
as the reference.
