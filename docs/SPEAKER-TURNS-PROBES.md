# Speaker turns: probes

Change `speaker-turns`, group 2. Two live probes on this PC with the real ChatGPT sign-in
(Plus) and Codex CLI 0.167.0, run on 2026-11-18 before any code of the floor or of the
Whisper queue was written. They set the values design.md left to them: the floor pause
(D2), the hold before a waiting buffer (D5), and whether the Whisper session adds a
question to the running turn with `turn/steer` or starts the next turn (D6').

Material (`target\speaker-turns\`, made by `make.ps1` and `mix.py` there):

| File | What | Length |
|---|---|---|
| `a.wav` | Microsoft David: "What time is it in Bangkok?" | 2.44 s |
| `b.wav` | Microsoft Zira: "And what is the weather in London?" | 2.35 s |
| `mixed.wav` | `b.wav` starting 2 s before `a.wav` ends, summed as the gate mixes (sum, divide by the count, clamp) | 2.79 s |

Whisper (base, English) reads `a.wav` and `b.wav` back word for word.

## Probe 2.2: `turn/steer`

`cargo test -p voicebro-core turn_steer_when_enabled -- --nocapture` with
`VOICEBRO_CODEX_STEER=1` (`assistant/codex/app_server.rs`). Per run, on a fresh app-server
and an ephemeral thread with the instructions "answer briefly; when more than one person
asked something, answer each of them by name":

1. `turn/start` with "Alex: What time is it in Bangkok?", then at once `turn/steer` with
   `expectedTurnId` and "Sam: And what is the weather in London?";
2. a second `turn/steer` after `turn/completed`;
3. on a fresh thread, the same two questions as two turns in a row.

Results of five runs (`target\speaker-turns\steer-runs.txt`; times from the first
`turn/start`):

| Run | Steer | Steered turn: first words | Steered turn: done | Answer names both cities | Two turns: A done | Two turns: B first words | Two turns: B done |
|---|---|---|---|---|---|---|---|
| 1 | accepted 10 ms after the start | 7.0 s | 22.1 s | yes | 11.8 s | 16.5 s | 33.9 s |
| 2 | accepted 13 ms after the start | 5.9 s | 20.8 s | yes | 9.6 s | 12.9 s | 26.4 s |
| 3 | accepted 10 ms after the start | 10.8 s | 44.3 s | yes | 16.2 s | 23.1 s | 165.7 s |
| 4 | accepted 11 ms after the start | 7.1 s | 36.1 s | yes | 8.8 s | 11.6 s | 24.5 s |
| 5 | accepted 10 ms after the start | 6.2 s | 19.6 s | yes | 10.4 s | 14.3 s | 60.2 s |
| Median | | 7.0 s | 22.1 s | 5 of 5 | 10.4 s | 14.3 s | 33.9 s |

- Every steer was accepted, and every answer of the steered turn answered Alex and Sam by
  name, the time in Bangkok and the weather in London, in one turn. The agent searched the
  web for both.
- A steer of a completed turn is refused with "turn/steer: no active turn to steer".
- The steered turn's first words come about 1.8 s later than a turn with Alex's question
  alone (median 7.0 s against 5.2 s for A in the two-turn runs), but the whole answer ends
  about 12 s earlier than two turns in a row (22.1 s against 33.9 s), and Sam hears his
  answer about 7 s earlier.

## Probe 2.1: two people in the voice call

`cargo test -p voicebro-core two_speakers_when_enabled -- --nocapture` with
`VOICEBRO_CODEX_TURNS_A`/`_B` (`assistant/codex/mod.rs`). Per run a fresh realtime call
(WebRTC, the ChatGPT sign-in, labels at `changes` so the note can be sent) gets `a.wav`,
then N ms of silence, then, with the note, "now speaking: Sam" through
`thread/realtime/appendText` (developer role), then `b.wav`, then silence until the agent's
final answers have been retold. Six gaps, with and without the note, three runs each: 36
calls (`target\speaker-turns\turns-runs.txt`, summed by `summarize.py` there).
"Both answered" counts the agent's final answers when the voice handed the questions over,
else the voice's own lines.

| Gap | Note | Runs | Answered before B started | A and B as two turns | Both answered | Median from B's end to the first answer audio |
|---|---|---|---|---|---|---|
| 100 ms | no | 3 | 0 | 1 | 3 | 1.33 s |
| 100 ms | yes | 3 | 0 | 3 | 2 | 1.45 s |
| 200 ms | no | 3 | 0 | 0 | 3 | 1.46 s |
| 200 ms | yes | 3 | 0 | 2 | 1 | 1.92 s |
| 300 ms | no | 3 | 0 | 0 | 2 | 1.74 s |
| 300 ms | yes | 3 | 0 | 2 | 2 | 1.45 s |
| 500 ms | no | 3 | 0 | 1 | 2 | 1.74 s |
| 500 ms | yes | 3 | 0 | 3 | 2 | 1.48 s |
| 700 ms | no | 3 | 0 | 3 | 2 | 1.38 s |
| 700 ms | yes | 3 | 0 | 3 | 2 | 1.29 s |
| 1000 ms | no | 3 | 0 | 3 | 2 | 2.28 s |
| 1000 ms | yes | 3 | 0 | 3 | 3 | 1.77 s |
| All | no | 18 | 0 | 8 | 14 | |
| All | yes | 18 | 0 | 16 | 12 | |

- **The model never answered in the gap.** In none of the 36 runs did answer audio start
  before B started, up to 1000 ms of silence. It ends a turn later than that, or waits for
  more speech, so a floor pause of 300 ms cannot let it answer the first person before the
  second one's buffer arrives.
- **Up to 300 ms the model mostly heard one question.** Without the note it took A and B as
  one turn in 8 of 9 runs and answered both in 8 of 9; from 500 ms on it split them more
  often.
- **The note splits the turn.** With the note the model split A and B in 16 of 18 runs
  (8 of 18 without) and answered both in 12 of 18 (14 of 18 without); at the gaps up to
  300 ms, 5 of 9 against 8 of 9. When it split them, it sometimes handed only the second
  question to the agent and dropped the first.
- Where the note lands: the chat shows it as a context item at the moment it was sent,
  before the transcript of A completes, so it reaches the conversation between the two
  people's audio as intended; the harm is in the split, not in the order.

## Check 6.1: two overlapping people through the built floor

`cargo test -p voicebro-core two_speakers_through_the_floor_when_enabled -- --nocapture`
with `VOICEBRO_CODEX_FLOOR_A`/`_B` (`assistant/codex/mod.rs`). Per run a real gate in
always mode feeds a fresh realtime call: Alex (source 1) says `a.wav`, Sam (source 2)
starts `b.wav` 2 s before Alex ends, as two Discord streams on a 20 ms tick; the gate
mixes them (switch off) or passes one at a time (switch on). Ten runs each, labels at
`start` (`target\speaker-turns\floor-runs.txt`). Latency is from the last speech (the end
of `b.wav` as said) to the first answer audio.

| | Both questions answered | Answer names who asked what | Median latency | Latencies, ms |
|---|---|---|---|---|
| Mix (switch off) | 9 of 10 | 0 of 10 | 1.60 s | 974, 1277, 1360, 1371, 1559, 1631, 1819, 1999, 2559, 6442 |
| One at a time (switch on) | 10 of 10 | 0 of 10 | 3.06 s | 11, 2578, 2940, 3042, 3050, 3061, 3089, 3117, 3151, 3188 |

- **Both answered:** 10 of 10 with the floor, 9 of 10 with the mix.
- **Names:** without a note the model cannot know whose voice is whose. It answered
  without names or guessed, and the guess was the wrong way round in 5 of the 10 floor
  calls ("Sam, it's 3 p.m. in Bangkok. Alex, London…").

The same check, ten floor calls each, with notes from the floor
(`floor-changes-runs.txt`, `floor-start-runs.txt`), judged by what the voice said:

| Notes | Both answered | Right names | Wrong way round | No names | Latencies, ms |
|---|---|---|---|---|---|
| None (the table above) | 10 of 10 | 3 | 5 | 2 | median 3055 |
| Only for Sam, who waited | 10 of 10 | 2, partly | 2 | 6 | 10, 30, 31, 51, 130, 161, 171, 3338, 3435, 3700 |
| At every change: Alex, then Sam | 10 of 10 | 7 | 0 | 3 | 3, 9, 20, 50, 68, 105, 261, 2759, 2930, 3200 |

With a note at every change the answers named the right person in 7 calls and never the
wrong one. Naming who asked what is not a criterion of the change (the user's decision of
2026-11-19): the call has one audio track, so the model learns who speaks only from the
text note, and a better way is left for another time. In the Whisper mode every question
reaches Codex as text with the speaker's name, so it does not arise there. With notes the first answer audio often came within 0.3 s of the last speech:
the voice acknowledged ("Mm-hmm. Thanks Sam, I'll check both") while the agent looked
the answers up.
- **Latency:** the floor answers about 1.5 s later. That is the shift the floor makes by
  design: Sam spoke for 2 s over Alex, so Sam's question reaches the call after Alex's,
  plus the 300 ms pause, less the silent frames skipped. The done criterion "no more than
  300 ms worse than the mix" cannot hold for a 2 s overlap and is not met; with people
  taking turns (no overlap) the floor adds nothing.

## Check 6.2: two questions in the Whisper mode, built

`cargo test -p voicebro-core two_questions_one_at_a_time_when_enabled -- --nocapture` with
`VOICEBRO_CODEX_TWO_QUESTIONS=1` (`assistant/codex/mod.rs`, the text session's rig on the
real Codex; a reader stands in for the local voice). One speaker at a time on; Alex asks
"What time is it in Bangkok?", Sam "And what is the weather in London?" 1 s later.
Five runs (`target\speaker-turns\whisper-two-runs.txt`):

| Run | Posted answers | Task line | Both answered, by name | Read to its end | First spoken piece | Done |
|---|---|---|---|---|---|---|
| 1 | 1 | "What time is it in Bangkok? / And what is the weather in London?" | yes | yes | 6.8 s | 24.8 s |
| 2 | 1 | the same | yes | yes | 8.9 s | 36.6 s |
| 3 | 1 | the same | yes | yes | 18.0 s | 45.3 s |
| 4 | 1 | the same | yes | yes | 6.7 s | 23.0 s |
| 5 | 1 | the same | yes | yes | 13.4 s | 35.9 s |

Sam's question joined Alex's running turn every time; nothing was interrupted, and the one
answer named Alex with the time and Sam with the weather. Not covered by this rig: the
latency lines, which need the voice's output to be played (nobody pulls it here).

## Choices

- **Floor pause (D2): 15 ticks, 300 ms, unchanged.** The model never answered within the
  gap in any run up to 1000 ms (probe 2.1), so 300 ms is below its own turn end with far
  more than the 100 ms margin, and up to 300 ms it mostly heard the two people as one
  question and answered both.
- **The note (D5): at every change of the floor, unless the labels are `off`.** Probe 2.1
  found that a note in a raw gap split the turn more often, but on the built floor (check
  6.1) every call answered both questions with or without notes, and only a note at every
  change got the names right (7 of 10, never wrong). So with the floor on, `start` and
  `changes` both send it, with the 240 ms hold; the caller named in the call context gets
  no second note.
- **The Whisper session (D6'): `turn/steer`.** Every steer was accepted and answered both
  questions by name in one turn, and the whole answer ended about 12 s sooner than two
  turns in a row (probe 2.2). The fallback (the next turn) stays for a refused steer.
