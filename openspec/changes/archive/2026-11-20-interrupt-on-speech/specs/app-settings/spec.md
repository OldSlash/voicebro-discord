# Spec Delta

## ADDED Requirements

### Requirement: Interrupt on speech setting
The settings SHALL hold `assistant.interrupt_on_speech`, a boolean with the default
`false`. A file without it SHALL load with `false`. A value that is not a boolean SHALL be
refused, naming the field. A save that changes only this field SHALL rebuild no wake
engine, no gate and no call, and SHALL apply from the next tick.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.3.0
- **THEN** `assistant.interrupt_on_speech` is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `assistant.interrupt_on_speech` as the string `"yes"`
- **THEN** the save is refused with an error naming the field and the stored settings are unchanged

#### Scenario: Turned on during an answer
- **WHEN** the user turns the setting on while the voice reads an answer in a voice channel
- **THEN** nothing is rebuilt, and a member who then talks for 600 ms stops the answer
