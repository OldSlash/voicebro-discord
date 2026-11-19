# Design

## Context

See proposal.md for the motivation. The state this design starts from:

- **The receive tick.** Every 20 ms `discord/receive.rs` hands the gate the mix and each
  passing speaker's own frame (`Gate::process_tick(mixed, speakers)`). Speakers dropped by
  the owner filter (and, after change `voice-consent`, by the consent filter) never reach
  the gate.
- **The gate in realtime mode** (`gate.rs`):
  - it keeps a pre-roll of the mix (`PREROLL_FRAMES` = 250, 5 s);
  - on activation `on_phrase` gets the caller's SSRC, names the caller and moves the
    pre-roll from the end of the phrase into `replay`;
  - during a session it passes one mixed frame per tick from `replay` (up to
    `MAX_REPLAY_FRAMES` = 300 while the assistant is not ready), skipping silent frames at
    the front to catch up (`VOICE_PEAK_THRESHOLD` = 700);
  - in always mode it passes the mix directly;
  - in `changes` mode `labels_for` reports the set of people who started talking within
    `CONCURRENT_WINDOW` (400 ms).
- **Toward the call.** The session's FIFO sends one packet per 20 ms and drops only beyond
  100 ms of backlog (`assistant-call-audio`). The input gain is applied after the gate.
- **Notes in the call.** `speakers_changed` sends `speaker_note(names)` ("Service note, do
  not answer it and do not verify anything: now speaking: <name>.") through
  `thread/realtime/appendText` with role `developer`. This works in a live WebRTC call
  with a ChatGPT sign-in today; realtime v3 takes it as context and starts no reply.
- **The Whisper session** (`codex/text_session.rs`): `user_text` stops the running turn
  (`turn/interrupt`), flushes the voice by raising the player's epoch, and starts a new
  turn. The app-server schema of Codex CLI 0.167.0 has `turn/steer` with `expectedTurnId`
  (the request fails when that is not the active turn) and `input`; review and compact
  turns cannot be steered.
- **The live tests.** `live_call_when_enabled` feeds WAVs into a real call on a jittery
  tick and prints the transcripts and latency lines. `whisper_text_turn_when_enabled` runs
  Whisper, a real Codex text turn and the voice on one WAV.

## Goals / Non-Goals

**Goals:**

- No added delay for the common case of one person talking.
- Bounded memory and work per tick: nothing in the floor allocates per tick after warm-up
  beyond the buffers' fixed capacity.
- The probes decide two values before code is written, so the build does not rest on
  guesses about the model.

**Non-Goals:**

- Changing anything while the switch is off: the mix, the mixed pre-roll, the speaker notes
  and the Whisper barge-in stay as they are.
- Separating two voices that reach Discord through one microphone (a person beside the
  speaker, speakers without headphones). The floor works on Discord's streams, not on
  voices.
- Changing who the wake engine hears: it keeps listening to every passing speaker.
- Any API-key path, other engine or Realtime API feature beyond what the call already uses.

## Decisions

### D0. The switch (the user's decision of 2026-11-18)

`assistant.one_speaker_at_a_time: bool` in `AssistantSettings`, serde default `false`,
"One speaker at a time" in Config → Assistant, for both modes.

- `GateSettings` carries it. The gate reads it at the start of a session: the floor, the
  buffers and the per-speaker pre-roll (D1 to D6) run only in a session that started with it
  on; otherwise the gate passes the mix and keeps the mixed pre-roll exactly as before. A
  session never switches halfway.
- The Whisper session reads it per question (D6'): off keeps `stop_running` and the epoch
  flush of today.
- Its save is part `assistant` of `changed_sections`, which already updates the gates'
  settings and reconfigures the assistant without a rejoin.

### D1. The floor lives in the gate

The floor sits in `Gate::process_tick`, between the per-speaker frames and the frame the
gate passes to the session. The gate already owns the session, the pre-roll, the replay and
the speaker names, and it runs once per receive tick.

Alternatives: in `receive.rs` (rejected: it does not know about sessions or the assistant's
voice); in the Codex session (rejected: it sees only the frames the gate passes, after the
speakers are joined).

State per guild: the floor holder (SSRC, ticks since their last voiced frame), and an
ordered list of waiting people, each with a buffer and the tick they started waiting.

### D2. Taking and passing the floor

- A speaker's frame is voiced when its peak reaches `VOICE_PEAK_THRESHOLD`.
- Free floor: the first voiced speaker of a tick takes it; with two in the same tick, the
  louder one.
- The holder's frames, voiced or not, go to the session as they arrive.
- Any other voiced speaker joins the waiting list (once) and their frames go to their
  buffer.
- The holder releases the floor after `FLOOR_PAUSE_TICKS` = 15 ticks (300 ms) without a
  voiced frame, counted in receive ticks.
- Then the floor goes to the person who started waiting first, unless the assistant's voice
  is playing (D4). With nobody waiting, the floor is free.

The 300 ms is the starting value. Probe 1 (D7) measures the silence after which the model
ends a turn on its own. `FLOOR_PAUSE_TICKS` must stay below it with a margin of at least
100 ms, and the measured value replaces 300 ms in the code and in the spec delta before
group 3 is built.

### D3. The buffers

- One `VecDeque<Frame>` per waiting person, allocated once with room for
  `MAX_WAIT_FRAMES` = 750 (15 s). Beyond that the oldest frame is dropped and
  `floor_dropped` grows.
- When a waiting person gets the floor, their buffer drains one frame per tick, followed by
  their live frames, which are appended to the buffer until it is empty.
- Silent frames at the front are skipped while more than one frame is waiting, the same
  catch-up the pre-roll replay uses. A pause in the buffer then costs no time.
- [decided alone while building] While a passed buffer plays, the holder's live frame
  follows it when it is voiced or comes right after a voiced frame; a silent frame after a
  silent one is not appended. Otherwise a finished speaker's silence would refill the
  buffer every tick and the floor would never pass on (the unit test of three speakers
  caught it). A waiting person's silence is kept for at most 15 frames in a row for the
  same reason.
- The end of a session, mute and a deactivation clear every buffer and the floor.

### D4. A waiting person does not cut the answer **[decided alone]**

The realtime model stops its answer when it hears speech. Buffered speech replayed into an
answer would cut it, though the person asked before the answer started.

So the floor does not pass to a waiting person while the output reports the assistant's
signal (`OutputHandle::last_signal` within the last 200 ms). The buffer waits until the
voice has been silent for 200 ms.

Live speech with nobody waiting goes to the call as before. A person who talks over the
answer still interrupts it, which is how someone stops the assistant.

### D5. The note before a passed floor (the hold before the buffer: **[decided alone]**)

**Changed after the probes** (`docs/SPEAKER-TURNS-PROBES.md`) **[decided alone]**. Probe
2.1, with a raw gap and no floor, found that a note between two people's speech made the
model split them into two turns more often (16 of 18 runs against 8 of 18) and answer both
a little less often (12 of 18 against 14 of 18). Check 6.1 on the built floor answered
both questions in all 30 calls whatever the notes, and the notes decided whether the
answers named who asked what:

| Notes on the built floor | Right names | Wrong way round | No names |
|---|---|---|---|
| none (`start` before this decision would have sent none at a free take) | 3 of 10 | 5 of 10 | 2 of 10 |
| only when the floor passes to a person who waited (the plan below) | 2 of 10, partly | 2 of 10 | 6 of 10 |
| at every change of the floor | 7 of 10 | 0 of 10 | 3 of 10 |

So while the floor is on, `start` and `changes` both send a note at every change of the
floor (the caller already named in the call context gets no second one), with the hold;
`off` sends none. The rest below is the original plan.

- When the floor passes, the gate emits the new holder's name through the existing
  `effects.speakers` → `speakers_changed` path:
  - with labels at `changes`: at every floor change;
  - with labels at `start`: only for a person who waited;
  - with labels at `off`: never.
  `labels_for` and its 400 ms window are replaced by this rule.
- The first frame of the new holder's buffer is held until `thread/realtime/appendText`
  for the note has returned, or for at most 250 ms. That way the note reaches Codex before
  the speech it names.
- The caller of an activation is named by the call context, as now, so no note is sent at
  activation.

[decided alone while building] The hold is a fixed `NOTE_HOLD_TICKS` = 12 receive ticks
(240 ms) whenever the labels are not `off`, not "until the reply": the reply of
`appendText` is seen only by the Codex session, and a path back to the gate for one value
would add a lock across the two for no measured gain. The Codex session now sends a note
whenever the gate asks and the labels are not `off` (before, only at `changes`); with the
switch off the gate asks only at `changes`, so nothing changes there.

Probe 1 checks where the note lands relative to the two voices. If the note lands after the
speech even with the wait, the hold becomes "until the reply, at most 500 ms" and the report
says so. If that still fails, the note is sent at the end of the previous holder's speech
instead, which is the same moment for the model.

### D6. The pre-roll per speaker **[decided alone]**

- The gate keeps the last `PREROLL_FRAMES` of each speaker heard in the last 5 s, in a ring
  per SSRC, instead of a ring of the mix. A speaker absent for 5 s gives their ring back.
- On activation the caller (the SSRC `on_phrase` reports) takes the floor. Their ring, from
  the end of the phrase, becomes `replay`.
- Every other ring's frames from the same point move into that person's waiting buffer, in
  the order those people started talking.
- The rest (the readiness hold, `MAX_REPLAY_FRAMES` and the catch-up) is unchanged.
- In always mode there is no activation: the floor starts free.

### D7. Probes before the build

Two probes, results in `docs/SPEAKER-TURNS-PROBES.md`:

1. **The model's turn end and the note.** This is an extension of `live_call_when_enabled`
   with two speaker WAVs, a gap and an optional note:
   - it feeds A, then N ms of silence, then the note through `appendText`, then B;
   - N is 100, 200, 300, 500, 700 and 1000 ms, three runs each;
   - it records whether the model answered before B started, and the order of the people's
     transcript pieces and of the note in the item events.

   Output: the floor pause (D2) and the note hold (D5).
2. **`turn/steer`.** This is an extension of the Whisper text-turn test, or a probe with the
   real app-server:
   - it starts a turn with question A and, after `turn/started` and before
     `turn/completed`, steers question B into it;
   - it records whether the answer covers both, the time to first words, the error when
     steering a completed turn, and the same two questions sent as two turns in a row.

   Output: D6' below.

Test material: two 48 kHz mono 16-bit WAVs with different Windows voices ("What time is it
in Bangkok?" and "And what is the weather in London?"), and a mixed copy with about 2 s of
overlap for the baseline of the current mixed call.

### D6'. The Whisper session: join the running turn, never interrupt

- `user_text` no longer calls `stop_running` and no longer raises the player's epoch.
- With a turn running and its id known, the question goes to `turn/steer` with
  `expectedTurnId`.
- With a turn starting (the `turn/start` request in flight), the question waits for the id
  under the existing `turn_lock`, then is steered.
- When steer fails (the turn completed meanwhile, or a non-steerable turn), the question
  starts the next turn.
- With no turn running, the question starts a turn, as now. Its answer's sentences queue in
  the player behind the current answer's, under the same epoch.
- A turn keeps the list of its questions:
  - the answer posting's task line joins them with " / ";
  - the chat log gets each one;
  - the latency line is measured from the first question, and every added question logs one
    line "question added to turn <n>".
- Deactivation, mute, `/leave`, the idle timeout and a mode change still interrupt and flush
  as before.
- [decided alone while building] Questions that wait start the next turn together, joined
  by line breaks, when the running turn completes; their answers queue in the voice's FIFO
  behind the current answer. Since only one turn runs at a time and a turn's sentences are
  all queued before its `turn/completed`, two turns' sentences cannot interleave, so the
  player needs no change (task 5.2 holds by construction; the TTS queue is FIFO).

Fallback if probe 2 shows that Codex answers a steered question badly (it ignores it,
answers only the last one, or takes over 2 s longer than two turns): steer is not used. A
question that comes during a running turn waits and starts the next turn when the running
one completes. Nothing is interrupted in either case, so the spec delta holds; only its
first scenario's wording ("added to it") changes to "answered next", and the report records
the choice.

### D8. Counters

`GateCounters` gets `floor_passes`, `floor_dropped` and a `floor_waiting` gauge, shown in
the status like the other gate counters and in the gate's status line in the log.

## Risks / Trade-offs

- [The model ends turns faster than any usable floor pause] → D4 keeps the waiting speech
  from cutting an answer, so the worst case is two answers in a row instead of one answer
  to both. Probe 1 measures it before the build.
- [A long overlap delays the second person by up to 15 s plus the answer] → The buffer cap
  bounds it. The skipped silent frames shorten it. The report measures the latency against
  the done criterion (no more than 300 ms worse than the mixed call for the first audio).
- [Two voices through one microphone still reach the call together] → Out of scope. The
  user guide says so.
- [`turn/steer` changes in a later Codex version] → It is marked experimental in the
  schema. Probe 2 runs against the installed version. The fallback of D6' needs no steer.
- [Memory] → 5 s of pre-roll and 15 s of buffer per person, as stereo 48 kHz frames
  (3840 B), is about 4 MB per person who talks. Rings are given back after 5 s of absence
  and buffers when they drain.
- [Answers now queue behind each other in Whisper mode] → Someone who wants the assistant
  to stop uses the deactivation phrase or Mute. The user guide says so, since talking over
  the voice no longer stops it.

## Migration Plan

A file without `assistant.one_speaker_at_a_time` loads with it off, so nothing changes until
the user turns it on. Rolling back is reverting the change; an older build keeps the field
as an unknown one.
