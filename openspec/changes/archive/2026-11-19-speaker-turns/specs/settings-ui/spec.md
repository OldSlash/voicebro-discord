## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Access (who the assistant hears, privacy policy URL); Audio (both gains,
earcons and their volume); Assistant (session mode, idle timeout, wake engine and model,
activation and deactivation phrases, one speaker at a time); Codex (refresh the lists at
start, model, reasoning effort, voice, answer handoff, speaker labels, save chat history,
post answers to Discord and its channel, instructions); Hotkey (mute), in that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Access, Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab
- **THEN** it shows the cards "Discord and Codex", Access, Audio, Assistant, Codex and Hotkey, in that order

### Requirement: Switches are large
Every on/off setting SHALL be a switch control with an "On" or "Off" label next to it:
Earcons, One speaker at a time, Save chat history, Refresh the lists at start, Memory
measurement, Verbose log, and Performance monitoring both in the Diagnostics card and on
the Performance tab. The switch SHALL be at least 44 × 24 px, SHALL toggle when the switch
or its On/Off label is clicked, SHALL toggle with Space when focused, and SHALL show a
visible focus mark.

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

### Requirement: One speaker at a time in the Assistant card
The Assistant card SHALL hold the switch "One speaker at a time", off by default and saved
on change, in both assistant modes. Its help line SHALL say that while on the assistant
hears one person at a time in the voice mode (others wait and follow), that a question
during an answer waits instead of stopping it in the Whisper mode, and that the
deactivation phrase and Mute still stop an answer.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.one_speaker_at_a_time`
- **THEN** the Assistant card shows the switch off

#### Scenario: Turned on
- **WHEN** the user clicks the switch
- **THEN** it reads "On" and `assistant.one_speaker_at_a_time` is saved as `true` without a button
