# Spec Delta

## MODIFIED Requirements

### Requirement: App-server lifecycle
The app SHALL start `codex app-server --disable shell_tool` (the configured executable, or
an auto-detected `codex.exe`) on `/join`, on Sign in, Check, Refresh lists or Link from the
window, and at app start only to read the lists as the requirement "Lists are read at start"
says. It SHALL stop the app-server when the bot leaves its last voice channel, one minute
after the last Sign in, Check, Refresh lists, Link or read at start while the bot is in no
channel, and when the app exits. A missing executable or a failed handshake SHALL be shown
in the status with the error text.

#### Scenario: Codex not installed
- **WHEN** no `codex.exe` is found and none is configured
- **THEN** the status shows the assistant as unavailable with "codex not found", and Discord keeps working

#### Scenario: App exit
- **WHEN** the main window is closed
- **THEN** the app-server process is gone within two seconds

#### Scenario: App start
- **WHEN** the app starts with saved lists and the refresh at start off, and nobody has used `/join`, Sign in, Check, Refresh lists or Link
- **THEN** no `codex.exe` process belongs to the app

#### Scenario: Read at start ends
- **WHEN** the app read the lists at start and the bot joins no voice channel
- **THEN** the app-server process is gone about one minute after the read

#### Scenario: Link reads the account
- **WHEN** the user presses Link in the Access card while the bot is in no voice channel
- **THEN** the app-server starts, the account is read, and the process is gone about one minute later whether or not the code is used

#### Scenario: Last channel left
- **WHEN** the bot leaves its last voice channel
- **THEN** the app-server process is gone within two seconds and the conversation of each guild is kept for the next join
