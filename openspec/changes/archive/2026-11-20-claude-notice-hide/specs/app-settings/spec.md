# Spec Delta

## MODIFIED Requirements

### Requirement: Claude Code settings
The settings SHALL hold a `claude` section with `executable` (empty means found
automatically), `model` (empty means Claude Code's default),
`subscription_notice_shown` (default `false`) and `subscription_notice_hidden` (default
`false`). A model SHALL have no spaces and at most 200 characters. A change of the model
SHALL apply from the guild's next Claude Code conversation. A file without
`subscription_notice_hidden` SHALL load with `false`, and a value that is not a boolean
SHALL be refused, naming the field.

#### Scenario: Defaults
- **WHEN** the app starts with no settings file
- **THEN** the `claude` section has an empty executable, an empty model, `subscription_notice_shown` false and `subscription_notice_hidden` false

#### Scenario: A model with a space
- **WHEN** a save sets `claude.model` to "sonnet 5"
- **THEN** it is refused with a message naming `claude.model`

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.3.0 without `claude.subscription_notice_hidden`
- **THEN** the field is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `claude.subscription_notice_hidden` as the string `"yes"`
- **THEN** the save is refused with an error naming the field, and the stored settings stay
