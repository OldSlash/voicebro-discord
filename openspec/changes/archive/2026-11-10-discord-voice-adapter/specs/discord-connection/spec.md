# Spec Delta

## Purpose

Keeps the bot connected to Discord with the stored token and makes its connection state and
errors visible, so the user always knows whether the bot is online and why not.

## ADDED Requirements

### Requirement: The bot connects with the stored token
The app SHALL connect to the Discord gateway with the token from the secret store when a
token is set and `discord.auto_connect` is on, at start-up and whenever a token is stored.

#### Scenario: Start with a stored token
- **WHEN** the app starts with a token in the secret store and auto-connect on
- **THEN** the bot connects without any action from the user, and the status shows the bot's user name and the number of guilds

#### Scenario: Token stored while running
- **WHEN** the user stores a new token
- **THEN** any running connection is stopped and a new one is started with the new token

#### Scenario: Token cleared
- **WHEN** the user clears the token
- **THEN** the bot disconnects and the status shows it as disconnected

#### Scenario: Auto-connect off
- **WHEN** auto-connect is off and the app starts
- **THEN** the bot stays disconnected until the user presses Connect

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
The app SHALL register `/join`, `/leave` and `/status` in the guild named by
`discord.guild_id` when it is set, and globally when it is empty.

#### Scenario: Guild id set
- **WHEN** the bot connects with a guild id configured
- **THEN** the three commands are registered for that guild and are usable there within seconds

#### Scenario: Guild id empty
- **WHEN** the bot connects with no guild id
- **THEN** the three commands are registered globally and the log says they may take up to an hour to appear

### Requirement: Connect and disconnect on demand
The UI SHALL offer Connect and Disconnect; Connect with no stored token is refused with a
message naming the token.

#### Scenario: Connect without a token
- **WHEN** the user presses Connect and no token is stored
- **THEN** nothing connects and the message says a token must be stored first
