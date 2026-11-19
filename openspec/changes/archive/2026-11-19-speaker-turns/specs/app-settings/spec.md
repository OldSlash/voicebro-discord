## ADDED Requirements

### Requirement: One speaker at a time setting
The settings SHALL hold `assistant.one_speaker_at_a_time`, a boolean with the default
`false`. A file without it SHALL load with `false`. A save that changes only this field
SHALL rebuild no wake engine and no call, and SHALL apply from the next session in the
voice mode and from the next question in the Whisper mode.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.2.0
- **THEN** `assistant.one_speaker_at_a_time` is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `assistant.one_speaker_at_a_time` as the string `"yes"`
- **THEN** the save is rejected with an error naming the field and the stored settings are unchanged
