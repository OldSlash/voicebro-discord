## MODIFIED Requirements

### Requirement: Slash commands are registered where they appear at once
The app SHALL register `/join`, `/leave`, `/status`, `/link`, `/optin`, `/optout` and
`/privacy` globally and, when `discord.guild_id` is set, also in that guild. The global
`/link`, `/optin`, `/optout` and `/privacy` SHALL also be usable in a direct message to the
bot. The descriptions of `/optin` and `/optout` SHALL say that they let the assistant hear
the sender or stop it.

#### Scenario: Guild id set
- **WHEN** the bot connects with a guild id configured
- **THEN** the seven commands are registered for that guild, usable there within seconds, and globally for every other server

#### Scenario: Guild id empty
- **WHEN** the bot connects with no guild id
- **THEN** the seven commands are registered globally and the log says they may take up to an hour to appear

#### Scenario: `/link` in a direct message
- **WHEN** a user opens a direct message with the bot after the global commands appeared
- **THEN** `/link` is offered there

#### Scenario: `/optin` in a direct message
- **WHEN** a user opens a direct message with the bot after the global commands appeared
- **THEN** `/optin`, `/optout` and `/privacy` are offered there and `/join` is not
