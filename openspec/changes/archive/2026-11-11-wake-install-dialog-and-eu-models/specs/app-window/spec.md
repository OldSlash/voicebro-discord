## ADDED Requirements

### Requirement: The install dialog opens when the wake word cannot work
The window SHALL open a modal install dialog when it starts in wake-word mode while the
selected model's files or the Vosk library are missing, when the bot joins a voice channel
while they are still missing, and when the user switches the activation to wake word with
them missing. The dialog SHALL name the model's language and each missing file with its
size and site. One dialog at most SHALL be open.

#### Scenario: Start on a clean folder
- **WHEN** the app starts for the first time in an empty folder
- **THEN** the dialog opens as soon as the window shows, says the English wake model is not installed, and lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com)

#### Scenario: The bot joins with files missing
- **WHEN** the user closed the dialog with Not now and the bot then joins a voice channel in wake-word mode
- **THEN** the dialog opens again

#### Scenario: Switched to wake word
- **WHEN** the activation is always listening, nothing is on disk, and the user switches it to wake word
- **THEN** the setting is saved and the dialog opens

#### Scenario: Always listening
- **WHEN** the app starts or the bot joins in always-listening mode with nothing on disk
- **THEN** no dialog opens

#### Scenario: Files on disk
- **WHEN** the app starts in wake-word mode and the selected model and the Vosk library are on disk
- **THEN** no dialog opens

### Requirement: The install dialog downloads only on Download
The dialog's Download button SHALL start the download of the named model and show each
file's progress in the dialog, which SHALL close when the files are installed; a failure
SHALL show the error with Try again and Close. Not now SHALL close it and download nothing,
leaving the Assistant card's notice. Closing the dialog during a download SHALL leave the
download running.

#### Scenario: Download
- **WHEN** the user presses Download in the dialog
- **THEN** the dialog shows the progress of the library and then of the model, and closes when both are installed

#### Scenario: Failure
- **WHEN** a download fails or its hash does not match
- **THEN** the dialog shows the error, Try again starts the download again, and Close closes the dialog with the notice left in the Assistant card

#### Scenario: A stray key when the dialog comes up
- **WHEN** the dialog opens while the user is typing and Enter or Space reaches it
- **THEN** that key closes it as Not now does and nothing is downloaded

#### Scenario: Not now
- **WHEN** the user presses Not now
- **THEN** the dialog closes, nothing is requested from the network, and the Assistant card shows the download notice

#### Scenario: Closed during the download
- **WHEN** the user closes the dialog while a file is downloading
- **THEN** the download goes on and its progress shows in the Assistant card's notice
