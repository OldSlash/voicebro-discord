## MODIFIED Requirements

### Requirement: Barge-in on words
While `assistant.one_speaker_at_a_time` is off, a new question during a session SHALL
interrupt a running turn and stop the voice at once, and SHALL then start a new turn. While
it is on, requirement "Questions wait their turn" SHALL apply instead. Sound without words
SHALL interrupt nothing in either case.

#### Scenario: Follow-up while the answer plays
- **WHEN** one speaker at a time is off and a member asks a new question while the voice is still reading the previous answer
- **THEN** the voice stops, the previous turn is interrupted, and the answer to the new question follows

#### Scenario: Cough during an answer
- **WHEN** a member coughs while the voice speaks
- **THEN** the answer goes on

### Requirement: Questions are Codex text turns
In Whisper mode each question SHALL be sent to the guild's Codex thread as a text turn, or,
while one speaker at a time is on, added to the running turn as requirement "Questions wait
their turn" says. While speaker names are on, the text SHALL be prefixed with the speaker's
display name. The first turn of each session SHALL carry the people in the voice channel
and the caller as context.

#### Scenario: Speaker named
- **WHEN** Alex asks "what time is it in London" during a session with speaker names at call start
- **THEN** Codex gets "Alex: what time is it in London"

#### Scenario: Added question named
- **WHEN** one speaker at a time is on and Sam's question is added to a running turn with speaker names at call start
- **THEN** Codex gets it as "Sam: " followed by the question

## ADDED Requirements

### Requirement: Questions wait their turn
While `assistant.one_speaker_at_a_time` is on, a question during a session SHALL NOT
interrupt a running turn or stop the voice. A question that comes while Codex works on a
turn SHALL be added to that turn; one that comes while an answer is spoken SHALL start the
next turn. Answers SHALL be spoken in the order of their turns.

#### Scenario: Second question while Codex works
- **WHEN** one speaker at a time is on, Alex asks "What time is it in Bangkok?", and Sam asks "And what is the weather in London?" while Codex is still working on Alex's question
- **THEN** Alex's turn is not interrupted, Sam's question is added to it, and the answer covers both questions

#### Scenario: Follow-up while the answer plays, one at a time
- **WHEN** one speaker at a time is on and a member asks a new question while the voice is still reading the previous answer
- **THEN** the voice reads the previous answer to its end, and the answer to the new question follows it

#### Scenario: The running turn ends at the same moment
- **WHEN** one speaker at a time is on and a question comes just as Codex finishes the running turn and cannot add it there
- **THEN** the question starts the next turn and is answered
