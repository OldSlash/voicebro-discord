# Design

## Context

See proposal.md for the request. In the Whisper mode the gate (`gate.rs`) gets each receive
tick's mix and the frames of every speaker the access filter lets through (filtered in
`discord/receive.rs`). It already tracks, per speaker, the last voiced frame (peak at or
above `VOICE_PEAK_THRESHOLD`, 700) and starts a new utterance after a gap of 700 ms
(`UTTERANCE_GAP`). The frames also go to Whisper's cutter, which ends an utterance after 35
silent ticks and hands it to the transcriber; the text reaches the gate as Whisper text and,
during a session, the text session as a question (`AssistantSession::user_text`). The text
session's barge-in is `stop_running` (flush the player, drop waiting questions, mark the
turn stale) followed by `backend.interrupt`. The gate knows whether the assistant is working
(`AssistantSession::working`: a turn runs or the player holds speech).

## Goals / Non-Goals

**Goals:**
- Stop the spoken answer 600 ms into a person's speech, without waiting for Whisper.

**Non-Goals:**
- No speech classifier: only the duration and the existing loudness threshold decide. The
  user did not agree to another model for this.
- No resume of a stopped answer; no adjustable threshold.
- No change to the realtime mode or the text answers mode.

## Decisions

### D1. The gate counts speech per speaker and utterance

Each gate speaker gets `voiced_frames` (frames at or above the threshold since its utterance
started) and `stopped_answer` (this utterance already stopped an answer); both reset when a
new utterance starts (the existing gap test). The count is one addition per voiced frame
and always runs; only the check of D2 acts on it. `INTERRUPT_AFTER_FRAMES` = 30 frames of
20 ms.

Alternative: count in Whisper's cutter, which also counts voiced frames. Rejected: the
cutter runs on the transcriber's side for every guild, while the session state (active,
muted, working) and the effects live in the gate.

### D2. When it acts

The gate asks for the stop when all hold: the mode hears with Whisper and speaks locally
(the Whisper mode, not the text answers mode), `assistant.interrupt_on_speech` is on,
`assistant.one_speaker_at_a_time` is off, the gate is not muted, a session is active (wake
word activated, or always listening), the assistant is working, the speaker's
`voiced_frames` reached 30 and `stopped_answer` is false. It then sets `stopped_answer` and
a new effect `stop_answer`, and logs "<name> spoke for 600 ms: the answer stopped". The
settings are read per tick from the gate's settings, which a save updates without a
rebuild.

### D3. The session stops the answer without starting a turn

`AssistantSession` gets `stop_answer()`, a no-op by default (the realtime call leaves this
to its server). The text session implements it as its barge-in without the new turn:
`stop_running` and, when a turn id comes back, `interrupt`. The stopped turn posts nothing,
as an interrupted turn already does. The utterance goes on to Whisper untouched; its text
arrives later as a question and starts a turn as usual, or, when Whisper finds no words,
nothing follows.

### D4. Why 600 ms

The cutter already drops less than 240 ms of voiced audio as noise. A sneeze or a click is
well under 600 ms of voiced frames; "wait" with the next words passes it. The value is a
constant the user asked to be sensible, not a field; the gated corpus check in the tasks
measures it on real speech and the noise clips.

## Risks / Trade-offs

- [A long cough, laughter or loud talk among the players stops the answer] → The setting is
  off by default and the user turns it on knowingly; the help line says what stops it.
- [A steady loud sound in someone's microphone stops the answer] → Measured on 2026-11-20
  (task 1.4): the `whisper-corpus` game-like clip, whose 70 Hz hum stays above the speech
  threshold, stopped the answer after 600 ms, as a sentence does; silence and steady noise
  of peak 300 did not. Discord's own noise suppression and voice activity usually keep such
  a sound out of the channel; where they do not, the setting can stay off. A speech
  classifier would tell them apart, which the user did not want for this change.
- [Someone's microphone picks up the bot's voice from their speakers] → Discord's echo
  cancellation normally removes it; otherwise the answer stops itself, and the setting can
  be turned off.
- [Saying the activation phrase again during a session for 600 ms or more stops the answer]
  → Expected: it is speech during an answer.
