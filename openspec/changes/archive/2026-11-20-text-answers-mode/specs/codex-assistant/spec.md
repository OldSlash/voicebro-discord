# Spec Delta

## ADDED Requirements

### Requirement: Join prepares the text answers mode
In the text answers mode the join SHALL load the Whisper model as in the Whisper mode and
SHALL neither load nor require the voice. When Whisper's files are not downloaded or fail to
load, the bot SHALL not join and SHALL reply that they are missing, to open VoiceBro's
Settings tab and press Download in the Whisper card, then run `/join` again.

#### Scenario: Only Whisper's files on disk
- **WHEN** a member runs `/join` in the text answers mode with Whisper's files on disk and the selected voice not downloaded
- **THEN** the bot joins, Whisper is loaded, and no voice is loaded

#### Scenario: Whisper's files missing
- **WHEN** a member runs `/join` in the text answers mode and the selected Whisper model is not downloaded
- **THEN** the bot stays out of the channel, the reply says to press Download in the Whisper card of VoiceBro's Settings tab and run `/join` again, and the window opens the install dialog
