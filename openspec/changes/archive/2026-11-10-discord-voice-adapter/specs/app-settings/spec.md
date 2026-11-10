# Spec Delta

## ADDED Requirements

### Requirement: Discord guild id setting
The settings SHALL hold `discord.guild_id`, a string of digits or empty, used to scope the
slash command registration. A value that is not empty and not all digits SHALL be rejected
naming the field.

#### Scenario: Valid guild id
- **WHEN** the user saves `discord.guild_id` as a string of digits
- **THEN** the save succeeds and the next connection registers the commands in that guild

#### Scenario: Invalid guild id
- **WHEN** the user saves `discord.guild_id` containing letters
- **THEN** the save is rejected and the message names `discord.guild_id`
