# call-latency Specification

## Purpose
Measures how long the assistant takes to answer each turn and where the time goes, as the
first version did, so slow answers can be traced to the model, the call or the playback.

## Requirements

### Requirement: Turns are detected from speech toward the assistant
A turn SHALL start when speech is sent toward the assistant after at least 350 ms without
speech; later speech within the same turn SHALL update the turn's last speech time.

#### Scenario: Two sentences with a short pause
- **WHEN** a member speaks, pauses for 200 ms and speaks again
- **THEN** both belong to one turn and its last speech time is the end of the second sentence

### Requirement: Each answer is measured
For the first answer audio after a turn, within 15 s, the app SHALL measure first speech to
answer audio, last speech to answer audio, answer audio to playback in Discord, and first
speech to playback, and SHALL write them as one log line with the turn number.

#### Scenario: A normal answer
- **WHEN** a question is asked and the assistant answers
- **THEN** one log line names the turn and the four durations in milliseconds

#### Scenario: Answer without a question
- **WHEN** the assistant speaks with no turn in the last 15 s
- **THEN** the line reports an autonomous answer with its playback delay only

### Requirement: Interruptions are reported
Speech toward the assistant while its answer is playing SHALL be reported once per answer as
an interruption, and the next answer SHALL be labelled as a response after interruption.

#### Scenario: Member talks over the answer
- **WHEN** a member speaks while the assistant is answering
- **THEN** one line says the answer was interrupted, and the next answer's line says "response after interruption"

### Requirement: Recent turns are in the status
The status SHALL carry the measurements of the last 50 turns with their times.

#### Scenario: Reading the turns
- **WHEN** the window reads the status after three answered turns
- **THEN** it gets three entries with the four durations each
