# Spec Delta

## ADDED Requirements

### Requirement: Diagnostics settings
The settings SHALL hold a `diagnostics` section with `performance` and `memory`, each a
boolean with the default `false`. A settings file without the section SHALL load with both
off, and the next save SHALL write the section. A document whose value is not a boolean
SHALL be rejected with the stored settings unchanged.

#### Scenario: Older file
- **WHEN** the settings file has no `diagnostics` section
- **THEN** both switches are off and the next save writes `diagnostics` with both fields

#### Scenario: Valid change
- **WHEN** the user saves `diagnostics.memory` as `true`
- **THEN** the save succeeds and the stored settings hold it

#### Scenario: Wrong type
- **WHEN** a saved document sets `diagnostics.performance` to the string `yes`
- **THEN** the save is rejected and the previous settings stay in effect

### Requirement: Diagnostics changes apply at once
A save that changes only the `diagnostics` section SHALL take effect without a restart and
SHALL restart nothing else: not the Discord connection, the wake engine, the assistant or a
running call.

#### Scenario: Only diagnostics changed
- **WHEN** a save changes `diagnostics.memory` and nothing else
- **THEN** the memory reading starts or stops, and the Discord connection, wake engine, assistant and call keep running untouched
