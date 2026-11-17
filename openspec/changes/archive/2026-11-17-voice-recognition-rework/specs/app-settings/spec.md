# Spec Delta

## MODIFIED Requirements

### Requirement: Wake phrases per model
The `wake` section SHALL hold `phrases`, a map from a model id to its `activation` and
`deactivation` lists. A list that is absent SHALL mean the model's default phrases; a list
that is present, even empty, SHALL be used as stored. A save SHALL change only the lists it
names. The phrases of a model SHALL stay stored while another model is selected and while a
local model's folder is missing.

#### Scenario: Switching models keeps the phrases
- **WHEN** the user sets Russian phrases, selects the English model and later the Russian model again
- **THEN** the Russian phrases are the ones set before

#### Scenario: One model's list saved
- **WHEN** a save sets the English model's activation list to `["bro"]`
- **THEN** only that list changes, the English deactivation list stays absent and keeps the default, and the Russian model's phrases are untouched

#### Scenario: A settings file without phrases
- **WHEN** the app loads a settings file whose `wake.phrases` has no entry for the selected model
- **THEN** the file is not rewritten for it, and the model's default phrases are in effect

#### Scenario: An empty list stays empty
- **WHEN** a save sets the English model's deactivation list to `[]`
- **THEN** the file holds the empty list and no default deactivation phrase is used for the English model

## ADDED Requirements

### Requirement: Phrases are compared in their matching form
Validation SHALL compare wake phrases in the matching form the wake engine uses
(lower-case, "ё" as "е", the words separated by single spaces). The phrases SHALL be stored
as entered.

#### Scenario: The same phrase written two ways in both lists
- **WHEN** a saved document has "Братан всё" as an activation phrase and "братан  все" as a deactivation phrase of one model
- **THEN** the save is rejected, the message names the model and both lists, and the previous settings stay in effect

#### Scenario: A phrase kept as typed
- **WHEN** a saved document has "Отбой братан" as a deactivation phrase
- **THEN** the save succeeds and the settings file holds "Отбой братан"
