# assistant-session-gate Specification

## Purpose
Decides when the assistant hears the channel and when it stops, so the assistant is engaged
only when called and the question after the wake phrase is never lost.

## Requirements

### Requirement: Two modes
In `always` mode a session SHALL be wanted whenever the bot is in a voice channel and mute
is off. In `wake_word` mode a session SHALL start only on an activation phrase while mute is
off and end on a deactivation phrase, the idle timeout or mute.

#### Scenario: Always mode
- **WHEN** the bot joins a channel in always mode
- **THEN** the assistant session is wanted at once and the channel audio flows to it

#### Scenario: Wake-word mode, nobody called
- **WHEN** the bot is in a channel in wake-word mode and no phrase has been said
- **THEN** no audio reaches the assistant and the assistant's output is muted

#### Scenario: Always mode, muted
- **WHEN** mute is turned on in always mode
- **THEN** the session is no longer wanted, and turning mute off makes it wanted again

### Requirement: Activation and deactivation count only in their state
An activation phrase SHALL count only while no session is active; a deactivation phrase
SHALL count only while a session is active. Repeating the activation phrase during a session
SHALL change nothing.

#### Scenario: Activation during a session
- **WHEN** someone says an activation phrase while a session is active
- **THEN** the session continues unchanged and no new call is started

### Requirement: Pre-roll replay from the end of the phrase
The gate SHALL keep about 5 s of mixed audio. On activation it SHALL replay audio starting
at the end of the activation phrase (from the recognizer's timing, or 1 s before the hit
plus an 80 ms margin), hold the replay while the assistant is not ready, catch up by skipping
silent frames, and cap the replay at about 6 s.

#### Scenario: Question right after the phrase
- **WHEN** a member says the activation phrase and a question in one breath, and the assistant takes 2 s to become ready
- **THEN** the assistant receives the question and not the phrase, and the live audio continues after the replay

#### Scenario: Assistant never becomes ready
- **WHEN** the assistant does not become ready within the replay cap
- **THEN** the held audio is dropped, the drop is logged, and the session ends with an error in the status

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

### Requirement: Deactivation is immediate
On a deactivation phrase, from the wake engine or from the call's transcript, the gate
SHALL mute the assistant's output and flush what is queued at once, play the falling
earcon when earcons are on, then tell the assistant the session is not wanted.

#### Scenario: Deactivation mid-answer
- **WHEN** a member says the deactivation phrase while the assistant is speaking
- **THEN** the assistant's voice stops within one frame, the rest of the answer is never heard, and the falling earcon is heard

#### Scenario: Earcons off
- **WHEN** earcons are off and a member says the deactivation phrase while the assistant is speaking
- **THEN** the assistant's voice stops within one frame and no tone plays

### Requirement: Call context for the assistant
Before a session starts the gate SHALL give the assistant the caller's name (if a phrase
started it) and the participant names; when the set of people talking changes it SHALL
report the new set.

#### Scenario: Session start
- **WHEN** a member starts a session with the activation phrase
- **THEN** the assistant receives that member's name as the caller and the current participants

### Requirement: Gate status
The status SHALL show per guild: mode, whether listening for the wake word, whether a
session is active, the last phrase heard, and the gate's drop counters.

#### Scenario: Status during a session
- **WHEN** the UI reads the status during a session
- **THEN** it shows active and the phrase that started it

### Requirement: Deactivation from the call's transcript
In wake-word mode, while a session is active and mute is off, the gate SHALL check the
call's transcript of the people's speech, joined per turn, for the selected wake model's
deactivation phrases. Words match whole, ignoring case and punctuation. A turn that ended
less than 2 s before SHALL be read with the next one, and a match SHALL end in the newer
turn. A match SHALL end the session as the wake engine's deactivation does, and the log
SHALL name the transcript as the source.

#### Scenario: The phrase cut into two turns
- **WHEN** the deactivation phrase is "отбой братан" and the call's transcript ends one turn with " Отбой" and starts the next with ", братан" 0.2 s later
- **THEN** the session ends

### Requirement: A phrase at the end of the transcript waits for its word to end
A deactivation phrase whose last word is the last word of the transcript so far, with
nothing after it, SHALL count once the text shows the word ended: the next piece starts
with a space or punctuation, the turn ends, or 300 ms pass without a newer piece. While it
waits, the assistant's voice SHALL be silenced, and it SHALL come back if the word goes on.

#### Scenario: Held phrase confirmed
- **WHEN** the deactivation phrase is "goodbye" and a turn's transcript so far is " Goodbye", with nothing after it
- **THEN** the voice is silenced at once and the session ends 300 ms later

#### Scenario: The wake engine misses the phrase during an answer
- **WHEN** a member says the deactivation phrase over the assistant's answer, the wake engine does not confirm it, and the call's transcript of that speech holds it
- **THEN** the assistant's voice stops within one frame of the match, the falling earcon plays, the call stops, and the log line names the call's transcript

#### Scenario: The phrase arrives in pieces
- **WHEN** the deactivation phrase is "goodbye" and the transcript of one turn arrives as " Good" and then "bye", and nothing follows within 300 ms
- **THEN** the session ends without waiting for the end of the turn

#### Scenario: A longer word is not the phrase
- **WHEN** the deactivation phrase is "stop" and the transcript of one turn arrives as " stop" and then "watch"
- **THEN** the session continues, and the voice, silenced after " stop", plays again after "watch"

#### Scenario: The voice replies while the phrase is held
- **WHEN** the transcript of one turn ends with " Отбой, братан" and the voice starts its reply 100 ms later
- **THEN** nothing of the reply is heard, and the session ends with the falling earcon 300 ms after that piece

#### Scenario: Punctuation and case
- **WHEN** the deactivation phrase is "goodbye" and a turn's transcript reads "Okay, GOODBYE!"
- **THEN** the session ends

#### Scenario: Both sources hear the phrase
- **WHEN** the wake engine confirms the deactivation phrase and the call's transcript holding it arrives afterwards
- **THEN** the session ends once and the falling earcon plays once

#### Scenario: The wake word right after the phrase
- **WHEN** a member says the deactivation phrase followed by an activation phrase, the call's transcript ends the session, and the wake engine confirms the activation phrase 0.8 s later
- **THEN** no new session starts

#### Scenario: Always mode
- **WHEN** the session mode is always and the transcript holds a deactivation phrase
- **THEN** the session continues

#### Scenario: Phrases of another model
- **WHEN** only a model other than the selected one has deactivation phrases stored and the transcript holds one of them
- **THEN** the session continues

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
