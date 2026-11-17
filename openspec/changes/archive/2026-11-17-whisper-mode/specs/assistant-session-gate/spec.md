# Spec Delta

## ADDED Requirements

### Requirement: The gate in Whisper mode
In Whisper mode, whenever mute is off, the gate SHALL hand each speaker's frames and one
tick per receive tick to the Whisper feed, in both session modes. It SHALL send no frames
to the assistant's input and SHALL not use the pre-roll or the replay.

#### Scenario: Waiting in Whisper mode
- **WHEN** the bot is in a channel in Whisper mode, wake-word activation, no session
- **THEN** every speaker's audio reaches Whisper, nothing reaches the assistant's input, and the assistant's output is muted

#### Scenario: Muted in Whisper mode
- **WHEN** mute is on in Whisper mode
- **THEN** no audio reaches Whisper, and nothing is transcribed

### Requirement: The gate takes phrases and questions from Whisper's text
In Whisper mode the gate SHALL take phrases and questions from the transcripts
(requirements "Phrases in Whisper's text", "Activation phrase in Whisper mode" and
"Deactivation phrase in Whisper mode" of `whisper-assistant-mode`) and SHALL hand each
question to the assistant session with the speaker's name. Activation and deactivation
SHALL count only in their state, as in realtime mode, and the same tones SHALL play.

#### Scenario: Question handed on with the speaker's name
- **WHEN** a session is active in Whisper mode and a member's utterance holds a question
- **THEN** the gate hands the question to the assistant session with that member's name

## MODIFIED Requirements

### Requirement: Idle timeout
A session SHALL end after `session.idle_timeout_seconds` without input. The assistant's own
audio SHALL count as input, and so SHALL a turn the assistant is still working on in Whisper
mode. 0 disables the timeout.

#### Scenario: Long answer
- **WHEN** the assistant speaks for longer than the timeout without anyone else talking
- **THEN** the session stays active until the assistant finishes and the timeout then elapses

#### Scenario: Silence
- **WHEN** nobody and nothing has been heard for the timeout
- **THEN** the session ends, the falling earcon plays, and the assistant is told the session is no longer wanted

#### Scenario: Codex still working in Whisper mode
- **WHEN** a question was sent in Whisper mode and Codex works on it longer than the timeout
- **THEN** the session stays active until the answer has been spoken and the timeout then elapses
