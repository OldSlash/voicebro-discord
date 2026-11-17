## MODIFIED Requirements

### Requirement: Join prepares the Whisper mode
In Whisper mode the join SHALL also load the Whisper model and the voice. When their files
are not downloaded or fail to load, the bot SHALL not join and SHALL reply with the reason
and what to do.

#### Scenario: Whisper files missing
- **WHEN** a member runs `/join` in Whisper mode and the selected Whisper model is not downloaded
- **THEN** the bot stays out of the channel, the reply says to open VoiceBro's Config tab and press Download in the Assistant and Local voice cards, then run `/join` again, and the window opens the install dialog

#### Scenario: Whisper mode join
- **WHEN** a member runs `/join` in Whisper mode with every file on disk
- **THEN** the thread is open and Whisper and the voice are loaded before the bot is in the channel, and no realtime call is ever opened
