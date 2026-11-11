# Spec Delta

## MODIFIED Requirements

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

#### Scenario: Codex missing
- **WHEN** a member runs `/join` and no Codex executable is found
- **THEN** the bot stays out of the channel and the reply says that Codex CLI is not installed and to press Install Codex on the app's Status tab, then run `/join` again

## ADDED Requirements

### Requirement: Finding Codex
The app SHALL look for Codex in this order: the configured executable, `codex.exe` on the
PATH, winget's command links folder `%LOCALAPPDATA%\Microsoft\WinGet\Links`, npm's global
folder, then the `codex.cmd` shim. Each status read SHALL look again, so a Codex installed
while the app runs is found without a restart.

#### Scenario: Installed by winget while the app runs
- **WHEN** Codex is installed with winget after the app started and the app's PATH does not contain winget's links folder
- **THEN** the next status read finds `codex.exe` in that folder and shows its version

#### Scenario: Configured path wins
- **WHEN** the Codex executable is set in Config and Codex is also installed by winget
- **THEN** the configured executable is used
