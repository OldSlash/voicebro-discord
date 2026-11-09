# Spec Delta

## MODIFIED Requirements

### Requirement: App-server lifecycle
The app SHALL start `codex app-server --disable shell_tool` (the configured executable, or
an auto-detected `codex.exe`) on `/join`, and on Sign in or Check from the window, never at
app start. It SHALL stop the app-server when the bot leaves its last voice channel, one
minute after the last Sign in or Check while the bot is in no channel, and when the app
exits. A missing executable or a failed handshake SHALL be shown in the status with the
error text.

#### Scenario: Codex not installed
- **WHEN** no `codex.exe` is found and none is configured
- **THEN** the status shows the assistant as unavailable with "codex not found", and Discord keeps working

#### Scenario: App exit
- **WHEN** the main window is closed
- **THEN** the app-server process is gone within two seconds

#### Scenario: App start
- **WHEN** the app starts and nobody has used `/join`, Sign in or Check
- **THEN** no `codex.exe` process belongs to the app

#### Scenario: Last channel left
- **WHEN** the bot leaves its last voice channel
- **THEN** the app-server process is gone within two seconds and the conversation of each guild is kept for the next join

## ADDED Requirements

### Requirement: Join prepares the assistant
On `/join` the app SHALL start the app-server if needed, read the account and open the
guild's thread before the bot enters the voice channel, so the first activation only places
the call. When Codex is missing, fails to start or is not signed in, the bot SHALL not join
and SHALL reply with the reason and what to do.

#### Scenario: Signed in
- **WHEN** a member runs `/join` and Codex is signed in
- **THEN** the thread is open before the bot is in the channel, and no voice call exists until an activation

#### Scenario: Signed out
- **WHEN** a member runs `/join` and the account read says no account
- **THEN** the bot stays out of the channel and the reply says to use Sign in on the app's Status tab and run `/join` again
