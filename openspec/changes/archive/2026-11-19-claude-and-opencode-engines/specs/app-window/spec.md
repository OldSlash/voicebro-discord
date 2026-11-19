## MODIFIED Requirements

### Requirement: Tabs
The window SHALL have the tabs Console, Chat, Status, Performance, System, Settings and
Config, in that order, and SHALL keep each tab's content and scroll position when the user
switches between them.

#### Scenario: Switching tabs
- **WHEN** the user scrolls the Settings tab, opens Status and returns to Settings
- **THEN** the Settings tab shows the same position and the same unsaved input as before

#### Scenario: Whisper tab
- **WHEN** the window opens
- **THEN** there is no Whisper tab: the last tab is Config, and the Whisper mode's fields are on the Config and Settings tabs

#### Scenario: Chat tab name
- **WHEN** the window opens with any engine
- **THEN** the second tab is named "Chat", not "Codex chat"

### Requirement: Config tab
The Config tab SHALL hold the "Discord and Codex" card with the Discord token (masked,
write-only, with help on getting one), the guild id (with help on copying it), the Codex
executable, an "Auto-start enabled" note, Save changes and Reset; below it the bot's
Access, Audio, Assistant, Local voice, engine and Hotkey cards; and at the bottom one line
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
