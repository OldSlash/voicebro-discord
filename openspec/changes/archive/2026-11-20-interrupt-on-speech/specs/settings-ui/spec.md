# Spec Delta

## ADDED Requirements

### Requirement: Stop the answer at the start of speech in the Assistant card
In the Whisper mode the Assistant card SHALL show the switch "Stop the answer when someone
starts talking", off by default and saved on change, after "One speaker at a time". Its
help line SHALL say that while on the answer stops once a person has talked for 600 ms,
that shorter sounds stop nothing, and that while off the answer stops when the question has
been recognized. In the realtime and text answers modes the switch SHALL not show.

#### Scenario: Turned on
- **WHEN** the mode is Whisper and the user clicks the switch
- **THEN** it reads "On" and `assistant.interrupt_on_speech` is saved as `true` without a button

#### Scenario: Realtime mode
- **WHEN** the mode is realtime voice
- **THEN** the Assistant card does not show the switch

### Requirement: The switch says when one speaker at a time overrides it
While "One speaker at a time" is on, the switch "Stop the answer when someone starts
talking" SHALL be disabled, keep its stored value, and show a note that it has no effect
while one speaker at a time is on, because a question then does not stop the answer.

#### Scenario: One speaker at a time turned on
- **WHEN** the mode is Whisper, the switch is on, and the user turns "One speaker at a time" on
- **THEN** the switch is disabled, still reads "On", and the note says it has no effect while one speaker at a time is on
