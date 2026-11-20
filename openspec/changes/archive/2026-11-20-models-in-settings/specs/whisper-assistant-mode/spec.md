# Spec Delta

## MODIFIED Requirements

### Requirement: The mode switches at once
A change of `assistant.mode` SHALL take effect at once:

- the wake engine unloads (to Whisper) or loads (to realtime);
- in every voice channel the bot is in, the session ends with the falling tone;
- the guild's assistant session is replaced by one in the new mode, on the same gate and
  output, without the bot leaving the channel.

`/status` and the member panel SHALL name the mode.

#### Scenario: Switched to Whisper while in a channel
- **WHEN** the bot is in a voice channel in realtime mode and the user picks Whisper
- **THEN** any realtime call ends, the bot stays in the channel, Vosk unloads, Whisper and the voice load, and the next activation phrase is spotted by Whisper

#### Scenario: Files missing at the switch
- **WHEN** the user switches to Whisper and the model or the voice is not downloaded
- **THEN** the mode is saved, no dialog opens, Config → Assistant says what is not downloaded with a button to Settings, and nothing is downloaded until Download is pressed there
