# Spec Delta

## MODIFIED Requirements

### Requirement: The install dialog opens when the wake word cannot work
The window SHALL open a modal install dialog when the bot joins a voice channel in
wake-word mode while the selected model's files or the Vosk library are missing, and when
the user switches the activation to wake word with them missing. The window's start SHALL
NOT open it. The dialog SHALL name the model's language and each missing file with its
size and site. One dialog at most SHALL be open.

#### Scenario: Start on a clean folder
- **WHEN** the app starts for the first time in an empty folder
- **THEN** no dialog opens, and the Status tab's Wake word line and the notice under Wake model in Settings → Assistant say that the Vosk library and the English model are needed

#### Scenario: The bot joins with files missing
- **WHEN** the app runs in wake-word mode with the selected model's files missing and the bot joins a voice channel
- **THEN** the dialog opens, says the English wake model is not installed, and lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com)

#### Scenario: Every join asks again
- **WHEN** the user closed the dialog with Not now, and the bot leaves and joins a voice channel again with the files still missing
- **THEN** the dialog opens again

#### Scenario: Switched to wake word
- **WHEN** the activation is always listening, nothing is on disk, and the user switches it to wake word
- **THEN** the setting is saved and the dialog opens

#### Scenario: Always listening
- **WHEN** the app starts or the bot joins in always-listening mode with nothing on disk
- **THEN** no dialog opens

#### Scenario: Files on disk
- **WHEN** the bot joins a voice channel in wake-word mode and the selected model and the Vosk library are on disk
- **THEN** no dialog opens

## ADDED Requirements

### Requirement: The install dialog offers the download by hand
Under the missing files the dialog SHALL list each file's archive name with a link to the
official page it is published on, an "Open assets folder" button and a line saying that a
`.zip` put into that folder as it is gets checked and unpacked on Download without going
online. This part SHALL be hidden while a download runs and shown again after a failure.

#### Scenario: The links
- **WHEN** the dialog opens for the English model with nothing on disk
- **THEN** it names `vosk-win64-0.3.45.zip` with a link to the Vosk release page on github.com and `vosk-model-small-en-us-0.15.zip` with a link to the Vosk models page on alphacephei.com, and shows the "Open assets folder" button

#### Scenario: A link is clicked
- **WHEN** the user clicks one of the links
- **THEN** the page opens in the default browser, the window keeps showing the app with the dialog open, and nothing is downloaded by the app

#### Scenario: Open assets folder
- **WHEN** the user presses "Open assets folder"
- **THEN** the folder `assets\` next to the executable opens in Explorer, created when it is missing, and the dialog stays open

#### Scenario: Only what is missing
- **WHEN** the Vosk library is on disk and the user chooses the German model, which is not
- **THEN** the dialog names only `vosk-model-small-de-0.15.zip` with its link

#### Scenario: During and after a download
- **WHEN** the user presses Download and the download later fails
- **THEN** the links and the folder button are hidden while it runs and shown again with the error

#### Scenario: Files put there by hand
- **WHEN** both archives are in `assets\` and the user presses Download in the dialog
- **THEN** the dialog says that it checks and unpacks each file, never that it downloads one, and closes when both are installed

#### Scenario: The focus
- **WHEN** the dialog opens
- **THEN** the focus starts on Not now (Cancel in the choice case), not on a link or Download

### Requirement: Links open in the default browser
Every link in the window SHALL open in the user's default browser, whichever mouse button,
modifier key or keyboard key follows it. The window SHALL NOT show a web page or open a
window of its own for a link.

#### Scenario: A plain click
- **WHEN** the user clicks the link to the Vosk models page
- **THEN** the page opens once in the default browser and the VoiceBro window keeps showing the app

#### Scenario: Other ways to follow a link
- **WHEN** the user follows a link with Ctrl+click, Shift+click, a middle click, Enter or the context menu
- **THEN** the page opens in the default browser and no VoiceBro window shows it or opens for it
