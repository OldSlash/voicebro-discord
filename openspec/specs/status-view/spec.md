# status-view Specification

## Purpose
Shows what the bot is doing right now, with values the app measured, so a problem can be
seen without opening the log.

## Requirements

### Requirement: Status contents
The Status tab SHALL show, as in the first version, the connected guilds on the left and a
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
