## RENAMED Requirements

- FROM: `### Requirement: Both modes share Codex and the session rules`
- TO: `### Requirement: Both modes share the session rules`

- FROM: `### Requirement: Questions are Codex text turns`
- TO: `### Requirement: Questions are text turns of the selected engine`

## MODIFIED Requirements

### Requirement: Two assistant modes
The setting `assistant.mode` SHALL choose how people talk to the assistant:

- `realtime` (the default): Vosk spots the phrases and a Codex realtime voice call hears the
  channel and speaks; only Codex offers it;
- `whisper`: Whisper turns speech into text and spots the phrases, the selected engine
  (Codex, Claude Code or OpenCode) answers in text, and a local voice speaks the answer.

#### Scenario: A new settings file
- **WHEN** the app starts with no settings file
- **THEN** the assistant mode is realtime and the app behaves as before this change

#### Scenario: Whisper with Claude Code
- **WHEN** the engine is Claude Code and the assistant mode is Whisper
- **THEN** Whisper spots the phrases, Claude Code answers in text and the local voice speaks

### Requirement: Both modes share the session rules
Both modes SHALL use the selected engine with the same sign-in, conversation,
instruction, speaker names and answer posting. The session mode (always or wake word), the
idle timeout, the earcons, the mute and the access filter SHALL work in both.

#### Scenario: Owner-only access in Whisper mode
- **WHEN** owner-only access is on and the assistant mode is Whisper
- **THEN** only the linked owner's voice can start a session or ask a question, as in realtime mode

### Requirement: Questions are text turns of the selected engine
In Whisper mode each question SHALL be sent to the guild's conversation with the selected
engine as a text turn, or, while one speaker at a time is on, handed over as requirement
"Questions wait their turn" says. While speaker names are on, the text SHALL be prefixed
with the speaker's display name. The first turn of each session SHALL carry the people in
the voice channel and the caller as context.

#### Scenario: Speaker named
- **WHEN** Alex asks "what time is it in London" during a session with speaker names at call start
- **THEN** the engine gets "Alex: what time is it in London"

#### Scenario: Added question named
- **WHEN** one speaker at a time is on and Sam's question is handed to the engine while it works, with speaker names at call start
- **THEN** the engine gets it as "Sam: " followed by the question

### Requirement: Whisper mode adds no instruction of its own
In Whisper mode the app SHALL add no instruction of its own: the instruction the engine
gets from the app SHALL be the user's `assistant.instructions` only.

#### Scenario: Developer instructions
- **WHEN** a question is sent to Codex in Whisper mode
- **THEN** the thread's developer instructions are the user's `assistant.instructions` and nothing else

#### Scenario: Claude Code's instruction
- **WHEN** a question is sent to Claude Code
- **THEN** the only instruction the app added to Claude Code's own system prompt is the user's `assistant.instructions`

### Requirement: Questions wait their turn
While `assistant.one_speaker_at_a_time` is on, a question during a session SHALL NOT
interrupt a running turn or stop the voice. A question that comes while the engine works on
a turn SHALL be handed to the engine without interrupting it: Codex adds it to the running
turn, OpenCode takes it up at its next step, and Claude Code answers it in the next turn. A
question that comes while an answer is spoken SHALL start the next turn. Answers SHALL be
spoken in the order of their turns.

#### Scenario: Second question while Codex works
- **WHEN** one speaker at a time is on, the engine is Codex, Alex asks "What time is it in Bangkok?", and Sam asks "And what is the weather in London?" while Codex is still working on Alex's question
- **THEN** Alex's turn is not interrupted, Sam's question is added to it, and the answer covers both questions

#### Scenario: Follow-up while the answer plays, one at a time
- **WHEN** one speaker at a time is on and a member asks a new question while the voice is still reading the previous answer
- **THEN** the voice reads the previous answer to its end, and the answer to the new question follows it

#### Scenario: The running turn ends at the same moment
- **WHEN** one speaker at a time is on and a question comes just as the engine finishes the running turn and cannot add it there
- **THEN** the question starts the next turn and is answered
