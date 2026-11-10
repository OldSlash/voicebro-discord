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
A session SHALL end after `session.idle_timeout_seconds` without input, where the
assistant's own audio counts as input; 0 disables the timeout.

#### Scenario: Long answer
- **WHEN** the assistant speaks for longer than the timeout without anyone else talking
- **THEN** the session stays active until the assistant finishes and the timeout then elapses

#### Scenario: Silence
- **WHEN** nobody and nothing has been heard for the timeout
- **THEN** the session ends, the falling earcon plays, and the assistant is told the session is no longer wanted

### Requirement: Deactivation is immediate
On a deactivation phrase the gate SHALL mute the assistant's output and flush what is
queued at once, then tell the assistant the session is not wanted.

#### Scenario: Deactivation mid-answer
- **WHEN** a member says the deactivation phrase while the assistant is speaking
- **THEN** the assistant's voice stops within one frame and the rest of the answer is never heard

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
