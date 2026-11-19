# Tasks

Decisions the request left open are in design.md, marked **[decided alone]** (D4, D5's
hold, D6). Questions that come up while building are decided the same way and added to
design.md under the decision they touch, so the user can review them all at the end.

Group 2 runs before any code of groups 3 to 5. Its report sets the floor pause (D2), the
note hold (D5) and whether the Whisper session uses `turn/steer` or the next turn (D6').
If a value or choice differs from design.md, update design.md and the spec deltas first,
then build.

## 1. Branch and test material

- [x] 1.1 Build on the uncommitted work of `voice-consent` in the same checkout, since it is not committed (a commit needs the user's word) and both changes touch the settings, the window and the docs. Keep its snapshot in `target\voice-consent-snapshot\` (`tracked.patch` against `main` plus its new files) so that the two can be committed apart: `voice-consent` first, then this change on `change/speaker-turns` cut from it. Verify that the snapshot applies to a clean `main` with `git apply --check`
- [x] 1.3 Add `assistant.one_speaker_at_a_time` (D0): the field with serde default `false`, `GateSettings` carrying it, the switch "One speaker at a time" in the Assistant card with its help line, the mock's settings and a `?one_at_a_time` parameter, and the scenarios of the `app-settings` and `settings-ui` deltas in `check-config-tab.mjs` (the switch list too). Verify:
  - `cargo test -p voicebro-core settings::` with a 1.2.0 file loading `false`, `"yes"` rejected naming the field, and a save of only this field restarting no wake engine;
  - `node apps/desktop/ui-dev/check-config-tab.mjs` passes
- [x] 1.2 Make the probe material in `target\speaker-turns\` with the Windows voices `scripts\make-wake-corpus.ps1` uses: `a.wav` (David, "What time is it in Bangkok?"), `b.wav` (Zira, "And what is the weather in London?"), both 48 kHz mono 16-bit, and `mixed.wav`, in which `b.wav` starts about 2 s before `a.wav` ends, summed as the gate mixes (sum, divide by the count, clamp). Verify that Whisper reads each file back word for word (`probe whisper <wav>`), and that `mixed.wav` is as long as the two minus the overlap

## 2. Probes and their report

- [x] 2.1 Extend `live_call_when_enabled` (`assistant/codex/mod.rs`) with a two-speaker run. It feeds `VOICEBRO_CODEX_TURNS_A`, then N ms of silence, then (when `VOICEBRO_CODEX_TURNS_NOTE` is set) `speaker_note(["Sam"])` through `thread/realtime/appendText`, then `VOICEBRO_CODEX_TURNS_B`, for each N in `VOICEBRO_CODEX_TURNS_GAPS` (default `100,200,300,500,700,1000`). Per run it prints:
  - whether the model's first answer audio came before B started;
  - the people's transcript pieces in order;
  - the item events with the note's position between them;
  - the latency lines.

  Run it three times per gap, with and without the note, on the real ChatGPT sign-in. Verify that every run prints all four parts and none fails on setup
- [x] 2.2 Probe `turn/steer` against the real app-server. Start a text turn with "Alex: What time is it in Bangkok?". After `turn/started` and before `turn/completed`, send `turn/steer` with `expectedTurnId` and "Sam: And what is the weather in London?". Record:
  - whether the final answer covers both cities;
  - the time to first words;
  - the error a steer of a completed turn returns;
  - the same two questions sent as two turns in a row, for comparison.

  Run it five times. Verify that every run prints the answer text and the times
- [x] 2.3 Write `docs/SPEAKER-TURNS-PROBES.md` with the tables of 2.1 and 2.2. Then set from them:
  - `FLOOR_PAUSE_TICKS`: the largest gap at which the model never answered before B, minus at least 100 ms, at most 300 ms;
  - the note hold of D5;
  - steer or the next turn for D6'.

  Update design.md D2, D5 and D6' and the spec deltas where a value or a wording changes, and run `openspec validate speaker-turns --strict`. Verify that the report states each choice with the run that decided it and that validation passes

## 3. The floor in the gate

- [x] 3.1 Add the floor of D1 and D2 to `gate.rs`:
  - the holder with its silent-tick count;
  - the waiting list in order;
  - take, hold and pass by `FLOOR_PAUSE_TICKS`;
  - in realtime mode, both session modes, the gate passes the holder's frame instead of the mix, only in a session that started with the switch on (D0).

  Verify unit tests on recorded per-speaker frames:
  - with the switch off the passed frames are the mix, as before;
  - one speaker passes unchanged with no delay;
  - a second speaker during the first is never in the passed frames;
  - a 200 ms pause keeps the floor, a 300 ms one passes it;
  - with three speakers the earlier waiter goes first;
  - always mode behaves the same

- [x] 3.2 Add the buffers of D3: fixed capacity of `MAX_WAIT_FRAMES`, the oldest dropped and counted, the drain one frame per tick followed by live frames, silent frames skipped at the front, and everything cleared on session end, mute and deactivation. Verify unit tests:
  - 20 s of waiting speech keeps the last 15 s and counts the rest;
  - a buffer of speech, silence and speech passes both speech parts with less than 1 s of the silence;
  - live frames follow the buffer with no frame lost or repeated;
  - a deactivation empties the buffers.

  Run `cargo test -p voicebro-core gate::`

- [x] 3.3 Add D4: the floor does not pass to a waiting person while the output's signal is newer than 200 ms, and passes 200 ms after it stops. Verify unit tests with a fake output signal:
  - a waiting buffer stays until the answer ends, then passes;
  - a new speaker with nobody waiting goes to the call live during the answer
- [x] 3.4 Replace the mixed pre-roll with the per-speaker rings of D6. On `on_phrase` the caller takes the floor, their ring from the end of the phrase becomes `replay`, and the other rings' frames from that point move into their buffers. Verify:
  - the existing pre-roll tests ("Question right after the phrase", "Assistant never becomes ready") still pass;
  - a new test for "Someone else talks during the phrase": only the caller's speech is replayed and the other speech waits;
  - a ring is given back after 5 s of absence
- [x] 3.5 Add the counters of D8 to `GateCounters`, the status and the gate's status line. Show them in the window wherever the other gate counters appear. Verify:
  - a unit test reads one waiting person, the passes and the drops from the status;
  - `node apps/desktop/ui-dev/check-config-tab.mjs` still passes if the window shows gate counters.

  Update `docs/ARCHITECTURE.md` (the gate section: the floor, the buffers, the per-speaker pre-roll and the counters) and the audio facts in `AGENTS.md`: overlapping speakers are still averaged in the mix, but the call gets one speaker at a time

## 4. The note before a passed floor

- [x] 4.1 Replace `labels_for` with the rule of D5: at `changes` a note at every floor change, at `start` only for a person who waited, at `off` never. The gate holds the first frame of a waiting buffer until the session reports the note's `appendText` reply, or until the hold of 2.3 runs out. Verify:
  - gate unit tests for each label setting;
  - a session test against the fake Codex that the note request is sent before the first buffered frame enters the FIFO, and that a reply that never comes releases the buffer after the hold.

  Update `docs/ARCHITECTURE.md` (the speaker notes) to match

## 5. The Whisper session waits its turn

- [x] 5.1 Change `text_session.rs` as D6' (or its fallback, as 2.3 decided), only while the switch is on (D0); off keeps today's interrupt and flush:
  - no interrupt and no flush on a new question;
  - steer into the running turn, or wait for its id;
  - the next turn when steer fails or no turn runs.

  Verify against the fake Codex (`testdata/fake-codex`, extended to answer `turn/steer` and to refuse it for a completed turn):
  - a question during a running turn makes no `turn/interrupt`, and makes a `turn/steer` (or a queued `turn/start` after `turn/completed`);
  - a refused steer becomes a `turn/start`;
  - a question during the spoken answer starts a turn without raising the player's epoch;
  - deactivation, mute, `/leave`, the idle timeout and a mode change still interrupt and flush;
  - with the switch off a new question interrupts and flushes, as before
- [x] 5.2 Keep the answers in order in the player: the sentences of a later turn play after the earlier turn's last sentence. Verify a player test with two turns' sentences arriving interleaved: they play turn by turn
- [x] 5.3 Keep the questions of a turn together: the answer posting's task line joins them with " / ", every question appears in the chat log, the latency line is measured from the first question, and each added question logs "question added to turn <n>". Verify unit tests for the task line and the log line.

  Update `docs/ARCHITECTURE.md` (the Whisper session) and `docs/user-guide.md`:
  - what happens when two people talk in each mode;
  - that talking over the voice no longer stops it, and that the deactivation phrase or Mute does;
  - that two voices through one microphone still go together

## 6. Checks on the real Codex

- [x] 6.1 Run the live call test on the built floor with the two speakers of 1.2 fed as two overlapping per-speaker streams through the gate, 10 runs, and the current mixed call on `mixed.wav`, 10 runs. Verify:
  - both questions are answered in at least 8 of 10 runs;
  - the median time from the end of the last speech to the first answer audio is no more than 300 ms worse than the mixed call's.

  Add the numbers to `docs/SPEAKER-TURNS-PROBES.md`

  The user's decision of 2026-11-19: answering each person by the right name is not a criterion in the voice mode. The call has one audio track, so the model learns who speaks only from the text note; a better way is for another time.

  2026-11-18 (`two_speakers_through_the_floor_when_enabled`, the mix through the same gate with the switch off instead of `mixed.wav`, plus ten floor calls for each note rule): both questions answered in 10 of 10 floor calls in every variant (the mix: 9 of 10); right names in 7 of 10 with a note at every change, the rule now built (never the wrong way round); median latency 3.06 s without notes against 1.60 s for the mix. Not met: "no more than 300 ms worse" (about 1.5 s worse, the shift of Sam's 2 s of overlapped speech, which the floor makes by design). Closed by the user on 2026-11-19 (commit, merge and archive asked) with the latency criterion not met
- [x] 6.2 Run the Whisper text-turn test with the two questions arriving 1 s apart, 5 runs. Verify:
  - no answer is cut;
  - both questions are answered in order;
  - the latency lines appear.

  Add the numbers to the report

  2026-11-18 (`two_questions_one_at_a_time_when_enabled`, the real Codex, a reader for the voice): 5 of 5 answered both questions by name in one turn, the second question steered in, each answer read to its end, first spoken piece in 6.7 to 18.0 s. Not covered: the latency lines, which need the voice's output played; that is part of 6.4. Closed by the user on 2026-11-19 (commit, merge and archive asked) with the latency lines not checked
- [x] 6.3 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, the three window checks and `openspec validate speaker-turns --strict`. Verify that all pass, and put their results in the report
- [x] 6.4 Build a trial with `scripts\build-release.ps1 -Trial` and run it with the user in a real voice channel, with two people and a game running. Verify, in the realtime mode:
  - when two people talk over each other, the assistant answers both (naming who asked what is not checked: the user's decision of 2026-11-19);
  - one person alone gets answers as fast as before;
  - a waiting question does not cut an answer;
  - the deactivation phrase from either person ends the session.

  In the Whisper mode:
  - a second question during an answer does not cut it and is answered after it;
  - Mute stops the voice at once.

  The report lists what was not covered

  Closed by the user on 2026-11-19 (commit, merge and archive asked) without the live check: none of it was run in a real channel with two people and a game.

## Workflow follow-up

- When the user says to commit: one commit on `change/speaker-turns` with the code, the docs and this change archived into `openspec/changes/archive/<date>-speaker-turns`, its delta specs merged into `openspec/specs/`.
- When the user says to merge: `git merge --no-ff --log change/speaker-turns` into `main` with the message `Merge change/speaker-turns: <one line on the result>`, then delete the branch.
