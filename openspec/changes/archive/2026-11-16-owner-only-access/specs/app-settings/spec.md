# Spec Delta

## ADDED Requirements

### Requirement: Owner-only setting
The settings SHALL hold `discord.owner_only`, a boolean with the default `false`. A file
without it SHALL load with `false`. A save that changes only this field SHALL restart
nothing and SHALL apply from the next voice frame. The settings SHALL
hold no owner account: the link is kept apart from them.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.0.1
- **THEN** `discord.owner_only` is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `discord.owner_only` as the string `"yes"`
- **THEN** the save is rejected with an error naming `discord.owner_only` and the stored settings are unchanged

#### Scenario: Turned on while in a channel
- **WHEN** the user saves `discord.owner_only` as `true` while the bot is in a voice channel
- **THEN** the bot stays in the channel, no gate, wake engine or call is rebuilt, and the next frame is filtered
