# Spec Delta

## MODIFIED Requirements

### Requirement: Status contents
The Status tab SHALL show the connected guilds on the left and a grid of fact tiles on the
right:

- Discord state, bot user, voice channel per guild;
- the assistant engine and the assistant mode (realtime or Whisper);
- the assistant state (waiting for the wake word, listening, muted) and the Codex account
  and call state;
- the wake engine state, model, download progress and unknown words;
- in Whisper mode, Whisper's state, model and device in use, and the voice's state and
  voice;
- mute;
- the hotkey shortcut and whether it is registered;
- connection errors;
- the build stamp;

with the actions Mute, Connect/Disconnect, Sign in, Check.

#### Scenario: Bot in a channel, waiting
- **WHEN** the bot is in a voice channel in wake-word mode with no session
- **THEN** the tab shows the channel and "waiting for the wake word"

#### Scenario: Layout
- **WHEN** the user opens the Status tab with the bot in one guild
- **THEN** the guild is listed on the left and each fact is a separate tile on the right with its label above its value

#### Scenario: Whisper mode in a channel
- **WHEN** the bot is in a channel in Whisper mode with the model on the GPU
- **THEN** the tiles show the mode "Whisper", the model, "GPU (CUDA)" and the voice, and the wake engine tile says Whisper spots the phrases
