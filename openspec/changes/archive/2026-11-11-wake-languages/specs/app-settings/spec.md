# Spec Delta

## MODIFIED Requirements

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

## ADDED Requirements

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
