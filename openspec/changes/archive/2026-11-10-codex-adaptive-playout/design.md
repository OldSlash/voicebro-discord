# Design

## Context

See proposal.md for the measurements and the reasons. Today the Codex call's receive path
(`assistant/codex/jitter.rs`, `rtc.rs`) works like this:
- The remote track's task inserts each RTP packet into the `JitterBuffer`: a `BTreeMap` by
  extended sequence number, at most 25 packets.
- Songbird's mixer thread pulls one 20 ms frame through `PcmSource::read` →
  `FrameProvider::next_frame`.
- The buffer starts at a target of 3 packets, grows by one up to 8 after a late packet or a
  playout underrun, and relaxes after 750 stable frames.
- It decodes one packet per pull with `opus2`, decodes a lost packet from the next one's FEC
  or conceals it (at most 5 in a row), and drops late packets.
- When it runs dry it stops, and waits for the target number of packets before playing
  again.
- `arrival()` reports when the playing packet came from the network, for the turn latency
  line.

The rules that bind this change:
- "only necessary allocation after warm-up" on the receive path (`docs/ARCHITECTURE.md`,
  changed in commit `e18303c`);
- "No panics on the audio paths", with `panic = "abort"` in release, where any panic ends
  the process;
- a read on the mixer thread never blocks for more than a few ms.

## Goals / Non-Goals

**Goals:**
- the playback absorbs a server that is late in stalls and on average, with the depth and
  the stretching driven by measurement;
- every decision is visible in counters;
- the behaviour can be replayed and heard from recordings of real calls.

**Non-Goals:**
- matching NetEQ's full feature set (merge, comfort noise generation, DTMF, music
  detection);
- changing the packet store, the FEC and loss concealment, or the late-packet rule beyond
  what bridging needs;
- tuning for other engines.

## Decisions

**D1. A new file, `assistant/codex/playout.rs`, composed with the existing jitter buffer.**
- `jitter.rs` keeps the packet store, reordering, late drops, FEC and loss concealment, and
  decoding. It gains:
  - the arrival-based lateness per packet;
  - a "dry" state that bridges instead of stopping;
  - a way to report how much audio it holds.
- `playout.rs` owns:
  - the depth estimator (D2);
  - the per-pull decision (D3);
  - the time-scale DSP (D4);
  - the pause detection;
  - the new counters.
  
  It works on decoded 48 kHz mono samples between the decoder and the 20 ms frame.
- The pair stays behind `FrameProvider`, so `output.rs`, songbird and everything outside
  `assistant/codex/` are untouched.

Alternatives:
- the `neteq` crate (rejected, see the proposal);
- rewriting `jitter.rs` from scratch (its packet logic is tested and was not at fault);
- a fixed prebuffer or a user delay setting (a steady 5 % lateness drains a 300 ms cushion
  in about 6 s of speech, and every answer would start 300 ms later).

**D2. The depth comes from relative arrival delay, as WebRTC measures it.**
1. For each packet, its expected arrival is `arrival - (ts - ts_ref) / 48 kHz`.
2. Its delay is that value minus the smallest one among the packets of the last 2 s.
3. Delays go into a fixed histogram of 20 ms buckets (0 to 1000 ms) with exponential
   forgetting, so old stalls fade out over tens of seconds.
4. The depth is the 95th percentile plus 20 ms, clamped to 60–500 ms.

What this gives:
- A steady server keeps the depth at 60 ms.
- Stalls of 200 ms push it to about 220 ms.
- An average lateness of a few percent adds only its share over 2 s, about 100 ms at 5 %.
  The stretching (D3) absorbs the rest, so no rate is assumed anywhere.

The 2 s window, the forgetting, the 95th percentile and the bounds are tuning constants,
checked on recordings (task 4).

Alternatives: the current count of underruns (it reacts only after a hole has been heard),
or the mean inter-arrival time (it cannot tell one big stall from steady jitter).

**D2a. The start of a call (added after the first implementation).**

An empty estimator starts at 60 ms. So the very first stall of a call, before anything has
been learnt, is still heard: one hole of about 100 ms in the synthetic test. Two changes
remove it.
- **A starting prior.** A new estimator's histogram holds 30 % of its mass in the 160–180 ms
  bucket and 70 % at 0, which gives a depth of 200 ms. That mass fades like any other
  learnt value: on a steady server, 0.3 × 0.9983ⁿ drops under 5 % after about 1050 packets,
  roughly 21 s, and the depth is back at 60 ms. 200 ms of depth plus 60 ms of bridging
  cover a first stall of about 250 ms; the longest measured so far was 314 ms.
- **The next call starts from the last.** `CallReport` (one per guild, kept for the app's
  run) keeps the estimator's histogram when a call closes. The next call starts from it,
  with the reference and the 2 s window cleared. Only the first call after the app starts
  uses the prior.

The open line `[rtc] call opens with a voice buffer of N ms (…)` says which start was used.

Cost: the first answer of the first call starts about 140 ms later than with 60 ms. The baseline playout of the replay stand
(`adaptive: false`) starts empty, as before, so the comparisons stay as they were.

**D3. One decision per 20 ms pull.**

The level is the audio held: decoded samples waiting plus the RTP time span of the held
packets. Hysteresis is 20 ms below the depth and 60 ms above it.

- **Dry, in speech:** bridge with loss concealment (D5), at most 3 frames, then silence. A
  silence inside speech is one audible gap. Dry after a pause or at the end of an answer is
  plain silence with no counter, because the server sends no voice then.
- **Below depth − 20 ms:**
  - the next 20 ms is a pause: play it twice (cross-faded) and count 20 ms of pause
    stretch. At most 25 times (500 ms) without a new packet from the network: beyond that
    the stream has stopped rather than slowed, and the pause plays out so the buffer can run
    dry;
  - voiced, the level under half the depth, no pause usable for 200 ms, and the last voiced
    stretch 60 ms or more ago: insert one pitch period (D4) and count it as speech stretch;
  - otherwise play normally and wait for a pause.
- **Above depth + 60 ms:**
  - a pause longer than 60 ms: drop 20 ms of it;
  - voiced, the level over twice the depth, under the same timing rules: remove one pitch
    period;
  - count the removed time.
- **Otherwise:** play normally.

The half/twice rule was added during implementation. With only the 200 ms rule, the
synthetic stream's 1.7 s phrases got more voiced than pause stretching. With it, voiced
changes happen only when waiting for a pause would not do.

Pause detection compares the frame's RMS level with a slowly tracked speech level (about
−30 dB below it), so it follows quiet and loud voices. Pauses first, because a longer pause
is almost inaudible while a stretched vowel can be heard. That is also why voiced stretching
is rate-limited.

**D4. Time scaling by one pitch period (WSOLA), with the search on a decimated signal.**
- The pitch period is found by normalized cross-correlation on the signal decimated to
  4 kHz (lags 2.5–15 ms, 10–60 samples), then refined at 48 kHz within ±12 samples.
- A correlation under 0.5 means unvoiced: no voiced stretch for that frame.
- A periodic signal also matches at two or three periods, so the shortest lag whose
  correlation is within 0.05 of the best is taken.
- Inserting repeats one period. Removing skips one. Each joint is cross-faded over one period.
- The cost is about 30 thousand multiply-adds per stretch, tens of microseconds, against
  roughly 300 thousand on full-rate data in the `neteq` crate.
- Output stays in exact 20 ms frames through a carry buffer.

Alternative: plain resampling of the whole stream. It is simpler, but at 5–7 % it shifts the
pitch by about a semitone.

**D5. Bridging does not consume the sequence.**
- When dry in speech, the jitter buffer asks the decoder for concealment (`decode` with an
  empty payload) for up to 3 frames and leaves `next_seq` where it is.
- When the late packet arrives, it is decoded next; Opus smooths the transition.
- The bridged time is extra time like a stretch, and the shrink rule (D3) takes it back in
  later pauses if the level ends up above the depth.

This replaces today's "stop and refill to target". That refill doubled each hole: the time
waiting plus the time refilling.

**D6. Memory and safety.**
- **Memory:**
  - every buffer of `playout.rs` is allocated in `new()`: the histogram, the decoded-sample
    ring sized for the 500 ms ceiling plus a 120 ms packet and a pitch period, the carry
    buffer and the decimated scratch;
  - the packet store keeps its `BTreeMap`, whose node allocations are the necessary ones
    under the updated rule;
  - its cap rises from 25 to 50 packets (1 s) to hold the 500 ms ceiling plus a burst.
- **Safety:**
  - no `unwrap`, `expect` or unchecked slice indexing in the decision and DSP code: lengths
    are clamped with `min`, and slices are taken with `get`;
  - a decoder error is a counted decode error followed by concealment;
  - the existing `lock().expect` stays: a poisoned lock needs a panic, and a panic already
    aborts the release process.

**D7. The turn latency keeps working.**
- `arrival()` keeps reporting the arrival of the packet whose samples are playing.
- Stretched and bridged frames keep the last value.
- "ChatGPT audio -> Discord relay handoff" now includes the depth, which is the honest number
  for how long the answer waited.
- The interruption flush (`reset`) clears the playout too, so a barge-in does not hear the
  buffered rest.

**D8. A replay stand built from real calls.**
- **Recording:** the live call test, under `VOICEBRO_CODEX_RECORD=<file>`, writes one line
  per packet: arrival offset in microseconds, sequence, timestamp and the payload in hex. The
  hook is `#[cfg(test)]` in `jitter.rs`, so release builds have none. Recordings stay outside
  the repository (they hold the assistant's synthetic voice answering test questions; about
  1 MB per 100 s).
- **Replay:** a gated test (`VOICEBRO_PLAYOUT_REPLAY=<file>[;<file>]`) runs each recording on
  a simulated clock at full speed. It inserts packets whose arrival time has passed and pulls
  one frame per simulated 20 ms. The playout reads no wall clock, because one pull is 20 ms.
  - It prints the counters and the delay each packet waited (p50, p95, max).
  - It writes `<file>.wav` for listening, and with `VOICEBRO_PLAYOUT_BASELINE=1` also a WAV
    of the same recording with stretching and bridging off, for comparison.
- **Synthetic streams** (a voiced test signal with pauses, 5 % slower, plus 200 ms stalls)
  cover the same rules in ordinary unit tests without a recording.

**D9. Counters and display.**
- `CallAudioCounters` gains:
  - `stretched_pause_ms`, `stretched_speech_ms`, `removed_ms`;
  - `bridged_frames`, `speech_gaps`;
  - `depth_ms`, `depth_max_ms`.
- They appear in the `[rtc] call audio …` line, in `CallStatus.audio` and as Performance tab
  stat cards (`main.js`, the `ui-dev` mock).
- `concealed` keeps counting lost packets only, so the quality verdict ("degraded" over 1 %
  concealed) is not triggered by stalls.

## Risks / Trade-offs

- [Stretching is audible on some voices] → Mitigations:
  - pauses first, voiced stretching rate-limited and only when correlation shows a clear
    period;
  - the WAVs from the replay stand are listened to before a build is handed over;
  - a test-only switch turns off voiced stretching, if pauses alone prove enough.
- [Pause detection misfires on background noise or breathing] → the threshold is relative
  to the tracked speech level; tuned on the recordings.
- [Answers start later when the server is uneven] → Mitigations:
  - the depth is bounded at 500 ms and shrinks in pauses;
  - the turn latency line shows the cost;
  - a steady server keeps today's 60 ms.
- [Concealment longer than 60 ms sounds artificial] → bridging stops at 3 frames, and longer
  holes are honest silence counted as audible gaps.
- [Recordings capture one day's server behaviour] → several recordings (idle answers and a
  delegated retelling), plus synthetic stalls of larger size than seen so far.
- [More buffered audio is lost on barge-in] → it was never played, and the flush on
  interruption already drops it.

## Migration Plan

Nothing to migrate: no settings and no stored data change. Rollback is reverting the
change's commits. The first build carries the build stamp. The user's check in a real
channel with several people and a game decides acceptance.

## Open Questions

- The exact pause threshold, the forgetting factor and the 2 s window are tuned on the
  recordings in task 4. They change constants, not the approach.
