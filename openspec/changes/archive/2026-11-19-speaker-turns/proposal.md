# Proposal

## Why

When two people talk at once, Codex gets them tangled. In the realtime mode the gate mixes
every speaker into the one audio track of the call, so overlapped questions reach the model
as one voice, and the model cannot tell who asked what. In the Whisper mode each person's
utterance is already a separate text turn with a name, but a question that arrives while
Codex answers interrupts that answer. The user wants people's speech to reach Codex one
person after another, each with a name, in both modes (the user's request of 2026-11-18,
task `docs/TASK-CONSENT-AND-TURNS.md`, part 2). It must stay on Codex with the user's
existing sign-in: no new engine, no paid service, no add-on.

## What Changes

- **Optional, off by default.** A new switch, "One speaker at a time"
  (`assistant.one_speaker_at_a_time`, Config → Assistant), turns everything below on, in
  both modes. While it is off, the call gets the mix and a new question interrupts the
  answer, as before this change (the user's decision of 2026-11-18: every such mode is
  optional).
- **Realtime mode: one speaker at a time toward the call.** The first person to speak has
  the floor and goes to the call live, as now. Whoever talks while someone else has the
  floor is not mixed in: their speech waits in a buffer of their own, up to 15 s. When the
  speaker with the floor has been silent for 300 ms, the floor passes to the person who has
  waited longest, and their buffer goes to the call at the normal pace. A buffer waits while
  the assistant's voice plays, so it does not cut the answer.
- **A name before each passed floor.** Before a waiting person's speech, the call gets the
  developer note "now speaking: <name>" through the path the speaker notes already use. With
  speaker labels at `start` (the default) the note comes only when the floor passes to
  someone who waited; at `changes` it comes at every change of the floor; at `off` never.
- **The pre-roll keeps each speaker apart.** On activation the caller's own speech after
  the phrase is replayed; what others said meanwhile goes into their buffers.
- **Whisper mode: questions wait their turn.** A question that arrives while Codex works on
  an answer joins that turn (`turn/steer`). A question that arrives while the answer is being
  spoken starts the next turn, and its answer is spoken after the current one. Nothing
  interrupts a running answer any more, except the deactivation phrase, Mute, `/leave`, the
  idle timeout and a mode change. With the switch on, a new question no longer stops the
  voice; with it off, "barge-in on words" works as before.
- **Probes first.** Before any of this is built, two live probes measure what the design
  depends on: after how much silence the realtime model ends a turn, and where a note sent
  between two people's speech lands; and how Codex answers a question added with
  `turn/steer`. Their results go into a report under `docs/` and set the values or the
  fallbacks of the design.
- **New counters** in the gate status: people waiting for the floor, floor passes, and
  frames dropped from full buffers.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `assistant-session-gate`: the switch, the floor toward the call, the per-person buffers
  and their counters; the pre-roll kept per speaker while on; the call context reports
  floor changes while on (its block also carries the access-mode names rule of change
  `voice-consent`, which modifies the same requirement).
- `assistant-conversation`: speaker labels send a note when the floor passes while on.
- `whisper-assistant-mode`: "Barge-in on words" holds while off; the new "Questions wait
  their turn" while on; a question may join the running turn.
- `app-settings`: the new setting `assistant.one_speaker_at_a_time`.
- `settings-ui`: the switch in the Assistant card; the list of switches and the field list
  (both blocks also carry the Access changes of `voice-consent`).

## Impact

- **Code:** `crates/voicebro-core/src/gate.rs` (the floor, the buffers, the per-speaker
  pre-roll, the counters); `assistant/codex/session.rs` and `conversation.rs` (the note at a
  passed floor, sent before the buffer is released); `assistant/codex/text_session.rs`
  (`turn/steer`, no interrupt, answers spoken in order); `tts/player.rs` only if a new answer
  must queue behind the current one instead of flushing it.
- **Tests:** unit tests for the floor and the buffers on recorded per-speaker frames; the
  Whisper session against the fake Codex (`testdata/fake-codex`) for steer, the next turn
  and the missing interrupt; the live call test fed two overlapping speakers; the Whisper
  text-turn test with two questions.
- **Docs:** `docs/ARCHITECTURE.md` (the gate, the Codex sessions), `AGENTS.md` (the audio
  facts on overlapping speakers), `docs/user-guide.md` (what happens when two people talk),
  a probe report under `docs/`.
- **Settings and window:** `settings/mod.rs` (the field), the Assistant card's switch in
  `ui/index.html`, `ui-dev/mock.js` and `ui-dev/check-config-tab.mjs`.
- **Not affected:** the access filter of change `voice-consent` runs before the gate, so it
  works the same with the floor. Discord commands and the wake engine do not change.
