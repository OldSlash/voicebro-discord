# Tasks

## 1. Branch

- [x] 1.1 Create the branch `change/transmit-only-when-speaking` from `main`; verify `git log -1` shows the `main` tip `43f4d11` and `git status` lists nothing changed besides this change's folder and `docs/HANDOFF-MODES-REPORT.md` (untracked from before, not part of this change) (both `43f4d11`; `git status` shows only the change folder and `docs/HANDOFF-MODES-REPORT.md`)

## 2. Voice ahead from the call

- [x] 2.1 Add `Playout::future_peak()` (the peak of the decoded samples not yet played) and, in `JitterBuffer`, compute voice ahead at the end of each pull under the same lock against `SIGNAL_PEAK`, stored in the state and cleared by `reset`; add `FrameProvider::voice_ahead()` with a default of `false` and implement it for `JitterBuffer` (design D3); verify with a jitter test on a synthetic stream (silence, then a voiced segment, then silence) that voice ahead turns true at least two pulls before the first voiced frame is played, stays false over the silence before it and turns false once the voiced samples have played, and that every existing jitter and playout test still passes (`voice_ahead_is_seen_before_the_voice_plays`: with the 200 ms prior the voice is seen 4 pulls before it plays, at the 60 ms minimum depth 2 pulls; never while only silence has arrived, never after the last voiced frame; `reset_empties_the_buffer` checks the reset; all 23 jitter tests pass with libopus)
- [x] 2.2 In `docs/ARCHITECTURE.md` 4.1 (`FrameProvider`), say what voice ahead is and where the call's jitter buffer takes it from; verify the text names `voice_ahead`, the decoded lookahead of up to 100 ms and `SIGNAL_PEAK` (the `OutputQueue` / `PcmSource` item names `FrameProvider::voice_ahead`, `LOOKAHEAD` 100 ms and `SIGNAL_PEAK` 256)

## 3. The sending decision in the output

- [x] 3.1 In `audio/output.rs`, add the Silent / Opening / Sending state to `PcmSource` with `SEND_LEAD` (2 frames) and `SEND_HANGOVER` (25 frames), the peak stored with each pushed frame, earcons held while Opening, the `watch` sender set on transitions only, `OutputHandle::sending()` (a receiver), and the counters `sending`, `sends_started`, `frames_not_sent` and `cut_starts` in `OutputCounters` (design D2, D4, D6); verify with unit tests that:
  - an output with nothing to play never sends and counts every frame as not sent;
  - pushed voice turns sending on before its first voiced frame is played, with no cut start;
  - sending stops after exactly 25 quiet frames, and a pause of 20 quiet frames keeps it on;
  - an earcon from Silent waits two frames and then plays every sample;
  - an earcon while output is muted turns sending on, and muted assistant frames never do;
  - a provider reporting voice ahead turns sending on before its voiced frame;
  - a voiced frame with nothing ahead counts one cut start;
  - the receiver sees one change per transition and none in between;
  - every existing output test still passes.

  Done: the state is `Sending` (Silent / Opening / On) in `Inner::step_sending`. There are eight new tests in `audio::output::tests`, one per point above. The receiver test is `the_sending_receiver_sees_each_change_once`. With pushes in real time, sending is on 2 reads before the first voiced frame. With a provider that sees four frames ahead, 4 reads before. Three existing tests read an earcon in the first packet; they now read up to `SEND_LEAD` + 1 packets: `mute_drops_queued_and_incoming_but_earcons_play`, `the_output_gain_scales_pushed_and_pulled_frames_but_not_earcons` and the gate's `mute_tone` test. `PcmSource::sending()` was not added: with the encoder left as it is nothing reads it, and `OutputCounters.sending` carries the state. All 265 core tests pass with libopus.
- [x] 3.2 In `docs/ARCHITECTURE.md` 4.1 (`OutputQueue` / `PcmSource`, Source lifetime, Earcons) and section 6, describe the decision: audible now and ahead, the two constants, earcons held while opening, silence played but not sent; verify the section 6 table has a "Sending toward Discord" row with the peak, the hangover and the lead (new item "Sending toward Discord" in 4.1, Source lifetime and Earcons amended, row added after "Opus toward Discord")

## 4. The Discord session

- [x] 4.1 In `discord/session.rs`, mute the driver with `songbird::Driver::mute` (never `Call::mute`) before `play_only_input`. Spawn the task per join that applies the output's `sending()` changes to the driver: its loop is a function over the receiver and an apply callback. Keep its `JoinHandle` in `VoiceSession` and abort it in `leave_voice_locked` before `manager.remove` (design D1, D4). Verify with a tokio test that the loop applies every change in order, applies none while nothing changes, and returns when the sender is dropped. Verify with the existing session tests that a refused preparation still never builds a track or a task. (Done: `the_sending_loop_applies_each_change_and_ends_with_the_output` covers true, false, true in order, nothing without a change, and the end when the sender drops. `a_refused_preparation_keeps_the_bot_out_and_tells_the_caller_why` passes: the task is spawned only after a join, so a refused preparation builds neither a track nor a task. The whole workspace builds with `--all-targets`.)
- [x] 4.2 Make `delivery::leave_line` carry the output counters: how many times sending turned on, the time not sent in seconds and the cut starts (design D6); verify with a unit test of the line's text and that the channel status JSON carries the new fields (`the_join_and_leave_lines_name_the_server_and_the_counters` checks "sending turned on 4 times, nothing sent for 60.4 s, 0 cut starts" and the four fields in the serialized `OutputCounters`, which is `VoiceChannelStatus.output`)
- [x] 4.3 Update `docs/ARCHITECTURE.md` 4.2 (Output, the leg toward Discord) and section 5 (Audio out), and add to `AGENTS.md` (audio facts) that the bot sends only while something is audible, through `Driver::mute` and never `Call::mute`, which would show the bot muted in Discord, and that the encoder and the loss path are not involved; verify both files name the hangover, the lead and the reason for `Driver::mute`

## 5. Checks on real call audio

- [x] 5.1 Add a replay check (skipped without `VOICEBRO_PLAYOUT_REPLAY`) that plays each recorded call through a `JitterBuffer` attached to an `output_pair`, pulls `PcmSource` on the simulated 20 ms clock, applies `sending()` one tick late as songbird would, and prints the counters. Run it on the seven recordings of 2026-11-10 to 11-12 (kept outside the repository, under the earlier sessions' scratchpad `recordings\` and `playout\` folders). Verify that:
  - every voiced frame is pulled while the late-applied state is on;
  - `cut_starts` is 0;
  - sending is off for most of each gap between answers;
  - `sends_started` is about the number of answers, with no blinking inside an answer.

  Done as `jitter::tests::sending_on_recorded_calls_when_enabled`. It ran on all eleven recordings found, the seven plus `mousiki-a/b` and `ropus-a/b` (copied into this session's scratchpad `recordings\`), with libopus and with opus-rs, the mute applied one and two ticks late. Results, the same for both codecs:
  - 0 voiced frames not sent and 0 cut starts, even two ticks late.
  - Nothing sent for 55 to 68 % of each call with answers, and 94 % of the two calls that were mostly silent (`mousiki-a`, `opus-rs-a`).
  - In each gap over 1 s, sending stayed on exactly 25 frames after the voice and turned on 1 to 5 frames before the next one; once 9 (`delegated`, where the playout stretched the pause while the voice waited ahead).
  - No pause of 0.5 to 1 s inside an answer, so no blinking. `sends_started` is the number of gaps plus one (4 or 5 per call).

  The check asserts that the hangover is at most 25 frames, that turning on early stays within the lookahead plus the playout's pause-stretch limit (30 frames), and that sending never blinks inside a gap.
- [x] 5.2 Run the live call test with the ChatGPT account (`VOICEBRO_CODEX_SMOKE`, `VOICEBRO_CODEX_CALL_WAV`, as in `LOCAL-TESTING.md`) and the RTC loopback test; verify the answer is heard whole (the canary word starts it), input drops are zero, and the printed output counters show sending on for the answer, off after it, and zero cut starts

  2026-11-14, with `q1-remember` and `q2-which` from `target\opus-compare\input\calls\`. The test now prints and checks the output's sending counters.
  - libopus: ready in 2.5 s. Both answers start with "Banana", and the second recalls 42. Input drops 0. Toward Discord: sending turned on 2 times, nothing sent for 805 of 1079 frames, 0 cut starts.
  - opus-rs: ready in 1.5 s. Same answers, input drops 0. Sending turned on 2 times, nothing sent for 815 of 1191 frames, 0 cut starts.
  - The RTC loopback passes with both codecs: 0 input and output drops, 0 speech gaps.
- [x] 5.3 Run the release benchmark `output_pull_cost` for both codecs; verify one pull costs the same as in `docs/OPUS-CODEC-COMPARISON.md` within the noise (the decision adds no measurable work)

  One pull toward Discord, fill and encode, mean / p99 / max in µs:
  - libopus: 616 / 754 / 1054 (the report: 630 / 828 / 1132).
  - opus-rs: 248 / 322 / 2101 (the report: 258 / 401 / 994). The max is one spike in 3000 frames; the report noted such spikes for opus-rs too.

  The fill alone, with the sending decision, takes 1.3 µs.

## 6. Whole-project checks

- [x] 6.1 With CMake on PATH run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (the whole workspace, both codecs through `VOICEBRO_OPUS_CODEC`), `node apps/desktop/ui-dev/check-wake-dialog.mjs`, `scripts\run-wake-corpus.ps1` and `scripts\build-release.ps1 -Trial`; verify each passes and `openspec validate transmit-only-when-speaking --strict` passes; the report lists what each returned and what was not tested: the bot in a real Discord channel, which the user tests after the implementation

  Results, 2026-11-14:
  - `cargo fmt --check`: clean, after `cargo fmt` on the changed files.
  - `cargo clippy --all-targets`: no errors. The warnings (`chunks_exact` in `secrets.rs`, the probe and the output test helper `read_packet`, `let...else` in `receive.rs`) are all in code this change does not touch.
  - `cargo test`: 267 core, 15 desktop and 1 probe tests pass, with libopus and with opus-rs.
  - `check-wake-dialog.mjs`: all 23 scenarios pass.
  - `run-wake-corpus.ps1`: 77 of 98 right, no false hit. Every file gives the same result as the run of 2026-11-11, whose 79 of 100 included `ru-positive-48k.wav` (2 right runs), no longer in the corpus. The catalog check loads all ten models.
  - `build-release.ps1 -Trial`: builds `target\release\dist-trial\` (installer and zip, 1.0.0-trial); not installed.
  - `openspec validate --strict`: valid.

  Not tested: the bot in a real Discord channel, which the user tests after the implementation.

## Workflow follow-up

- The user checks the bot in Discord with two or more people: the ring lights only while the bot speaks or plays an earcon, no answer starts cut, the bot is never shown muted, and the reported loss shows no jump right after an answer starts. If the ring stays lit while `frames_not_sent` grows, the track-per-answer variant (design Risks) is a change of its own. If the reported loss jumps after answers start, songbird's sequence numbering is the fix, also in a change of its own.
- The build for that test follows the git flow: the branch is merged into `main` and tagged before an installer is built; a `-Trial` build is for the agent's own checks only.
- One commit on the branch when the user says to commit or merge: code, docs and the change archived. Before it run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`, and report what they returned.
- Merge with `git merge --no-ff --log change/transmit-only-when-speaking` when the user says so, then delete the branch.
