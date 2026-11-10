# Spec Delta

## ADDED Requirements

### Requirement: Audio gain settings
The settings SHALL hold `audio.input_gain` and `audio.output_gain`, each a number from 0.0
to 4.0 with the default 1.0. A value outside that range SHALL be rejected with a message
naming the field. A settings file without the `audio` section SHALL load with the defaults.

#### Scenario: Valid gains
- **WHEN** the user saves an input gain of 3.39 and an output gain of 2.52
- **THEN** the save succeeds and the next status shows both values

#### Scenario: Out of range
- **WHEN** a saved document sets `audio.input_gain` to 5
- **THEN** the save is rejected and the message names `audio.input_gain` and the range 0 to 4

#### Scenario: Older file
- **WHEN** the settings file has no `audio` section
- **THEN** both gains are 1.0 and the next save writes the section
