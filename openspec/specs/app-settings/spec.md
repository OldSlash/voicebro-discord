# app-settings Specification

## Purpose
Keeps every user-facing setting of the app in one validated document that survives upgrades
and is applied without a restart.

## Requirements

### Requirement: Settings live in one versioned document
The app SHALL keep all settings in a single JSON document with a schema `version` field,
stored as `settings.json` in the folder the executable runs from. The document SHALL contain
no secrets.

#### Scenario: First start without a settings file
- **WHEN** the app starts and no settings file exists
- **THEN** it runs with the default settings and writes them to the settings file with the current schema version

#### Scenario: Settings file never contains a secret
- **WHEN** a settings file is written for any reason
- **THEN** it contains no Discord token or other credential, only a flag that one is set is available through the status

### Requirement: Settings are validated in one place
The app SHALL validate a settings document before storing or applying it, rejecting the
whole document with a message that names the offending field.

#### Scenario: Overlapping phrase lists
- **WHEN** a saved document has a phrase that appears in both the activation and the deactivation list
- **THEN** the save is rejected, the message names both lists, and the previous settings stay in effect

#### Scenario: Idle timeout out of range
- **WHEN** a saved document sets the idle timeout to a value other than 0 or 5..3600 seconds
- **THEN** the save is rejected and the message names the field and the allowed range

#### Scenario: Unknown wake model
- **WHEN** a saved document names a wake model that is not in the model catalog, or whose engine differs from the chosen engine
- **THEN** the save is rejected and the message names the model and the engine

### Requirement: Settings are written atomically
The app SHALL write settings to a temporary file in the same directory and rename it over the
previous file, so an interrupted write never leaves a missing or truncated settings file.

#### Scenario: Interrupted write
- **WHEN** the process is killed while a settings save is in progress
- **THEN** on the next start the settings file is either the previous complete document or the new complete document, never empty or partial

### Requirement: Older settings documents are migrated
The app SHALL load a settings document from an older schema version, fill missing fields with
defaults, keep unknown fields, and write the current version back.

#### Scenario: Missing field from an older version
- **WHEN** the settings file lacks a field that the current schema has
- **THEN** the app uses the field's default and the next save writes the field with the current schema version

#### Scenario: Unknown field from a newer version
- **WHEN** the settings file has a field the current schema does not know
- **THEN** the app keeps the field unchanged through load and save

### Requirement: Saved settings apply at once
The app SHALL apply a successfully saved document immediately, reconfiguring only the parts
whose section changed, without a restart.

#### Scenario: Only one section changed
- **WHEN** a save changes the idle timeout and nothing else
- **THEN** the session timeout is updated and the Discord connection, wake engine and assistant are not restarted

#### Scenario: Status reflects the new values
- **WHEN** a save succeeds
- **THEN** the next status snapshot and settings read return the saved values
