# Spec Delta

## MODIFIED Requirements

### Requirement: Assistant mode in the Assistant card
The Config tab's Assistant card SHALL hold the assistant mode as its first field: "Realtime
voice", "Whisper" or "Whisper, text answers", with a help line saying what runs on the PC
and what leaves it in each mode, and that the text answers mode posts the answers in a
Discord chat instead of speaking them. Below it a grey Recognition field SHALL show "Vosk"
in the realtime mode and "Whisper" in the two other modes; it follows the mode and cannot be
edited. In the text answers mode the card SHALL show the rows of the Whisper mode.

#### Scenario: Switching to Whisper
- **WHEN** the user picks Whisper as the assistant mode on the Config tab
- **THEN** the mode is saved at once, and the Recognition field shows "Whisper"

#### Scenario: Switching to text answers
- **WHEN** the user picks "Whisper, text answers" as the assistant mode
- **THEN** `assistant.mode` is saved as `text` at once, the Recognition field shows "Whisper", and the card shows the Whisper phrases

### Requirement: The Local voice card says when the voice is used
The Local voice card SHALL show a note that follows the assistant mode. In the realtime
voice mode it SHALL say that the local voice is not used, because the Codex call speaks
with its own voice. In the Whisper mode it SHALL say that this voice reads Codex's answers
into the channel. In the text answers mode it SHALL say that the local voice is not used,
because the answers are posted as text.

#### Scenario: Realtime mode
- **WHEN** the assistant mode is realtime voice
- **THEN** the Local voice card says the local voice is not used in this mode, and its fields and Download still work

#### Scenario: Text answers mode
- **WHEN** the assistant mode is text
- **THEN** the Local voice card says the local voice is not used because the answers are posted as text, and its fields and Download still work

### Requirement: Answer posting control
The engine card SHALL have a "Post answers to Discord" list after "Save chat history", with
Off, The voice channel's chat, The channel of /join and A chosen channel, saved on change.
Its help line SHALL say that the agent's final answers are posted there, that in the text
answers mode Off posts them to the voice channel's chat, and that the bot needs View
Channel, Send Messages and Embed Links in that channel.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.post_answers`
- **THEN** the list shows Off and no channel list is shown

#### Scenario: Voice channel's chat chosen
- **WHEN** the user picks The voice channel's chat
- **THEN** it is saved without a button, no channel list is shown, and a restart shows the choice

## ADDED Requirements

### Requirement: The Assistant card's note in the text answers mode
In the text answers mode the Assistant card's note on missing files SHALL name only what
Whisper misses (the model, its runtime and the VAD model), never the voice, and its button
SHALL show the Settings tab's Whisper card.

#### Scenario: Voice missing, Whisper on disk
- **WHEN** the mode is text, Whisper's files are on disk and the selected voice is not
- **THEN** the Assistant card shows no note on missing files

#### Scenario: Whisper missing
- **WHEN** the mode is text and the selected Whisper model is not on disk
- **THEN** the note names the Whisper model, and its button shows the Whisper card
