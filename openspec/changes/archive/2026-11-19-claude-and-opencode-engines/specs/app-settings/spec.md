## MODIFIED Requirements

### Requirement: Assistant mode setting
`assistant.mode` SHALL be `realtime` or `whisper`, default `realtime`. Any other value
SHALL be refused, naming the field and the two values. `realtime` SHALL be refused while
`assistant.engine` is not `codex`, with a message that the voice mode works only with
Codex. A change SHALL apply at once, as the requirement "The mode switches at once" of
`whisper-assistant-mode` says.

#### Scenario: Older file
- **WHEN** a settings file from 1.0.1 is loaded
- **THEN** the assistant mode is realtime, and the next save writes the field

#### Scenario: Unknown value
- **WHEN** a save sets `assistant.mode` to "local"
- **THEN** it is refused with a message naming `assistant.mode`, `realtime` and `whisper`, and the stored settings stay

#### Scenario: Realtime with Claude
- **WHEN** a save sets `assistant.engine` to `claude` and `assistant.mode` to `realtime`
- **THEN** it is refused with a message that the voice mode works only with Codex, and the stored settings stay

## ADDED Requirements

### Requirement: Assistant engine setting
`assistant.engine` SHALL be `codex`, `claude` or `opencode`, default `codex`. Any other
value SHALL be refused, naming the field and the three values. A change SHALL apply at
once, as the requirement "The engine switches at once" of `assistant-engines` says, and
SHALL restart no Discord connection.

#### Scenario: Unknown engine
- **WHEN** a save sets `assistant.engine` to "gemini"
- **THEN** it is refused with a message naming `assistant.engine` and its three values, and the stored settings stay

#### Scenario: Engine and mode in one save
- **WHEN** the stored settings are Codex in the realtime mode and one save sets the engine to `opencode` and the mode to `whisper`
- **THEN** the save succeeds and both apply at once

### Requirement: Assistant instruction setting
`assistant.instructions` SHALL hold the user's instruction for the assistant, empty by
default, and SHALL apply to whichever engine runs, from its next conversation. The app
SHALL ship no instruction of its own.

#### Scenario: Instruction kept across engines
- **WHEN** the user saves an instruction with Codex as the engine and then picks Claude Code
- **THEN** Claude Code's next conversation gets the same instruction

### Requirement: The Codex instruction of older files
A settings file with `codex.instructions` and without `assistant.instructions` SHALL load
with the old field's text as `assistant.instructions`. The next save SHALL write
`assistant.instructions` and SHALL NOT write `codex.instructions`. When a file has both,
`assistant.instructions` SHALL decide.

#### Scenario: A file from build 1.2.0
- **WHEN** the app loads a settings file from 1.2.0 whose `codex.instructions` is "Answer in two sentences."
- **THEN** `assistant.instructions` is "Answer in two sentences.", and the next save writes it there and no `codex.instructions`

### Requirement: Claude Code settings
The settings SHALL hold a `claude` section with `executable` (empty means found
automatically), `model` (empty means Claude Code's default) and
`subscription_notice_shown` (default `false`). A model SHALL have no spaces and at most 200
characters. A change of the model SHALL apply from the guild's next Claude Code
conversation.

#### Scenario: Defaults
- **WHEN** the app starts with no settings file
- **THEN** the `claude` section has an empty executable, an empty model and `subscription_notice_shown` false

#### Scenario: A model with a space
- **WHEN** a save sets `claude.model` to "sonnet 5"
- **THEN** it is refused with a message naming `claude.model`

### Requirement: OpenCode settings
The settings SHALL hold an `opencode` section with `executable` (empty means found
automatically) and `model` (empty means OpenCode's default, else `provider/model`, where
the model part may hold more `/`). A model SHALL have no spaces and a non-empty provider
and model part around its first `/`. A change of the model SHALL apply from the guild's
next question.

#### Scenario: A model without a provider
- **WHEN** a save sets `opencode.model` to "gpt-5"
- **THEN** it is refused with a message that the model must be written as `provider/model`
