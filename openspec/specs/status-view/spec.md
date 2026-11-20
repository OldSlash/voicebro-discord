# status-view Specification

## Purpose
Shows what the bot is doing right now, with values the app measured, so a problem can be
seen without opening the log.

## Requirements

### Requirement: Status contents
The Status tab SHALL show the connected guilds on the left and a
grid of fact tiles on the right: Discord state, bot user, voice channel per guild; assistant
engine; assistant state (waiting for the wake word, listening, muted) and the selected
engine's account (for Codex the ChatGPT account, for Claude Code the sign-in kind, for
OpenCode the providers and the model in use) and, with Codex, the call state; wake engine
state, model, download progress, unknown words; mute; hotkey shortcut and whether it is
registered; connection errors; the build stamp; with the actions Mute, Connect/Disconnect,
and the selected engine's Install, Sign in and Check.

#### Scenario: Bot in a channel, waiting
- **WHEN** the bot is in a voice channel in wake-word mode with no session
- **THEN** the tab shows the channel and "waiting for the wake word"

#### Scenario: Layout
- **WHEN** the user opens the Status tab with the bot in one guild
- **THEN** the guild is listed on the left and each fact is a separate tile on the right with its label above its value

#### Scenario: Claude Code selected
- **WHEN** the engine is Claude Code, signed in with a Max subscription
- **THEN** the tiles show the engine "Claude Code", its version and "Claude subscription (Max)", and no call tile

### Requirement: Status in Whisper mode
The Status tab SHALL show the assistant mode (realtime, Whisper or text answers). In Whisper
mode it SHALL also show Whisper's state, the model and the device in use, and the voice's
state and voice, and the wake engine tile SHALL say that Whisper spots the phrases. In the
text answers mode it SHALL show the same, except that the voice's tile SHALL say that no
voice is used.

#### Scenario: Whisper mode in a channel
- **WHEN** the bot is in a channel in Whisper mode with the model on the GPU
- **THEN** the tiles show the mode "Whisper", the model, "GPU (CUDA)" and the voice, and the wake engine tile says Whisper spots the phrases

#### Scenario: Text answers mode in a channel
- **WHEN** the bot is in a channel in the text answers mode
- **THEN** the tiles show the mode "Whisper, text answers", the model and the device, and the voice's tile says no voice is used

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

### Requirement: Claude Code install offer
With Claude Code as the engine and no Claude Code executable found, the Status tab SHALL
say that Claude Code is not installed, what an install does (winget, package
`Anthropic.ClaudeCode` from Anthropic, the download and disk size) and offer an "Install
Claude Code" button. The button SHALL run winget in a console window the user sees,
accepting no agreement on the user's behalf; nothing SHALL be installed without the click.

#### Scenario: Install pressed
- **WHEN** the user presses Install Claude Code
- **THEN** a console window opens running `winget install --id Anthropic.ClaudeCode --exact --source winget`, and the notice says the install is running

#### Scenario: Install finished
- **WHEN** the console window's winget ends with success
- **THEN** within 5 s the notice is gone, the Status tab shows the Claude Code version, and the engine area says to sign in

### Requirement: Claude Code without winget
When `winget.exe` is not found, the Claude Code notice SHALL say so instead of offering the
button and SHALL name the other way: Claude Code's own installer from its setup page, with
the path set in the Claude Code card when it is not found by itself.

#### Scenario: No winget
- **WHEN** Claude Code is missing and `winget.exe` is not on the machine
- **THEN** the notice links to Claude Code's setup page and shows no Install button

### Requirement: Claude Code sign-in step
When Claude Code is found and reports no sign-in, the engine area SHALL say that the next
step is to sign in, next to a "Sign in to Claude Code" button that opens a console window
running Claude Code's own sign-in. After the window closes, the app SHALL read the sign-in
kind again within 5 s.

#### Scenario: Signed in through the console
- **WHEN** the user presses Sign in to Claude Code, finishes the sign-in with a Pro subscription in the browser and the console window closes
- **THEN** within 5 s the engine area shows "Claude subscription (Pro)"

### Requirement: OpenCode install offer
With OpenCode as the engine and no OpenCode executable found, the Status tab SHALL say that
OpenCode is not installed, what an install does (winget, package `Anomaly.OpenCode`, the
download and disk size) and offer an "Install OpenCode" button run as for Claude Code.
Without winget it SHALL name `npm install -g opencode-ai` and OpenCode's releases on GitHub.

#### Scenario: Install pressed
- **WHEN** the user presses Install OpenCode
- **THEN** a console window opens running `winget install --id Anomaly.OpenCode --exact --source winget`, and the notice says the install is running

### Requirement: OpenCode setup hint
OpenCode SHALL have no Sign in button. When OpenCode is found and reports no provider, the
engine area SHALL say that a provider is set up in OpenCode itself, with the command
`opencode auth login` to run in a terminal and a link to OpenCode's providers page.

#### Scenario: No provider
- **WHEN** OpenCode was just installed and the user presses Check
- **THEN** the engine area shows the version, "no provider set up" and the hint with the command, and no Sign in button
