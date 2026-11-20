# Spec Delta

## ADDED Requirements

### Requirement: A join needs the wake word's files
In the realtime mode with the wake word, a `/join` SHALL be refused while the Vosk library
or the selected wake model is not on disk. The bot SHALL stay out of the voice channel and
reply that the wake word's files are missing, to open VoiceBro's Config tab and press
Download under Wake model, then run `/join` again. Nothing SHALL be downloaded by the
refusal. In always-listening mode and in the Whisper mode the Vosk files SHALL not be
needed for a join.

#### Scenario: Model missing
- **WHEN** the mode is realtime with the wake word, the English model is not on disk, and a member runs `/join`
- **THEN** the bot stays out of the channel, the reply names the missing files and says to press Download under Wake model in VoiceBro's Config tab and run `/join` again, and nothing is downloaded

#### Scenario: Always listening
- **WHEN** the mode is realtime with always listening, nothing of Vosk is on disk, and a member runs `/join`
- **THEN** the bot joins

#### Scenario: Files on disk
- **WHEN** the mode is realtime with the wake word and the Vosk library and the selected model are on disk
- **THEN** the join goes on as before
