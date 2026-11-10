# settings-ui Specification

## Purpose
Lets the user configure everything the app does from its window, with immediate effect and
clear messages, so the settings file never has to be edited by hand.

## Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split as in the first version.
The Settings tab: Appearance (theme, font size); Audio (input and output gain, earcons and
volume); Assistant (session mode, idle timeout, wake engine, wake model, activation phrases,
deactivation phrases); Codex (model, reasoning effort, voice, instructions, speaker labels);
Hotkey (mute). The Config tab: Discord token (write-only with set and clear), guild id, Codex
executable. There is no auto-connect control: the bot always connects when a token is stored.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: No auto-connect control
- **WHEN** the Settings or Config tab is opened
- **THEN** no control for connecting at start-up is shown, and the Config tab says auto-start is enabled

### Requirement: Save on change with validation
On the Settings tab a change SHALL be saved automatically, as in the first version: a text
field when it loses focus or on Enter, a slider when released, a list or switch when
changed. On the Config tab changes SHALL be saved with Save changes and discarded with Reset.
A rejected value SHALL stay in the field with the validation message next to it while the
stored value stays unchanged; an accepted value SHALL be applied at once.

#### Scenario: Invalid timeout
- **WHEN** the user enters 3 as the idle timeout and leaves the field
- **THEN** a message naming the allowed range appears at the field, the field still shows 3, and the stored timeout is unchanged

#### Scenario: Phrase overlap
- **WHEN** the user enters a deactivation phrase that is also an activation phrase
- **THEN** the message names the shared phrase and both lists

#### Scenario: List choice saved
- **WHEN** the user picks another Codex model from its list
- **THEN** the choice is saved without pressing a button and a restart shows it

### Requirement: Lists come from the live sources when available
The model and reasoning effort controls SHALL offer the values from Codex's model list when
the assistant is available, and free text otherwise; the voice control SHALL offer the
voices for the account; the wake model control SHALL offer the catalog entries. Next to the
model and reasoning controls the tab SHALL say that they affect only the work the voice model
delegates to the Codex agent.

#### Scenario: Codex available
- **WHEN** the assistant process is running and signed in
- **THEN** the model control lists the models with their reasoning efforts and the current one is selected

#### Scenario: Codex unavailable
- **WHEN** the assistant process is not running
- **THEN** the model and effort controls accept free text and the last saved values are shown

### Requirement: Phrases are lists
The phrase controls SHALL accept one phrase per line or comma-separated, show them trimmed
and lower-cased after save, and show the words the wake model does not know.

#### Scenario: Unknown word
- **WHEN** a saved phrase contains a word the model does not know
- **THEN** the word is listed under the field as unknown to the model

### Requirement: Audio card
The Settings tab SHALL have an "Audio" card with a slider for "What the assistant hears"
(the input gain) and one for "How loud the assistant speaks" (the output gain), each from 0
to 400 % with its current percentage shown, plus the earcon switch and the earcon volume. A
change SHALL be saved automatically, without a Save button.

#### Scenario: Moving a slider
- **WHEN** the user moves "What the assistant hears" to 339 %
- **THEN** the label shows 339 %, the setting is saved as 3.39 without pressing a button, and a restart shows the same value

#### Scenario: Rejected value
- **WHEN** a save fails
- **THEN** the card shows the error next to the slider and the slider returns to the stored value

### Requirement: Edits survive refreshes
A field SHALL not be changed by periodic or event refreshes while it has focus, while it
holds a value that differs from the stored one, or while its list is open. Lists SHALL keep
their current selection when they are rebuilt.

#### Scenario: Events during a choice
- **WHEN** the user opens the wake model list and status events arrive for 30 s
- **THEN** the list stays open and the highlighted choice does not change

#### Scenario: Unsaved text
- **WHEN** the user types into the Config tab's guild id and an event arrives before Save changes
- **THEN** the typed text stays in the field

### Requirement: Save state is visible
Each field SHALL show whether its value is being saved, was saved (for about two seconds) or
was rejected; a field on the Config tab with an unsaved change SHALL be marked as changed
until Save changes or Reset.

#### Scenario: Successful save
- **WHEN** a value is saved
- **THEN** the field shows "Saved" for about two seconds and then no marker

#### Scenario: Unsaved Config change
- **WHEN** the user edits the Codex executable on the Config tab
- **THEN** the field is marked as changed until Save changes or Reset
