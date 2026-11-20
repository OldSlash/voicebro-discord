# Spec Delta

## MODIFIED Requirements

### Requirement: Assistant mode setting
`assistant.mode` SHALL be `realtime`, `whisper` or `text`, default `realtime`. Any other
value SHALL be refused, naming the field and the three values. `realtime` SHALL be refused
while `assistant.engine` is not `codex`, with a message that the voice mode works only with
Codex. A change SHALL apply at once, as the requirement "The mode switches at once" of
`whisper-assistant-mode` says.

#### Scenario: Older file
- **WHEN** a settings file from 1.0.1 is loaded
- **THEN** the assistant mode is realtime, and the next save writes the field

#### Scenario: Unknown value
- **WHEN** a save sets `assistant.mode` to "local"
- **THEN** it is refused with a message naming `assistant.mode`, `realtime`, `whisper` and `text`, and the stored settings stay

#### Scenario: Realtime with Claude
- **WHEN** a save sets `assistant.engine` to `claude` and `assistant.mode` to `realtime`
- **THEN** it is refused with a message that the voice mode works only with Codex, and the stored settings stay

#### Scenario: Text answers with Claude
- **WHEN** a save sets `assistant.engine` to `claude` and `assistant.mode` to `text`
- **THEN** it is saved
