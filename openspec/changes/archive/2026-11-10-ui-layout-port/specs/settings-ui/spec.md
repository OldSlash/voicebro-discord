# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between two tabs.
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
