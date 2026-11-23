# Spec Delta

## MODIFIED Requirements

### Requirement: The engine switches at once
A change of `assistant.engine` SHALL take effect at once: in every voice channel the bot is
in, a running session SHALL end with the falling tone, the guild's assistant session SHALL
be replaced by one of the new engine on the same gate and output, and the bot SHALL stay in
the channel. The old engine's processes SHALL stop as after the last leave.

#### Scenario: Codex to Claude in a channel
- **WHEN** the bot is in a voice channel in the Whisper mode with Codex and the user picks Claude Code
- **THEN** any running answer stops, the bot stays in the channel, the Codex app-server stops shortly after, and the next question goes to Claude Code

#### Scenario: The new engine cannot start
- **WHEN** the user switches to Claude Code while the bot is in a channel and Claude Code is not signed in
- **THEN** the bot stays in the channel, a connection error names the engine and what to do, and no question is sent anywhere until the engine is ready

#### Scenario: Claude Code in the Whisper mode back to the realtime voice in a channel
- **WHEN** the bot is in a voice channel with Claude Code in the Whisper mode and one save sets the engine Codex and the mode realtime
- **THEN** any running answer stops, the bot stays in the channel, Claude Code's process stops shortly after, Whisper and the voice unload, and the next session is a Codex realtime voice call
