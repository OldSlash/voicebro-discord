# discord-panel-controls Specification

## Purpose
Lets the bot's linked owner run it from the panel the bot posts in Discord: make it leave
the voice channel, mute it and switch the assistant mode, without the VoiceBro window and
without new slash commands.

## Requirements

### Requirement: Only the linked owner presses the panel's buttons
The app SHALL act on a press of any of the panel's buttons only when the presser is the
Discord account linked as the owner, whatever `discord.access` is. A press from anyone else
SHALL change nothing, SHALL leave the panel as it is and SHALL get a reply only the presser
sees, saying that the buttons are the owner's. While no owner is linked every press SHALL
be refused that way, and the reply SHALL say that the operator links an owner in VoiceBro.

#### Scenario: Another member presses Leave channel
- **WHEN** an owner is linked and a member who is not the owner presses Leave channel
- **THEN** the bot stays in the voice channel, the panel does not change, and only that member sees the reply that the buttons are the owner's

#### Scenario: No owner linked
- **WHEN** no owner is linked and a member presses Mute assistant
- **THEN** the mute state does not change, and only that member sees the reply that no owner is linked and that the operator links one in VoiceBro

#### Scenario: The owner in the everyone mode
- **WHEN** `discord.access` is `everyone`, an owner is linked, and the owner presses Mute assistant
- **THEN** the app-wide mute toggles

### Requirement: The panel's buttons
The panel SHALL carry one row of buttons: Mute assistant (Unmute assistant while muted),
Text answers and Leave channel. While the assistant mode is text answers, Text answers SHALL
be marked and SHALL NOT be pressable. Every panel SHALL show the current mute state and mode
within a second of a change, whatever made the change.

#### Scenario: The panel in the Whisper mode
- **WHEN** the mode is Whisper and a member runs `/join`
- **THEN** the panel shows one row with Mute assistant, Text answers and Leave channel, Text answers can be pressed, and there is no Realtime voice or Whisper mode button

#### Scenario: The panel in the text answers mode
- **WHEN** the mode is text answers and a member runs `/join`
- **THEN** the panel's Text answers button is marked and cannot be pressed

#### Scenario: The mode changed in the window
- **WHEN** the bot is in a voice channel and the user picks another assistant mode in the window
- **THEN** within a second the panel's Mode field names the new mode, and Text answers is marked only while the new mode is text answers

### Requirement: Leave channel leaves like `/leave`
The owner's press of Leave channel SHALL make the bot leave the voice channel of the
panel's server exactly as `/leave` does, SHALL delete the panel, and SHALL reply only to
the owner with the name of the channel the bot left.

#### Scenario: The owner presses Leave channel
- **WHEN** the bot is in the voice channel "General" and the owner presses Leave channel
- **THEN** the bot leaves "General", the panel is deleted, only the owner sees "Left General.", and a later `/join` starts with fresh handlers and a fresh track

### Requirement: A mode button switches the assistant mode
The owner's press of Text answers SHALL save `assistant.mode` as text answers with the same
checks and storage as a save in the window and apply it at once. The mode is app-wide: in
every voice channel the bot is in, the running session SHALL end with the falling tone, the
bot SHALL stay, and the assistant SHALL go on in the new mode. The engine SHALL stay as it
is. The window SHALL show the new mode without a restart.

#### Scenario: From the realtime voice to text answers
- **WHEN** the bot is in a voice channel in the realtime mode with Whisper's files on disk and the owner presses Text answers
- **THEN** a running session ends with the falling tone, the bot stays in the channel, `settings.json` holds `assistant.mode: "text"`, the window's Config → Assistant shows the text answers mode, and the next question's answer is posted in a chat with nothing spoken

#### Scenario: Two servers
- **WHEN** the bot is in voice channels of two servers and the owner presses Text answers on the first server's panel
- **THEN** both servers' sessions switch to the text answers mode and the second server's panel shows it within a second

#### Scenario: Unsaved edits in the window
- **WHEN** the window's "Discord and Codex" card has an unsaved edit and the owner presses Text answers on the panel
- **THEN** the window shows the new mode, and the card keeps the unsaved value and its "Unsaved changes." note

#### Scenario: Text answers keeps the engine
- **WHEN** the engine is OpenCode, the mode is Whisper, and the owner presses Text answers
- **THEN** the mode is text answers and the engine stays OpenCode

### Requirement: A failed mode save changes nothing
When the settings check refuses the save or the settings file cannot be written, the mode
and the engine SHALL stay as they were, nothing running SHALL be switched, and the owner
SHALL get the reason in a reply only they see.

#### Scenario: The settings file cannot be written
- **WHEN** `settings.json` is read-only and the owner presses another mode
- **THEN** the mode and the engine stay as they were, the sessions go on, and only the owner sees a reply with the error

### Requirement: A press on an old panel changes nothing
A press on a panel that is not the panel of the bot's current voice session in that server
SHALL change nothing. At the owner's press the old panel SHALL lose its buttons and SHALL
say that `/join` posts a new panel.

#### Scenario: A panel from before a restart
- **WHEN** the app was closed while the bot was in a voice channel, it was started again, and the owner presses Whisper mode on the panel left from before
- **THEN** the mode does not change, and the old panel shows no buttons and says that `/join` posts a new panel

### Requirement: Text answers is refused while Whisper's files are missing
Before saving, the app SHALL check on disk the files the text answers mode needs: the
Whisper runtime, model and voice activity model. When one is missing, nothing SHALL be
saved or downloaded, and a reply only the owner sees SHALL name what is missing and where
the operator downloads it.

#### Scenario: The Whisper model is missing
- **WHEN** the selected Whisper model is not on disk and the owner presses Text answers
- **THEN** the mode does not change, `settings.json` is unchanged, nothing is downloaded, the window opens no install dialog, and only the owner sees a reply naming the Whisper files and VoiceBro's Settings tab

#### Scenario: Text answers needs no voice
- **WHEN** the Whisper files are on disk, the selected voice is not, and the owner presses Text answers
- **THEN** the mode switches to text answers
