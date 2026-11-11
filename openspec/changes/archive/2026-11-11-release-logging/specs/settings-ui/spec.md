# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs as in the first version. The Settings tab: Appearance (theme, font size); Audio
(both gains, earcons and their volume); Assistant (session mode, idle timeout, wake engine
and model, activation and deactivation phrases); Codex (model, reasoning effort, voice,
instructions, speaker labels, save chat history); Hotkey (mute); Diagnostics (performance
monitoring, memory measurement, verbose log).

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

## ADDED Requirements

### Requirement: Verbose log switch
The Diagnostics card SHALL have a "Verbose log" switch, off by default, saved automatically
and applied at once. Its help line SHALL say that it adds what the recognizer heard, the
latency and call counters, and the chat text to the log file, that the log folder stays
within about 2 MB, and that it is meant for diagnosing a problem.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `diagnostics.verbose_log`
- **THEN** the switch is shown off

#### Scenario: Turned on
- **WHEN** the user turns "Verbose log" on
- **THEN** the setting is saved without a button and the help line is shown under the switch

### Requirement: Chat history switch
The Codex card SHALL have a "Save chat history" switch, off by default, saved
automatically and applied at once. Its help line SHALL say that while it is on every chat
entry is appended to `logs\codex-chat-<date>.log`, that while it is off nothing of the chat
is saved and the Codex chat tab still shows it, and that files saved earlier stay.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.save_chat_history`
- **THEN** the switch is shown off

#### Scenario: Turned on
- **WHEN** the user turns "Save chat history" on
- **THEN** the setting is saved without a button and a restart shows it on
