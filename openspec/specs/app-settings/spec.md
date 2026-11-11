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
- **WHEN** a saved document has a phrase that appears in both the activation and the deactivation list of one model
- **THEN** the save is rejected, the message names the model and both lists, and the previous settings stay in effect

#### Scenario: The same phrase for two models
- **WHEN** a saved document has the same activation phrase for the English and the Russian model
- **THEN** the save succeeds

#### Scenario: Idle timeout out of range
- **WHEN** a saved document sets the idle timeout to a value other than 0 or 5..3600 seconds
- **THEN** the save is rejected and the message names the field and the allowed range

#### Scenario: Unknown wake model
- **WHEN** a saved document names a wake model that is neither in the model catalog nor a `local:` id with a plain folder name, or whose engine differs from the chosen engine
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

### Requirement: Discord guild id setting
The settings SHALL hold `discord.guild_id`, a string of digits or empty, used to scope the
slash command registration. A value that is not empty and not all digits SHALL be rejected
naming the field.

#### Scenario: Valid guild id
- **WHEN** the user saves `discord.guild_id` as a string of digits
- **THEN** the save succeeds and the next connection registers the commands in that guild

#### Scenario: Invalid guild id
- **WHEN** the user saves `discord.guild_id` containing letters
- **THEN** the save is rejected and the message names `discord.guild_id`

### Requirement: Audio gain settings
The settings SHALL hold `audio.input_gain` and `audio.output_gain`, each a number from 0.0
to 4.0 with the default 1.0. A value outside that range SHALL be rejected with a message
naming the field. A settings file without the `audio` section SHALL load with the defaults.

#### Scenario: Valid gains
- **WHEN** the user saves an input gain of 3.39 and an output gain of 2.52
- **THEN** the save succeeds and the next status shows both values

#### Scenario: Out of range
- **WHEN** a saved document sets `audio.input_gain` to 5
- **THEN** the save is rejected and the message names `audio.input_gain` and the range 0 to 4

#### Scenario: Older file
- **WHEN** the settings file has no `audio` section
- **THEN** both gains are 1.0 and the next save writes the section

### Requirement: Appearance settings
The settings SHALL hold `ui.theme` (one of `dark`, `darcula`, `light`, `intellij`, default
`dark`) and `ui.font_size` (11 to 20, default 13). Other values SHALL be rejected naming the
field.

#### Scenario: Valid appearance
- **WHEN** the user saves the theme `intellij` and the font size 15
- **THEN** the save succeeds and the window applies both

#### Scenario: Unknown theme
- **WHEN** a saved document sets `ui.theme` to `pink`
- **THEN** the save is rejected and the message names `ui.theme` and the allowed themes

### Requirement: Answer handoff mode setting
The settings SHALL hold `codex.handoff_mode`, one of `items`, `commentary`, `bem_tags` and
`thinking`, with the default `items`. Any other value SHALL be rejected with a message naming
the field and the allowed values. A settings file without the field SHALL load with `items`.

#### Scenario: Valid choice
- **WHEN** the user saves `codex.handoff_mode` as `commentary`
- **THEN** the save succeeds and the stored settings hold `commentary`

#### Scenario: Unknown value
- **WHEN** a saved document sets `codex.handoff_mode` to `verbatim`
- **THEN** the save is rejected and the message names `codex.handoff_mode` and the four allowed values

#### Scenario: Older file
- **WHEN** the settings file has a `codex` section without `handoff_mode`
- **THEN** the field is `items` and the next save writes it

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

### Requirement: Wake phrases per model
The `wake` section SHALL hold `phrases`, a map from a model id to its `activation` and
`deactivation` lists, both empty by default. A save SHALL change only the lists it names.
The phrases of a model SHALL stay stored while another model is selected and while a local
model's folder is missing.

#### Scenario: Switching models keeps the phrases
- **WHEN** the user sets Russian phrases, selects the English model and later the Russian model again
- **THEN** the Russian phrases are the ones set before

#### Scenario: One model's list saved
- **WHEN** a save sets the English model's activation list to `["bro"]`
- **THEN** only that list changes and the Russian model's phrases are untouched

### Requirement: Settings schema version 2
The app SHALL write schema version 2. Loading a version-1 document SHALL move
`wake.activation_phrases` and `wake.deactivation_phrases` under the entry of the model in
`wake.model`; a document without `wake.model` SHALL get `vosk-model-small-ru-0.22`, the
default of version 1. The two old fields SHALL be removed, not kept as unknown fields.

#### Scenario: Version 1 with phrases
- **WHEN** a version-1 file selects the Russian model and has activation and deactivation phrases
- **THEN** after loading, both lists are under the Russian model's entry, the old fields are gone, and the next save writes version 2

#### Scenario: Version 1 without a model
- **WHEN** a version-1 file has phrases but no `wake.model`
- **THEN** the wake model becomes `vosk-model-small-ru-0.22` and the phrases are under its entry

#### Scenario: Version 1 without phrases
- **WHEN** a version-1 file has no phrase lists
- **THEN** `wake.phrases` is empty and the wake model is the one the file names, or the Russian model when it names none
