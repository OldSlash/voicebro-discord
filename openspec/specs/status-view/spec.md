# status-view Specification

## Purpose
Shows what the bot is doing right now, with values the app measured, so a problem can be
seen without opening the log.

## Requirements

### Requirement: Status contents
The Status tab SHALL show the connected guilds on the left and a
grid of fact tiles on the right: Discord state, bot user, voice channel per guild; assistant
engine; assistant state (waiting for the wake word, listening, muted) and the Codex account
and call state; wake engine state, model, download progress, unknown words; mute; hotkey
shortcut and whether it is registered; connection errors; the build stamp; with the actions
Mute, Connect/Disconnect, Sign in, Check.

#### Scenario: Bot in a channel, waiting
- **WHEN** the bot is in a voice channel in wake-word mode with no session
- **THEN** the tab shows the channel and "waiting for the wake word"

#### Scenario: Layout
- **WHEN** the user opens the Status tab with the bot in one guild
- **THEN** the guild is listed on the left and each fact is a separate tile on the right with its label above its value

### Requirement: Measured values only
The Status tab SHALL show no quantity the app did not measure; counters (ticks, drops,
underruns, wake drops) SHALL be behind a details toggle and SHALL be numbers from the
status snapshot.

#### Scenario: Details
- **WHEN** the user opens the details toggle after a minute in a channel
- **THEN** every counter shown is a number from the snapshot

### Requirement: Live updates
The tab SHALL refresh on every app event and at least every 5 s.

#### Scenario: Mute from Discord
- **WHEN** mute is toggled from the Discord panel
- **THEN** the tab shows the new state within a second

### Requirement: Codex install offer
When no Codex executable is found, the Status tab SHALL show that Codex CLI is not
installed, what an install does (winget, package `OpenAI.Codex` from OpenAI's GitHub
releases, the download and disk size) and an "Install Codex" button. The button SHALL run
winget in a console window the user sees, accepting no agreement on the user's behalf, and
nothing SHALL be installed without the click.

#### Scenario: Codex missing
- **WHEN** the app runs and no Codex executable is found
- **THEN** the Status tab shows the notice with the package, its source, the sizes and the Install Codex button, and no winget process runs

#### Scenario: Install pressed
- **WHEN** the user presses Install Codex
- **THEN** a console window opens running `winget install --id OpenAI.Codex --exact --source winget`, and the notice says the install is running

#### Scenario: Install finished
- **WHEN** the console window's winget ends with success
- **THEN** within 5 s the notice is gone, the Status tab shows the Codex version, and the Codex area says to sign in to ChatGPT

#### Scenario: Install failed
- **WHEN** winget ends with a non-zero exit code
- **THEN** the notice shows the exit code, says the console window has the details, and offers the button again

### Requirement: Install without winget
When `winget.exe` is not found, the notice SHALL say so instead of offering the button and
SHALL name the other ways: `npm install -g @openai/codex`, or the download from
github.com/openai/codex/releases with its path set in Config → Codex executable.

#### Scenario: No winget
- **WHEN** Codex is missing and `winget.exe` is not on the machine
- **THEN** the notice shows both other ways and no Install Codex button

### Requirement: Sign-in step after install
When Codex is found and the account read says not signed in, the Codex area of the Status
tab SHALL say that Codex is installed and that the next step is to sign in to ChatGPT, next
to the Sign in to Codex button.

#### Scenario: Installed, not signed in
- **WHEN** Codex was just installed and the account read says no account
- **THEN** the Codex area shows the version, "not signed in" and the next step, with the Sign in to Codex button
