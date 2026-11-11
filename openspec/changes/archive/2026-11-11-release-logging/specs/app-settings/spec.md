# Spec Delta

## ADDED Requirements

### Requirement: Log settings
The settings SHALL hold `diagnostics.verbose_log` and `assistant.save_chat_history`, each a
boolean with the default `false`. A settings file without them SHALL load with both off. A
document whose value is not a boolean SHALL be rejected with the stored settings unchanged.
A change SHALL apply at once and SHALL restart nothing else: not the Discord connection, the
wake engine, the assistant or a running call.

#### Scenario: Older file
- **WHEN** the settings file has neither field
- **THEN** both are off and the next save writes them

#### Scenario: Wrong type
- **WHEN** a saved document sets `assistant.save_chat_history` to the string `yes`
- **THEN** the save is rejected and the previous settings stay in effect

#### Scenario: Only the verbose log changed
- **WHEN** a save changes `diagnostics.verbose_log` and nothing else during a call
- **THEN** the file log follows the new value, and the Discord connection, wake engine, assistant and call keep running untouched
