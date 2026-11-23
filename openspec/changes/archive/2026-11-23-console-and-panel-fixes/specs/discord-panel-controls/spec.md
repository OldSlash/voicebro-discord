# Spec Delta

## MODIFIED Requirements

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

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: A mode whose files are missing is refused
**Reason**: Its Whisper mode and Realtime voice parts have no button any more (the user's
decision of 2026-11-23); what is left for Text answers is requirement "Text answers is
refused while Whisper's files are missing".
**Migration**: None for the operator. The app still checks the files of any mode a press
asks for, so a press on an older panel's button, which the old-panel rule answers first,
cannot save a mode with missing files either.

### Requirement: Realtime voice switches the engine to Codex
**Reason**: The panel no longer has a Realtime voice button (the user's decision of
2026-11-23), so no press on the current panel switches the engine.
**Migration**: Pick Realtime voice in the window: with Claude Code or OpenCode it saves the
realtime mode and Codex in one save (change `realtime-switches-to-codex`). A press on a
Realtime voice button of a panel posted before this change is answered by the old-panel
rule and changes nothing.
