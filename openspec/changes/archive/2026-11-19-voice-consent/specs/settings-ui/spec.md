## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Access (who the assistant hears, privacy policy URL); Audio (both gains,
earcons and their volume); Assistant (session mode, idle timeout, wake engine and model,
activation and deactivation phrases); Codex (refresh the lists at start, model, reasoning
effort, voice, answer handoff, speaker labels, save chat history, post answers to Discord
and its channel, instructions); Hotkey (mute), in that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Access, Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab
- **THEN** it shows the cards "Discord and Codex", Access, Audio, Assistant, Codex and Hotkey, in that order

### Requirement: Access card
The Config tab SHALL have an Access card after "Discord and Codex" with the list "Who the
assistant hears" (Everyone, People who agreed, Only my Discord account), the field "Privacy
policy URL", both saved on change, and the link state: the owner's Discord name, or "not
linked" with the reason when a link ended. The list's help line SHALL say what each choice
lets through, that everyone can still use the bot's commands and hears the answers, and
that a change applies at once.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `discord.access` or `discord.owner_only` and no link file
- **THEN** the Access card shows "Everyone", an empty privacy policy URL, "not linked" and a Link button

#### Scenario: Linked
- **WHEN** an owner is linked
- **THEN** the card shows the owner's Discord name and an Unlink button, and no Link code

#### Scenario: Switch on without a link
- **WHEN** the user picks "Only my Discord account" while no owner is linked
- **THEN** the setting is saved and the card says that the assistant hears nobody until an account is linked

#### Scenario: Agreed mode without a policy
- **WHEN** the user picks "People who agreed" while the privacy policy URL is empty
- **THEN** the setting is saved and the card warns that `/optin` is refused until a privacy policy URL is set

#### Scenario: A wrong URL
- **WHEN** the user types `example.org/privacy` into the privacy policy URL and leaves the field
- **THEN** nothing is saved and the field says the URL must start with `https://`

### Requirement: Switches are large
Every on/off setting SHALL be a switch control with an "On" or "Off" label next to it:
Earcons, Save chat history, Refresh the lists at start, Memory measurement, Verbose log,
and Performance monitoring both in the Diagnostics card and on the Performance tab. The
switch SHALL be at least 44 × 24 px, SHALL toggle when the switch or its On/Off label is
clicked, SHALL toggle with Space when focused, and SHALL show a visible focus mark.

#### Scenario: Size
- **WHEN** the Diagnostics card is shown at the default font size
- **THEN** the "Performance monitoring" switch measures at least 44 × 24 px

#### Scenario: Clicking the label
- **WHEN** the user clicks the "Off" label next to the "Memory measurement" switch
- **THEN** the switch turns on, the label reads "On", and the setting is saved without a button

#### Scenario: Keyboard
- **WHEN** the "Verbose log" switch has the focus and the user presses Space
- **THEN** the switch changes and the setting is saved

#### Scenario: Switch next to a slider
- **WHEN** the Audio card is shown
- **THEN** the Earcons switch and the Earcon volume slider have their names on one line and their controls on one line

## ADDED Requirements

### Requirement: People who agreed in the Access card
The Access card SHALL list every person with a consent record, by Discord name with the
time they agreed and a Remove button, in every access mode, and SHALL say "Nobody yet"
when the list is empty. The list SHALL follow `/optin`, `/optout` and Remove within a
second. When the records file could not be read, the card SHALL say so.

#### Scenario: A new record
- **WHEN** a member presses "I agree" in Discord while the window is open
- **THEN** the card lists their name and time within a second

#### Scenario: Empty list
- **WHEN** nobody has a record
- **THEN** the list says "Nobody yet" and that people agree with `/optin` in Discord

#### Scenario: Unreadable file
- **WHEN** the app started with a records file it could not read
- **THEN** the card says the records could not be read and lists nobody
