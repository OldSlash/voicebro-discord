# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Access (owner only); Audio (both gains, earcons and their volume);
Assistant (session mode, idle timeout, wake engine and model, activation and deactivation
phrases); Codex (refresh the lists at start, model, reasoning effort, voice, answer handoff,
speaker labels, save chat history, post answers to Discord and its channel, instructions);
Hotkey (mute), in that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Access, Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab
- **THEN** it shows the cards "Discord and Codex", Access, Audio, Assistant, Codex and Hotkey, in that order

### Requirement: Config tab fields
The Config tab's "Discord and Codex" card SHALL hold the Discord token (write-only, with set
and clear), the guild id and the Codex executable. The Access, Audio, Assistant, Codex and
Hotkey cards follow it on the same tab.

#### Scenario: Config fields
- **WHEN** the Config tab is opened
- **THEN** the token field shows whether a token is stored but never the token, and the guild id and the Codex executable show their stored values

### Requirement: Collapsible cards on the Settings and Config tabs
Every card of the Settings tab (Appearance, Diagnostics) and of the Config tab ("Discord and
Codex", Access, Audio, Assistant, Codex, Hotkey) SHALL collapse to its title row and expand
again when the user activates the title, by mouse or keyboard. The title SHALL show whether
the card is collapsed. The state SHALL be saved and restored at the next start; every card
SHALL be expanded by default.

#### Scenario: Collapsing a card
- **WHEN** the user clicks the title of the Audio card
- **THEN** the card shows only its title row, and the cards below move up

#### Scenario: Collapsing a Settings card
- **WHEN** the user clicks the title of the Appearance card on the Settings tab
- **THEN** the card shows only its title row, the Diagnostics card moves up, and a restart shows Appearance collapsed

#### Scenario: Keyboard
- **WHEN** the title of the Hotkey card has the focus and the user presses Enter or Space
- **THEN** the card collapses, and the same key expands it again

#### Scenario: After a restart
- **WHEN** the user collapsed the Assistant and Hotkey cards and restarts the app
- **THEN** the Config tab opens with those two cards collapsed and the others expanded

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `ui.collapsed_cards`
- **THEN** every card of the Settings and Config tabs is expanded

#### Scenario: A collapsed card keeps its fields
- **WHEN** the guild id has an unsaved edit and the user collapses and expands "Discord and Codex"
- **THEN** the edit is still in the field and marked as changed

#### Scenario: A collapsed Access card keeps its code
- **WHEN** a link code is waiting and the user collapses and expands the Access card
- **THEN** the same code and its countdown are shown again

### Requirement: Switches are large
Every on/off setting SHALL be a switch control with an "On" or "Off" label next to it: Only
my Discord account, Earcons, Save chat history, Refresh the lists at start, Memory
measurement, Verbose log, and Performance monitoring both in the Diagnostics card and on the
Performance tab. The switch SHALL be at least 44 × 24 px, SHALL toggle when the switch or
its On/Off label is clicked, SHALL toggle with Space when focused, and SHALL show a visible
focus mark.

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

### Requirement: Access card
The Config tab SHALL have an Access card after "Discord and Codex" with the switch "Only my
Discord account", saved on change, and the link state: the owner's Discord name, or "not
linked" with the reason when a link ended. Its help line SHALL say that while on only the
linked account's voice reaches the assistant, that everyone can still use the bot's
commands and hears the answers, and that a change applies at once.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `discord.owner_only` and no link file
- **THEN** the Access card shows the switch off, "not linked" and a Link button

#### Scenario: Linked
- **WHEN** an owner is linked
- **THEN** the card shows the owner's Discord name and an Unlink button, and no Link code

#### Scenario: Switch on without a link
- **WHEN** the user turns the switch on while no owner is linked
- **THEN** the setting is saved and the card says that the assistant hears nobody until an account is linked

### Requirement: Link code in the Access card
While a code is waiting, the Access card SHALL show the code, the command to type with it,
the time left and a Cancel button. When the code ends without a link, the card SHALL say why:
expired, cancelled or ended after 5 wrong tries.

#### Scenario: Code shown
- **WHEN** the user presses Link while Codex is signed in
- **THEN** the card shows an 8-character code, `/link` with that code, a countdown from 10 minutes and a Cancel button

#### Scenario: Link completed
- **WHEN** the owner sends the code from Discord while the card shows it
- **THEN** the code disappears and the card shows the owner's name within a second

#### Scenario: Cancel
- **WHEN** the user presses Cancel
- **THEN** the code disappears, a later `/link` with it links nothing, and the card shows the previous link state
