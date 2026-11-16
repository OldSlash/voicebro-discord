# Spec Delta

## ADDED Requirements

### Requirement: Answer posting settings
The settings SHALL hold `assistant.post_answers`, one of `off`, `voice_channel`,
`command_channel` and `channel` with the default `off`, and `assistant.answers_channel`, a
string of digits or empty with the default empty. A file without them SHALL load with the
defaults. A save SHALL be rejected, naming the field, when `post_answers` has another value,
when `answers_channel` is not digits, or when `post_answers` is `channel` and
`answers_channel` is empty.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.0.1
- **THEN** `assistant.post_answers` is `off`, `assistant.answers_channel` is empty, and the next save writes both

#### Scenario: Unknown destination
- **WHEN** a saved document sets `assistant.post_answers` to `thread`
- **THEN** the save is rejected with an error that names the field and lists the four values, and the previous settings stay in effect

#### Scenario: A channel without an id
- **WHEN** a saved document sets `assistant.post_answers` to `channel` and leaves `assistant.answers_channel` empty
- **THEN** the save is rejected with an error naming `assistant.answers_channel`

#### Scenario: Not an id
- **WHEN** a saved document sets `assistant.answers_channel` to `#general`
- **THEN** the save is rejected with an error naming `assistant.answers_channel`

#### Scenario: A stored channel kept while off
- **WHEN** a saved document sets `assistant.post_answers` to `off` and keeps a channel id in `assistant.answers_channel`
- **THEN** the save succeeds and the id stays stored for when `channel` is chosen again

### Requirement: Answer posting settings apply at once
A save that changes only `assistant.post_answers` or `assistant.answers_channel` SHALL
restart nothing: not the Discord connection, the wake engine, the assistant or a running
call. The next answer SHALL go where the new settings say.

#### Scenario: Changed during a call
- **WHEN** the user changes `assistant.post_answers` from `off` to `voice_channel` while a call runs and an agent task completes after the save
- **THEN** the answer is posted in the voice channel's chat, and the call, the Discord connection and the wake engine kept running through the save
