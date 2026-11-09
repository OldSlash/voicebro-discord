# Spec Delta

## Purpose

Lets the user configure everything the app does from its window, with immediate effect and
clear messages, so the settings file never has to be edited by hand.

## ADDED Requirements

### Requirement: Every setting is editable
The Settings tab SHALL show a control for every field of the settings document: Discord
token (write-only with set and clear), guild id, auto-connect; session mode, idle timeout,
earcons and volume; wake engine, wake model, activation phrases, deactivation phrases;
assistant speaker labels; Codex executable, model, reasoning effort, voice, instructions;
mute hotkey.

#### Scenario: Field for each setting
- **WHEN** the Settings tab is opened
- **THEN** each field above is present and shows the current value

### Requirement: Save on change with validation
A change SHALL be saved when the control loses focus or is confirmed; a rejected value SHALL
show the validation message next to that field and leave the stored value unchanged; an
accepted value SHALL be applied at once.

#### Scenario: Invalid timeout
- **WHEN** the user enters 3 as the idle timeout
- **THEN** a message naming the allowed range appears at the field and the stored timeout is unchanged

#### Scenario: Phrase overlap
- **WHEN** the user enters a deactivation phrase that is also an activation phrase
- **THEN** the message names the shared phrase and both lists

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
