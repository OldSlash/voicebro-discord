## MODIFIED Requirements

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

## ADDED Requirements

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
