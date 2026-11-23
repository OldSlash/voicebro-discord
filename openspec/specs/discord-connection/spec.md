# discord-connection Specification

## Purpose
Keeps the bot connected to Discord with the stored token and makes its connection state and
errors visible, so the user always knows whether the bot is online and why not.

## Requirements

### Requirement: The bot connects with the stored token
The app SHALL connect to the Discord gateway with the token from the secret store whenever a
token is set, at start-up and whenever a token is stored; there is no switch to turn this
off.

#### Scenario: Start with a stored token
- **WHEN** the app starts with a token in the secret store
- **THEN** the bot connects without any action from the user, and the status shows the bot's user name and the number of guilds

#### Scenario: Token stored while running
- **WHEN** the user stores a new token
- **THEN** any running connection is stopped and a new one is started with the new token

#### Scenario: Token cleared
- **WHEN** the user clears the token
- **THEN** the bot disconnects and the status shows it as disconnected

#### Scenario: Auto-connect off
- **WHEN** an old settings file still holds `discord.auto_connect: false` and a token is stored
- **THEN** the bot connects at start-up and the field is dropped at the next save

### Requirement: Connection state and errors are visible
The status snapshot SHALL carry the connection state (disconnected, connecting, connected,
error), the bot's user name and guild count when connected, and the last error text while
in error.

#### Scenario: Invalid token
- **WHEN** the gateway rejects the token
- **THEN** the status shows the error state with Discord's error text, and the app keeps running

#### Scenario: Gateway drops
- **WHEN** the gateway connection is lost
- **THEN** the status leaves the connected state, the error is shown, and serenity's reconnect is allowed to bring it back

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

### Requirement: Connect and disconnect on demand
The UI SHALL offer Connect and Disconnect; Connect with no stored token is refused with a
message naming the token.

#### Scenario: Connect without a token
- **WHEN** the user presses Connect and no token is stored
- **THEN** nothing connects and the message says a token must be stored first

### Requirement: The bot needs no privileged intent
The app SHALL connect to the Discord gateway without any privileged intent (presence,
server members, message content). It SHALL still receive the text of the messages a link
code can come in: direct messages to the bot and server messages that mention it.

#### Scenario: Privileged intents off
- **WHEN** all three privileged intents are off for the bot in the Discord Developer Portal and the token is stored
- **THEN** the bot connects without Discord refusing its intents, and a link code sent in a direct message to it, or in a server channel with a mention of it, reaches the app with its text
