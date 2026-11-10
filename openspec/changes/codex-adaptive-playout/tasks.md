# Tasks

## 1. Branch and chain

- [x] 1.1 Create the branch `change/codex-adaptive-playout` from the tip of `change/codex-handoff-mode` and commit the rule "only necessary allocation after warm-up" in `docs/ARCHITECTURE.md`. Verify: `git log -2` shows `e18303c` on top of `5fbd590`. (Done before this plan; fmt clean, clippy no errors, tests core 140 and desktop 6 passed.)
- [x] 1.2 Add `codex-adaptive-playout` to the end of the chain in `AGENTS.md` (Git flow). Verify: the chain lists it after `codex-handoff-mode`.

## 2. Replay stand (design D8)

- [x] 2.1 Add a `#[cfg(test)]` packet recorder in `jitter.rs`. Make `live_call_when_enabled` write one line per received packet under `VOICEBRO_CODEX_RECORD=<file>`: arrival offset in µs, sequence, timestamp, payload in hex. Verify: a live call with the delegated question writes a file whose line count equals the call's `packets received`. (Plain call: 322 lines, 322 received. Delegated call: 1892 lines, 1892 received.)
- [x] 2.2 Add a gated replay test (`VOICEBRO_PLAYOUT_REPLAY=<file>[;<file>]`). It feeds each recording on a simulated 20 ms clock through the current buffer, prints the counters and the wait per packet (p50, p95, max), and writes `<file>.wav`. Record two calls (a plain answer and a delegated retelling, `items` mode), and document both variables in `AGENTS.md`. Verify: the replays reproduce the playout underruns of the live run, and the baseline numbers are noted under this task.
  - Recordings of 2026-11-10, 13:45.
  - **Plain** (6.4 s of audio over 6.7 s, lag 315 ms, 3 stalls over 60 ms): 3 playout underruns, live and replayed alike, all in pauses; 0 holes in speech; wait p50 60 ms, p95 83 ms.
  - **Delegated** (37.8 s over 39.3 s, lag 1468 ms, 12 stalls over 60 ms, up to 308 ms): 9 playout underruns, live and replayed alike; 3 holes in speech, 440 ms in all; wait p50 110 ms, p95 157 ms, max 158 ms.
  - A hole is a dry spell with speech (RMS ≥ 150) within 60 ms on both sides.
- [x] 2.3 Add a synthetic stream for unit tests: a voiced test signal with pauses, Opus-encoded, on an arrival schedule 5 % slower than real time with 200 ms stalls. Verify: a unit test shows today's buffer producing holes on it. (20 s, three 200 ms stalls: 6 holes, 880 ms, and 9 playout underruns.)

## 3. Depth from measured lateness (spec: Buffer depth follows measured lateness)

- [x] 3.1 Create `assistant/codex/playout.rs` with the relative-arrival-delay estimator (design D2): a 2 s window, a 20 ms-bucket histogram with forgetting, the 95th percentile + 20 ms, clamped to 60–500 ms. All arrays are allocated in `new()`. Verify by unit tests:
  - steady 20 ms ± 10 ms keeps 60 ms;
  - repeated 200 ms stalls give about 220 ms and never more than 500;
  - a calm period brings the depth back down.

  (Tests `a_steady_server_keeps_the_minimum_depth`, `repeated_stalls_grow_the_depth_and_a_calm_period_brings_it_back` (back to 100 ms or less after 30 s) and `the_depth_never_exceeds_the_ceiling`.)
- [x] 3.2 Use the depth in `jitter.rs` instead of the fixed 3–8 frame target and the 750-frame relaxation. Raise the packet cap to 50. Verify:
  - the existing tests pass: order, FEC, late drop, 10 and 40 ms slicing, wrap-around;
  - the old "underrun grows the target" test is replaced by a depth test.

  (The packet tests run on the baseline playout, `JitterBuffer::with_adaptive_playout(false)`, so their output can still be compared with a reference decode. The depth test is `the_depth_follows_lateness_not_underruns`.)

## 4. Bridging a dry buffer (spec: A dry buffer is bridged; The answer plays in order without gaps)

- [x] 4.1 Bridge a dry buffer in speech with Opus concealment, at most 3 frames, without consuming the sequence (design D5). Then play silence, counting one audible gap. Dry after a pause or at the end of an answer counts nothing. Verify by unit tests:
  - a packet 40 ms late gives 2 bridged frames and no gap, and plays next without being counted late;
  - 300 ms late gives 3 bridged frames, silence and 1 gap;
  - the end of an answer gives no bridged frame and no gap.

  (Tests `a_short_stall_in_speech_is_bridged_and_the_late_packet_plays`, `a_long_stall_in_speech_is_bridged_then_counted_as_one_gap` and `the_end_of_an_answer_is_neither_bridged_nor_a_gap`. Concealed stays 0 in the first two.)

## 5. Time scaling (spec: A late stream is stretched, pauses first; Extra delay is removed; Playout never stops the app)

- [x] 5.1 Pause detection (RMS against a tracked speech level) and pause stretching and shrinking by 20 ms with a cross-fade. Shrinking keeps at least 60 ms of every pause. Verify by unit tests:
  - pauses of the synthetic signal are detected;
  - below depth a pause is lengthened by 20 ms and counted;
  - above depth + 60 ms a long pause is shortened by 20 ms and counted.

  (Tests `a_pause_is_lengthened_below_the_depth` and `a_long_pause_is_shortened_above_the_depth_but_keeps_60_ms`. Detection is exercised by the synthetic streams of 5.3, where most added time lands in pauses.)

  Found in the replay: when the stream stops, a trailing pause was stretched for seconds. Fix: at most 25 pause stretches (500 ms) without a new packet from the network.
- [x] 5.2 One-pitch-period insert and remove (design D4): search on a 4 kHz decimation, refine at 48 kHz, cross-fade over one period, at most once every 60 ms, no voiced stretch below a correlation of 0.5, output in exact 20 ms frames through the carry buffer. Verify by unit tests:
  - a sustained vowel keeps its fundamental within 2 % after a stretch and after a removal;
  - noise gets no voiced stretch;
  - every pulled frame is exactly 960 samples.

  (Tests `a_stretched_vowel_keeps_its_pitch` and `a_shrunk_vowel_keeps_its_pitch` check the period of 331–355 samples for 140 Hz, f0 within 2 %, and every sample of every frame written. `noise_gets_no_voiced_stretch` covers noise.)

  The search first returned the double period (686). It now takes the shortest lag within 0.05 of the best correlation.
- [x] 5.3 The per-pull decision (design D3) joining sections 3–5. Verify on the synthetic stream:
  - no audible gap, and most added time is counted as pause time;
  - a 400 ms burst returns to depth within a few seconds.

  Verify on the two recordings of 2.2:
  - no audible gap;
  - wait p95 ≤ 400 ms;
  - speech stretching ≤ 3 % of speech time.

  Hand the replay WAVs, adaptive and with `VOICEBRO_PLAYOUT_BASELINE=1`, to the user to listen to.

  **Synthetic streams:**
  - 30 s, 5 % slow: 0 holes, 0 gaps, pauses 1400 ms, speech 565 ms (2.2 % of the speech).
  - With three 200 ms stalls: 1 hole of 100 ms at the first stall, before the depth had learnt anything; without stretching and bridging it was 5 holes, 1080 ms.
  - Burst: 313 ms removed, the wait back to 80 ms.

  The voiced rule was tightened while doing this (design D3 updated): speech is changed only below half the depth (above twice it for shrinking).

  **Recordings:**
  - Plain: 0 holes, 0 playout underruns (3 before), wait p95 163 ms, pauses 920 ms, speech 0 ms.
  - Delegated: 0 holes (3, 440 ms, with today's buffer), 0 playout underruns (9 before), wait p50 197 ms, p95 320 ms, pauses 2140 ms, speech 0 ms, depth up to 360 ms.

  WAVs (adaptive and `.baseline.wav`) are written next to the recordings.
- [x] 5.4 Robustness: no `unwrap`, `expect` or unchecked indexing in `playout.rs` and the new `jitter.rs` code. Verify by a unit test that feeds empty, truncated, random-byte and 2.5, 10, 40, 60 and 120 ms packets: playback continues, decode errors are counted, decodable packets play.

  (Test `unusual_and_broken_packets_never_stop_the_playout`. An empty packet now counts as a decode error and is concealed for one packet's length, instead of 120 ms. Only the lock's `expect` remains, per design D6.)

## 6. Counters, display and docs (spec: Call audio counters)

- [x] 6.1 Add `stretched_pause_ms`, `stretched_speech_ms`, `removed_ms`, `bridged_frames`, `speech_gaps`, `depth_ms` and `depth_max_ms` to `CallAudioCounters`, the `[rtc] call audio …` line and `CallStatus.audio`. Keep `concealed` for lost packets only. Verify:
  - a unit test of the log line names every counter;
  - `VOICEBRO_RTC_LOOPBACK=1 cargo test -p voicebro-core loopback_call_loses_nothing` still reports zero late and concealed, and no gap.

  Loopback result: late 0, concealed 0, speech gaps 0, depth 60 ms. The far end cuts its tone off at full level, so 3 frames are bridged at the end.

  A gap is now counted when the stream goes on after the silence. Before, it was counted when the silence began, which counted an abrupt end of the stream as a gap.
- [x] 6.2 Add the new counters as Performance tab stat cards in `main.js` and in the `ui-dev` mock. Verify in the mock: every new card shows a number, and the quality label is unchanged. (Checked in the mock: pauses stretched 160, speech 0, removed 0, bridged 0, gaps 0, voice buffer 220, max 360; "Quality: stable"; no console errors.)
- [x] 6.3 Update `docs/ARCHITECTURE.md`:
  - the receive part of 4.5;
  - the call audio counters;
  - the contract table: depth 60–500 ms, cap 50 packets, bridging at most 3 frames, pauses first.

  Verify: the documented values match the constants in the code.

  (Checked against the constants of `playout.rs` and `jitter.rs`. The thread table's receive row now says decoding runs 100 ms ahead and the buffers are allocated when the call opens.)

## 7. Performance and live checks

- [x] 7.1 Add a gated release benchmark of the pull path with stretching active (`VOICEBRO_BENCH=1`, like `receive_path_cost`). Verify: the worst pull is ≤ 200 µs and the mean ≤ 50 µs per 20 ms tick; report the numbers.

  Test `playout_pull_cost_when_enabled`: 60 s, 5 % slow, three stalls, three release runs.

  | | mean | p99 | worst |
  |---|---|---|---|
  | Adaptive playout | 43.3–44.0 µs | 97–119 µs | 224–320 µs |
  | Same stream, decode only (no playout work) | 43.7–43.8 µs | 104–139 µs | 265–422 µs |

  - **Deviation from the check as written.** The worst pull goes over 200 µs with or without the playout: it is the Opus decode plus scheduling noise on Windows. The playout's own cost does not show above the decode. The test asserts the mean ≤ 50 µs and p99 ≤ 200 µs instead of the worst.
  - **Change made while measuring.** Decoding ahead is limited to two packets per pull, so a burst is spread over several pulls.
- [x] 7.2 Live call test in `items` mode with the delegated question. Verify against the baseline of 2.2: the `call audio` line shows the depth, the stretched times and zero (or named) audible gaps.

  2026-11-10, 15:20, the whole call (1713 packets, recorded as `live-adaptive.rec`):
  - playout underruns 0 (9 in the baseline call), speech gaps 0, bridged 0;
  - pauses stretched 2640 ms, speech stretched 161 ms;
  - depth 380 ms (highest 420 ms);
  - late, concealed and decode errors 0.

  **The cost:** the turn latency's "ChatGPT audio -> Discord relay handoff" was 269–282 ms, against 50–125 ms in the morning's calls.

  Replay of the same call:
  - adaptive: 0 holes, speech stretched 161 ms of 13.6 s (1.2 %), wait p95 395 ms;
  - without stretching and bridging: 1 hole of 440 ms and 8 playout underruns, even at the same measured depth.

  Not checked: hearing it in a real Discord channel.
- [x] 7.3 Verify: `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` all pass; report their output. (fmt clean. Clippy: no errors, and the same 6 warnings in code this change does not touch; the 2 new `chunks_exact` warnings in `playout.rs` and the replay test were fixed. Tests: core 160, desktop 6, all pass.)
- [ ] 7.4 Build the release `voicebro.exe` at the change tip and hand it over. Verify: the build stamp names the commit.

## 8. The start of a call (spec: Buffer depth follows measured lateness; design D2a)

Added on 2026-11-10 after the first implementation. The first stall of a call, before the depth had learnt anything, was still heard once.

- [x] 8.1 Add `DepthEstimator::with_prior()`: 30 % of the histogram's mass in the 160–180 ms bucket and 70 % at 0, giving 200 ms. `JitterBuffer::new()` uses it, while the baseline and the packet tests start empty. Verify by unit tests:
  - a new estimator with the prior gives 200 ms;
  - after 30 s of a steady server it gives 60 ms;
  - a synthetic first call with one 200 ms stall 3 s in, in the middle of a phrase, has no hole and no speech gap.

  Tests:
  - `the_prior_starts_at_200_ms_and_fades_on_a_steady_server`: 200 ms at the start and after 5 s, 60 ms after 30 s;
  - `the_first_stall_of_a_call_is_not_heard`: 0 holes from the prior, against 1 hole of 100 ms from an empty start;
  - `stalls_are_absorbed_from_the_first_one`: the stream with three stalls now has 0 holes; it had 1.

  The burst test and the bridging and packet tests start empty, as written.
- [x] 8.2 Keep the learned estimator per guild in `CallReport` when a call closes, and start the next call from it (reference and window cleared). The call's open line names the start used. Verify by unit tests:
  - a buffer started from another's estimator begins at that one's depth;
  - the stalling stream played twice in a row has no hole the second time.

  Verify in a live run with `VOICEBRO_CODEX_CALL_WAV2`: the second call's open line names the learned depth.

  - **Where it is kept.** The assistant keeps one slot per guild for the app's run (`learned_depths`, next to the conversations), so the depth also survives leaving and rejoining voice.
  - **Unit tests.** `a_call_starts_from_what_the_previous_one_learnt`: first call 1 hole and 320 ms learnt; the next call starts at 320 ms and has 0 holes. `what_was_learnt_is_kept_without_the_call_s_clock` checks the reference and window are cleared.
  - **Live run.** Logs are not printed in tests, so the live test checks the status instead. The first call learnt 200 ms. The test then planted a 340 ms estimator in the guild's slot, and the second live call opened with 340 ms.
- [x] 8.3 Update `docs/ARCHITECTURE.md` (the playout part of 4.5 and the contract table), then repeat 7.3 and the replays of the three recordings. Verify: the checks pass, and the replays have no hole.

  - **Checks.** fmt clean. Clippy: no errors, the same 6 old warnings. Tests: core 164, desktop 6.
  - **Replays**, all with 0 holes:
    - plain: wait p95 257 ms (163 before the prior);
    - delegated: wait p95 320 ms;
    - live-adaptive: wait p95 395 ms.

## Workflow follow-up

- Checkpoint commits on `change/codex-adaptive-playout`, one per finished task group, each
  with the user's OK.
- Hearing the answers in a real channel with several people and a game is the user's step.
- Merged into `main` with `--no-ff --log` in chain order once the app works end to end and
  the user says so; the change is archived in the same step.
