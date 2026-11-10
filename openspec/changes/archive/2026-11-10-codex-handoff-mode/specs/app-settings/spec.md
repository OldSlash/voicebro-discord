# Spec Delta

## ADDED Requirements

### Requirement: Answer handoff mode setting
The settings SHALL hold `codex.handoff_mode`, one of `items`, `commentary`, `bem_tags` and
`thinking`, with the default `items`. Any other value SHALL be rejected with a message naming
the field and the allowed values. A settings file without the field SHALL load with `items`.

#### Scenario: Valid choice
- **WHEN** the user saves `codex.handoff_mode` as `commentary`
- **THEN** the save succeeds and the stored settings hold `commentary`

#### Scenario: Unknown value
- **WHEN** a saved document sets `codex.handoff_mode` to `verbatim`
- **THEN** the save is rejected and the message names `codex.handoff_mode` and the four allowed values

#### Scenario: Older file
- **WHEN** the settings file has a `codex` section without `handoff_mode`
- **THEN** the field is `items` and the next save writes it
