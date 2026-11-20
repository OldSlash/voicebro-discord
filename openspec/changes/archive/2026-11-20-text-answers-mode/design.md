# Design

## Context

See proposal.md for the request. Today `AssistantMode` has `Realtime` and `Whisper`, and
the core checks `== AssistantMode::Whisper` in about twenty places: the gate
(`GateSettings::whisper`), the runtime (join preparation, gate rebuilds), the Whisper and
voice managers (`configure`, `prepare_join`), the wake manager (off in the Whisper mode) and
the three engines (Codex's text session, Claude Code and OpenCode only in the Whisper mode).
The text session (`assistant/text.rs`) splits each delta into sentences and queues them
for the voice (`TextSession::speak`); a completed turn hands its last agent message to
answer posting (`AgentAnswer`). `runtime::answer_destination` maps
`assistant.post_answers` to a destination, `None` for `off`. The latency line of a Whisper
turn is written in `latency.rs`.

## Goals / Non-Goals

**Goals:**
- The text answers mode behaves as the Whisper mode in every respect but the voice.

**Non-Goals:**
- No questions typed in a text chat: the input stays the voice channel.
- No change to the earcons: they play in this mode as in the Whisper mode.
- No new posting destination and no change to how answers are formatted or split.

## Decisions

### D1. A third mode value with two questions on it

`AssistantMode::Text` (`"text"`) is added, with two methods that replace the comparisons:

- `hears_with_whisper()`: `Whisper` or `Text`; used wherever the code means "Whisper
  recognizes and the engine answers in text" (the gate, the Whisper manager, the wake
  manager, the engines, the runtime's gate rebuild and join);
- `speaks_locally()`: `Whisper` only; used by the voice manager and the text session.

Every existing `== AssistantMode::Whisper` is changed to one of the two, so no place is left
that silently treats the new mode as realtime. A switch between `whisper` and `text` is a
mode change like any other: the session ends with the falling tone and the gates are
rebuilt, as "The mode switches at once" says.

Alternative: keep two modes and add a flag `assistant.speak_answers`. Rejected: the user
asked for a separate mode, the flag would mean nothing in the realtime mode, and `/status`,
the panel and the Status tab would have to combine two settings to name the mode.

### D2. No voice

In the text answers mode the voice manager behaves as in the realtime mode: its state is
off, nothing is loaded, and `missing` is still computed for the Settings card.
`Runtime::prepare_whisper` asks only the Whisper manager, so a missing voice does not refuse
the join. The text session gets the mode with its options and, while the mode does not speak
locally, skips `speak`; the splitter is not fed. Stopping a turn still flushes the player,
which holds nothing.

### D3. Posting with posting off

`answer_destination` returns `AnswerDestination::VoiceChannel` for `PostAnswers::Off` while
the mode is `text`. Everything after that (the session's channels, formatting, splitting,
mentions, refusals) is the posting code of today.

### D4. The latency line

For a turn in the text answers mode the line keeps the time to the text, the audio length,
model and device, and the time to the engine's first words, and ends with "no voice"
instead of the two voice times. The status's recent turns carry the voice times as absent.

### D5. Names and the window

`discord::MODE_TEXT` = "Whisper mode, text answers" for `/status` and the member panel. The
window adds the third option to the mode list, treats `text` like `whisper` for the rows it
shows (`data-mode="whisper"`), and follows the spec deltas for the Recognition field, the
Local voice note, the posting help line, the Status tab's voice tile, the Config note (only
`whisper.missing`) and the refused join's dialog (only `whisper.missing`, Download calling
only `whisper_download`). The window recognizes a refused join in this mode by the same
reply prefix as the Whisper mode's ("The Whisper mode needs downloads"), followed by the
text of requirement "Join prepares the text answers mode" (Download in the Whisper card
only).

## Risks / Trade-offs

- [A place still compares with `AssistantMode::Whisper` after the change] → Task 1.1
  replaces every comparison and a search for `AssistantMode::Whisper` outside tests and the
  two methods must find nothing.
- [Posting fails (no permission in the voice channel's chat) and the answer is lost] → A
  refused post already shows as the `answers` connection error on the Status tab; the chat
  tab still shows the answer.
- [The earcons are the only sound in this mode] → Unchanged behaviour; the Earcons switch
  turns them off.
