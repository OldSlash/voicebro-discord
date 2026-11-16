# Spec Delta

## MODIFIED Requirements

### Requirement: Slash commands are registered where they appear at once
The app SHALL register `/join`, `/leave`, `/status` and `/link` globally and, when
`discord.guild_id` is set, also in that guild. The global `/link` SHALL also be usable in a
direct message to the bot.

#### Scenario: Guild id set
- **WHEN** the bot connects with a guild id configured
- **THEN** the four commands are registered for that guild, usable there within seconds, and globally for every other server

#### Scenario: Guild id empty
- **WHEN** the bot connects with no guild id
- **THEN** the four commands are registered globally and the log says they may take up to an hour to appear

#### Scenario: `/link` in a direct message
- **WHEN** a user opens a direct message with the bot after the global commands appeared
- **THEN** `/link` is offered there
