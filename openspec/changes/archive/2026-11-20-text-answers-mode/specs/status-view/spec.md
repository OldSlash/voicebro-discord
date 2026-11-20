# Spec Delta

## MODIFIED Requirements

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
