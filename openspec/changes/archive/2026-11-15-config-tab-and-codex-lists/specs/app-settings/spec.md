## ADDED Requirements

### Requirement: Codex lists refresh setting
The settings SHALL hold `codex.refresh_lists_at_start`, a boolean with the default `false`.
A settings file without it SHALL load with `false`, and the next save SHALL write it. A
document whose value is not a boolean SHALL be rejected with the stored settings unchanged.
A save that changes only this field SHALL restart nothing and SHALL take effect at the next
start.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.0.1
- **THEN** `codex.refresh_lists_at_start` is `false`, and the next save writes it

#### Scenario: Turned on
- **WHEN** a save sets `codex.refresh_lists_at_start` to `true` while the bot is in a voice channel
- **THEN** the setting is stored, and the Discord connection, the wake engine, the assistant and a running call keep running

### Requirement: Collapsed cards setting
The settings SHALL hold `ui.collapsed_cards`, a list of the ids of the collapsed cards of
the Settings and Config tabs, empty by default. A settings file without it SHALL load with an empty list.
An id the window does not know SHALL be kept in the file and ignored by the window. A save
that changes only this field SHALL restart nothing.

#### Scenario: Older file
- **WHEN** the app loads a settings file without `ui.collapsed_cards`
- **THEN** the list is empty and every card of the Settings and Config tabs is expanded

#### Scenario: Unknown id
- **WHEN** the file lists `["audio", "later-card"]`
- **THEN** the file loads, the Audio card is collapsed, and the next save keeps both ids

#### Scenario: Not a list
- **WHEN** a save sends `ui.collapsed_cards` as a number
- **THEN** the save is rejected and the stored settings are unchanged
