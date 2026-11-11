# Spec Delta

## ADDED Requirements

### Requirement: Status reads start no process
Reading the assistant's status SHALL NOT start a process. The Codex version shown in the
status SHALL be read once for an executable and read again only when the executable found
or configured changes its path, size or modification time.

#### Scenario: Idle with the app-server down
- **WHEN** the window is open for 60 s, the bot is in no voice channel and the Codex app-server is not running
- **THEN** no `codex` process is started in that minute, and the status still shows the Codex version

#### Scenario: Codex updated
- **WHEN** the Codex executable is replaced by a newer version while the app runs
- **THEN** the next status read shows the new version
