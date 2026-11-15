## MODIFIED Requirements

### Requirement: Config tab
The Config tab SHALL hold the "Discord and Codex" card with the Discord token (masked,
write-only, with help on getting one), the guild id (with help on copying it), the Codex
executable, an "Auto-start enabled" note, Save changes and Reset; below it the bot's
Audio, Assistant, Codex and Hotkey cards; and at the bottom one line with the location of
the settings file. The tab SHALL NOT show the contents of the settings file, and SHALL never
show the token.

#### Scenario: Stored settings view
- **WHEN** the user opens the Config tab
- **THEN** the path of `settings.json` is shown at the bottom of the tab, and neither the file's contents nor any secret appears on the tab

#### Scenario: Reset
- **WHEN** the user edits fields on the Config tab and presses Reset
- **THEN** the fields return to the stored values and nothing is saved

#### Scenario: Location with every card collapsed
- **WHEN** every card of the Config tab is collapsed
- **THEN** the location of the settings file is still shown

### Requirement: The install dialog opens when the wake word cannot work
The window SHALL open a modal install dialog when the bot joins a voice channel in
wake-word mode while the selected model's files or the Vosk library are missing, and when
the user switches the activation to wake word with them missing. The window's start SHALL
NOT open it. The dialog SHALL name the model's language and each missing file with its
size and site. One dialog at most SHALL be open.

#### Scenario: Start on a clean folder
- **WHEN** the app starts for the first time in an empty folder
- **THEN** no dialog opens, and the Status tab's Wake word line and the notice under Wake model in Config → Assistant say that the Vosk library and the English model are needed

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

#### Scenario: Assistant card collapsed
- **WHEN** the Assistant card is collapsed and the user presses Not now in the dialog
- **THEN** the dialog closes, the card stays collapsed, and expanding it shows the download notice
