## ADDED Requirements

### Requirement: ChatGPT account notice setting
The settings SHALL hold `codex.account_notice_hidden`, default `false`, which takes only `true` or `false`. While it is `true` the ChatGPT account notice SHALL not show in the window; the headless app SHALL not read it.

#### Scenario: Defaults
- **WHEN** the app starts with no settings file
- **THEN** `codex.account_notice_hidden` is `false`

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.3.1 without `codex.account_notice_hidden`
- **THEN** the field is `false`, the other Codex settings are kept, and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `codex.account_notice_hidden` as the string `"yes"`
- **THEN** the save is refused with an error naming the field, and the stored settings stay

#### Scenario: Hidden
- **WHEN** a save sets `codex.account_notice_hidden` to `true`
- **THEN** the settings file holds `true` and a restart keeps it
