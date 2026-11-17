## MODIFIED Requirements

### Requirement: Tabs
The window SHALL have the tabs Console, Codex chat, Status, Performance, System, Settings and
Config, in that order, and SHALL keep each tab's content and scroll position when the user
switches between them.

#### Scenario: Switching tabs
- **WHEN** the user scrolls the Settings tab, opens Status and returns to Settings
- **THEN** the Settings tab shows the same position and the same unsaved input as before

#### Scenario: Whisper tab
- **WHEN** the window opens
- **THEN** there is no Whisper tab: the last tab is Config, and the Whisper mode's fields are on the Config and Settings tabs

### Requirement: Config tab
The Config tab SHALL hold the "Discord and Codex" card with the Discord token (masked,
write-only, with help on getting one), the guild id (with help on copying it), the Codex
executable, an "Auto-start enabled" note, Save changes and Reset; below it the bot's
Access, Audio, Assistant, Local voice, Codex and Hotkey cards; and at the bottom one line
with the location of the settings file. The tab SHALL NOT show the contents of the settings
file, and SHALL never show the token.

#### Scenario: Stored settings view
- **WHEN** the user opens the Config tab
- **THEN** the path of `settings.json` is shown at the bottom of the tab, and neither the file's contents nor any secret appears on the tab

#### Scenario: Reset
- **WHEN** the user edits fields on the Config tab and presses Reset
- **THEN** the fields return to the stored values and nothing is saved

#### Scenario: Location with every card collapsed
- **WHEN** every card of the Config tab is collapsed
- **THEN** the location of the settings file is still shown

### Requirement: When the Whisper install dialog opens
The install dialog with its Whisper subject SHALL open:

- when a join in Whisper mode was refused for missing files;
- when the assistant mode is switched to Whisper with files missing;
- when a Whisper model, device or voice that needs files is chosen while the mode is
  Whisper.

The window's start SHALL NOT open it. In the realtime mode a choice SHALL save at once
without the dialog; its notice and Download button stay on the card.

#### Scenario: Refused join
- **WHEN** a member runs `/join` in Whisper mode on a PC with nothing of Whisper downloaded
- **THEN** the dialog opens listing the CPU runtime, the selected model, the VAD model and the voice with their sizes and the total

#### Scenario: Start
- **WHEN** the app starts in Whisper mode with files missing
- **THEN** no dialog opens, and the notices of Config → Assistant and Config → Local voice list what is missing
