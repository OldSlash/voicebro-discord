# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
The Settings tab SHALL show a control for every field of the settings document: Discord
token (write-only with set and clear), guild id; session mode, idle timeout, earcons and
volume; wake engine, wake model, activation phrases, deactivation phrases; assistant speaker
labels; Codex executable, model, reasoning effort, voice, instructions; mute hotkey. There is
no auto-connect control: the bot always connects when a token is stored.

#### Scenario: Field for each setting
- **WHEN** the Settings tab is opened
- **THEN** each field above is present and shows the current value

#### Scenario: No auto-connect control
- **WHEN** the Settings tab is opened
- **THEN** no control for connecting at start-up is shown
