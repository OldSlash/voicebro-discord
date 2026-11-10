# Spec Delta

## MODIFIED Requirements

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

## ADDED Requirements

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
