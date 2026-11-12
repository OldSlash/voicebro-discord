# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between two tabs.
The Settings tab: Appearance (theme, font size); Audio (input and output gain, earcons and
volume); Assistant (session mode, idle timeout, wake engine, wake model, activation phrases,
deactivation phrases); Codex (model, reasoning effort, voice, instructions, speaker labels);
Hotkey (mute); Diagnostics (performance monitoring, memory measurement). The Config tab:
Discord token (write-only with set and clear), guild id, Codex executable. There is no
auto-connect control: the bot always connects when a token is stored.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: No auto-connect control
- **WHEN** the Settings or Config tab is opened
- **THEN** no control for connecting at start-up is shown, and the Config tab says auto-start is enabled

### Requirement: Save state is visible
A field whose value was just saved SHALL be marked on the field itself, without any text
line at the field: its name takes the success colour for about two seconds and then returns
to normal. The Settings tab's single feedback line SHALL name the last saved field for the
same two seconds and then read "Changes save automatically" again. A rejected value SHALL
show its message at the field. A field on the Config tab with an unsaved change SHALL be
marked as changed until Save changes or Reset.

#### Scenario: Successful save
- **WHEN** the font size is saved
- **THEN** the field's name shows the success colour for about two seconds, the feedback line reads "Saved: Font size" for the same time, and no text appears at the field

#### Scenario: Rejected value
- **WHEN** a value is rejected
- **THEN** the message appears at that field and stays until the field is corrected

#### Scenario: Unsaved Config change
- **WHEN** the user edits the Codex executable on the Config tab
- **THEN** the field is marked as changed until Save changes or Reset

## ADDED Requirements

### Requirement: A save moves no other field
Marking a field as saved SHALL NOT change the position or size of any field, note or card.
The two fields of a row SHALL keep their names and controls at the same height, whatever
their control type.

#### Scenario: Save in a two-column row
- **WHEN** "What the assistant hears" is saved while "How loud the assistant speaks" sits next to it
- **THEN** the second field, the note under the row and the cards below keep their positions

#### Scenario: Switch next to a slider
- **WHEN** the Audio card is shown
- **THEN** the Earcons switch and the Earcon volume slider have their names on one line and their controls on one line

### Requirement: Every setting has a help line
Every control on the Settings and Config tabs SHALL have its own help line under it that
says what the setting affects, when a change takes effect (at once, from the next call, at
the next session, or at the next connection) and the default where one exists.

#### Scenario: Every control described
- **WHEN** the Settings and Config tabs are opened
- **THEN** every control has a non-empty help line of its own

#### Scenario: Voice
- **WHEN** the user reads the help of the Voice control
- **THEN** it says the voice is the one the realtime call speaks with, that an empty value is the server's default voice, and that a change applies from the next call

#### Scenario: Activation
- **WHEN** the user reads the help of the Activation control
- **THEN** it says that in wake-word mode a session starts at an activation phrase and ends at a deactivation phrase or the auto-stop with nothing sent between sessions, that in always-listening mode the assistant hears the channel and the call runs the whole time the bot is in it, and that a change applies at once

#### Scenario: Gain
- **WHEN** the user reads the help of "What the assistant hears"
- **THEN** it says the gain scales only what the assistant receives during a session, that the wake word and the speech detection hear the channel unchanged, that 100 % is unchanged, and that a change applies at once, also in a running call

### Requirement: Diagnostics card
The Settings tab SHALL have a Diagnostics card with the switches "Performance monitoring"
and "Memory measurement", both off by default, each with a help line that names what it
measures, what it costs while on, and that it is off by default. A change SHALL be saved
automatically like the other switches and SHALL take effect at once. "Performance
monitoring" is the same setting as the switch on the Performance tab: the two SHALL always
show the same value.

#### Scenario: Mirrored switch
- **WHEN** the user turns performance monitoring on from the Performance tab and opens Settings
- **THEN** the Diagnostics card shows "Performance monitoring" on

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `diagnostics` section
- **THEN** both switches are shown off

#### Scenario: Turning performance monitoring on
- **WHEN** the user turns "Performance monitoring" on
- **THEN** the setting is saved without a button, and the Performance tab starts sampling within a second without a restart

#### Scenario: Turning memory measurement off
- **WHEN** the user turns "Memory measurement" off
- **THEN** the setting is saved without a button, the status bar's memory item disappears, and no further memory reading is taken
