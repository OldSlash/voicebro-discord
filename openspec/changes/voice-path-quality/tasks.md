# Tasks

## 1. Send path toward the call (spec `assistant-call-audio`)

- [x] 1.1 Create the branch `change/voice-path-quality` from the tip of `change/settings-ui-and-acceptance` (chained branches, `AGENTS.md` Git flow); verify `git log -1` shows the previous tip as parent
- [x] 1.2 Extract the send scheduling into a pure function (queue depth and prefill state in, frame or silence out) with the 2-frame prefill and the 5-frame cap; verify unit tests: a producer with intervals from 5 to 35 ms loses no frame and keeps order, an empty queue yields silence and counts an input underrun, a backlog over 5 frames drops the oldest and counts them
- [x] 1.3 Rewrite `send_loop` in `rtc.rs` on that function: one packet every 20 ms, silence when empty, counters for frames sent, input drops and input underruns; the channel toward the call holds 5 frames; verify the unit tests and `cargo test -p voicebro-core assistant::`
- [x] 1.4 Set the encoder to complexity 10, 32 kbit/s, in-band FEC, 5 % expected loss (check the `opus2` 0.4 setter names in its source); verify `VOICEBRO_BENCH=1 cargo test --release -p voicebro-core receive_path_cost` stays under 1 ms per tick and record the number in `docs/MVP-PLAN.md`

## 2. Receive path and playout (spec `assistant-call-audio`)

- [x] 2.1 Read `rtc::media::io::sample_builder` in the `rtc` 0.21 source and decide reuse or own code per design D2; record the decision in design.md; verify the note exists
- [x] 2.2 Add the jitter buffer module: packets by sequence number, adaptive target 3 to 8 frames, late packets dropped and counted, FEC decode from the next packet, concealment for a sequence gap capped at 5 frames, silence for a pause with contiguous sequence numbers; verify unit tests with real Opus packets: reorder 10, 12, 11, 13 plays in order; a lost packet with the next one buffered is FEC-decoded; three lost packets give three concealed frames; a late packet is counted; 10 ms and 40 ms packets are sliced into 20 ms frames
- [x] 2.3 Let `PcmSource` pull frames from the call's jitter buffer on songbird's read clock, keep `OutputHandle::push` for the stub and tests, and clear the buffer on flush and mute; verify the existing output and gate tests still pass and a new test shows a flush empties the jitter buffer
- [x] 2.4 Wire `read_remote_track` to insert packets instead of pushing frames; verify an in-process loopback test (two peers as in `apps/probe/src/rtc.rs`) sends a 10 s tone through the new send path with a jittery producer and the receiving side counts zero input drops, zero late and zero concealed frames, with correlation above 0.9
- [x] 2.5 Add the counters to `CallStatus` and log them in one line at the end of each call; verify a unit test of the log line and the loopback test's counters in the status

## 3. Agent and voice (spec `codex-assistant`)

- [x] 3.1 Send `approvalPolicy: "untrusted"` in `thread_start_params`; verify the unit test of the parameters and, in the gated live test, a delegated question about current facts produces a `webSearch` entry in the chat log
- [x] 3.2 Allow an empty `codex.voice` and make it the default for new settings files (the first version's effective behaviour; existing files keep their voice), start the call without a voice field when empty, log "server default", add "(server default)" to the Settings voice list; verify unit tests of validation, of the default and of the start parameters

## 4. Log noise

- [x] 4.1 Set `songbird::driver::tasks::udp_rx` to Error in the desktop log plugin and in the probe's logger; verify `cargo run -p probe -- discord-lifecycle` writes no `RTCP decryption failed` line (the probe installs no logger, so it never wrote these lines; the desktop's level table is unit-tested and fern's lookup was checked to pick the most specific module; seeing the log stay clean in a channel is the user's step)

## 5. Integration check and docs

- [x] 5.1 Extend `live_call_when_enabled` with a jittery tick (5 to 35 ms) for the question frames; run it with `q1.wav` and `q2.wav` and compare the user transcript with the spoken text and the counters; record the result in `docs/MVP-PLAN.md`; verify the transcript matches the spoken sentence and input drops are zero (the bitrate comparison of design D4 at 24 and 48 kbit/s was not run: 32 kbit/s already gave the exact transcript)
- [x] 5.2 Update `docs/ARCHITECTURE.md` 4.5 (`rtc.rs`: FIFO send, pulled jitter buffer, counters; thread approval policy; voice default) and the audio facts in `AGENTS.md`; verify no statement about "newest frame" or `approvalPolicy: never` remains
- [x] 5.3 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output (fmt clean; clippy no errors, 6 warnings left in code this change does not touch: `discord/receive.rs`, `secrets.rs`, a test helper in `audio/output.rs` and three in `apps/probe`; tests: core 110, desktop 1, all pass)

## Workflow follow-up

- Checkpoint commits per task group on `change/voice-path-quality`, with the user's OK.
- The real-channel check of the voice (two people, a game) is the user's step.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
