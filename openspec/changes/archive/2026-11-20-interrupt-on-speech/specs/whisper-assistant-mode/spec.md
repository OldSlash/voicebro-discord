# Spec Delta

## MODIFIED Requirements

### Requirement: Barge-in on words
While `assistant.one_speaker_at_a_time` is off, a new question during a session SHALL
interrupt a running turn and stop the voice at once, and SHALL then start a new turn. While
it is on, requirement "Questions wait their turn" SHALL apply instead. While
`assistant.interrupt_on_speech` is off, sound without words SHALL interrupt nothing in
either case; while it is on, requirement "The answer stops when someone starts talking"
SHALL apply as well.

#### Scenario: Follow-up while the answer plays
- **WHEN** one speaker at a time is off and a member asks a new question while the voice is still reading the previous answer
- **THEN** the voice stops, the previous turn is interrupted, and the answer to the new question follows

#### Scenario: Cough during an answer
- **WHEN** stopping the answer at the start of speech is off and a member coughs while the voice speaks
- **THEN** the answer goes on

## ADDED Requirements

### Requirement: The answer stops when someone starts talking
In the Whisper mode, while `assistant.interrupt_on_speech` is on and one speaker at a time
is off, when one person's speech in an utterance reaches 600 ms of frames at or above the
speech threshold during a session, while a turn runs or the voice speaks, the voice SHALL
stop at once and the running turn SHALL be interrupted, once per utterance. The utterance
SHALL still be written down and SHALL be the next question as usual.

#### Scenario: "Wait" and a long phrase
- **WHEN** the setting is on, the voice reads an answer, and a member says "Wait, I meant the other server" without a pause
- **THEN** the voice stops about 600 ms after the member started talking, before the phrase ends, the turn is interrupted, and the phrase becomes the next question when Whisper has written it down

#### Scenario: A sneeze
- **WHEN** the setting is on and a member sneezes for 300 ms while the voice speaks
- **THEN** the answer goes on

#### Scenario: Outside a session
- **WHEN** the setting is on, the activation is the wake word, no session is active, and a member talks for two seconds
- **THEN** nothing is stopped

#### Scenario: Nothing to stop
- **WHEN** the setting is on during a session, no turn runs and the voice is silent, and a member talks for two seconds
- **THEN** nothing is interrupted, and the utterance becomes a question as usual

#### Scenario: One speaker at a time
- **WHEN** the setting is on, one speaker at a time is on, and a member talks for two seconds while the voice speaks
- **THEN** the answer goes on, as requirement "Questions wait their turn" says

#### Scenario: Off
- **WHEN** the setting is off and a member talks for two seconds while the voice speaks
- **THEN** the voice stops only when Whisper has written the utterance down and it is a question

#### Scenario: Logged
- **WHEN** the answer is stopped at the start of speech
- **THEN** the log has a line naming the speaker and saying the answer stopped after 600 ms of speech
