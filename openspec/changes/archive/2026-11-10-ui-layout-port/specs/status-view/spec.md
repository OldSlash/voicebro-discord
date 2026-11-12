# Spec Delta

## MODIFIED Requirements

### Requirement: Status contents
The Status tab SHALL show the connected guilds on the left and a grid of fact tiles on the
right: Discord state, bot user, voice channel per guild; assistant engine; assistant state
(waiting for the wake word, listening, muted) and the Codex account and call state; wake
engine state, model, download progress, unknown words; mute; hotkey shortcut and whether it
is registered; connection errors; the build stamp; with the actions Mute,
Connect/Disconnect, Sign in, Check.

#### Scenario: Bot in a channel, waiting
- **WHEN** the bot is in a voice channel in wake-word mode with no session
- **THEN** the tab shows the channel and "waiting for the wake word"

#### Scenario: Layout
- **WHEN** the user opens the Status tab with the bot in one guild
- **THEN** the guild is listed on the left and each fact is a separate tile on the right with its label above its value
