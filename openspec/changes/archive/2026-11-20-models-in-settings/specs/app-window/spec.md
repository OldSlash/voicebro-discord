# Spec Delta

## MODIFIED Requirements

### Requirement: Config tab
The Config tab SHALL hold the "Discord and Codex" card with the Discord token (masked,
write-only, with help on getting one), the guild id (with help on copying it), the Codex
executable, an "Auto-start enabled" note, Save changes and Reset; below it the bot's
Access, Audio, Assistant, engine and Hotkey cards; and at the bottom one line with the
location of the settings file. The tab SHALL NOT show the contents of the settings file,
and SHALL never show the token.

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
The window SHALL open a modal install dialog when a `/join` in wake-word mode was refused
because the selected model's files or the Vosk library are missing. It SHALL NOT open at
the window's start, when the activation is switched to wake word, or when a model is
picked. The dialog SHALL name the model's language and each missing file with its size and
site. One dialog at most SHALL be open.

#### Scenario: Start on a clean folder
- **WHEN** the app starts for the first time in an empty folder
- **THEN** no dialog opens, and the Status tab's Wake word line and the notice under Wake model in Config → Assistant say that the Vosk library and the English model are needed

#### Scenario: The bot joins with files missing
- **WHEN** the app runs in wake-word mode with the selected model's files missing and a member runs `/join`
- **THEN** the bot stays out of the voice channel, and the dialog opens, says the English wake model is not installed, and lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com)

#### Scenario: Every join asks again
- **WHEN** the user closed the dialog with Not now, and a member runs `/join` again with the files still missing
- **THEN** the join is refused again and the dialog opens again

#### Scenario: Switched to wake word
- **WHEN** the activation is always listening, nothing is on disk, and the user switches it to wake word
- **THEN** the setting is saved, no dialog opens, and the notice under Wake model lists the missing files

#### Scenario: Always listening
- **WHEN** the app starts or a member runs `/join` in always-listening mode with nothing on disk
- **THEN** no dialog opens and the bot joins

#### Scenario: Files on disk
- **WHEN** a member runs `/join` in wake-word mode and the selected model and the Vosk library are on disk
- **THEN** the bot joins and no dialog opens

#### Scenario: Assistant card collapsed
- **WHEN** the Assistant card is collapsed and the user presses Not now in the dialog
- **THEN** the dialog closes, the card stays collapsed, and expanding it shows the download notice

### Requirement: When the Whisper install dialog opens
The install dialog with its Whisper subject SHALL open only when a join in Whisper mode was
refused for missing files. It SHALL NOT open at the window's start, when the assistant mode
is switched to Whisper, or when a Whisper model, device or voice is picked, in either mode:
the choice is saved at once and its notice and Download button stay on its card.

#### Scenario: Refused join
- **WHEN** a member runs `/join` in Whisper mode on a PC with nothing of Whisper downloaded
- **THEN** the dialog opens listing the CPU runtime, the selected model, the VAD model and the voice with their sizes and the total

#### Scenario: Start
- **WHEN** the app starts in Whisper mode with files missing
- **THEN** no dialog opens, and the notices of Settings → Whisper and Settings → Local voice list what is missing

#### Scenario: Switched to Whisper
- **WHEN** the user switches the assistant mode to Whisper with files missing
- **THEN** no dialog opens, and Config → Assistant shows the note with "Open Settings"

#### Scenario: A model picked in the Whisper mode
- **WHEN** the mode is Whisper and the user picks a Whisper model that is not on disk
- **THEN** no dialog opens, the model is saved, and the Whisper card's notice lists its files
