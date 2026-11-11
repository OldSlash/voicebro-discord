## MODIFIED Requirements

### Requirement: Settings schema version 2
The app SHALL write schema version 2. Loading a version-1 document with phrases SHALL move
`wake.activation_phrases` and `wake.deactivation_phrases` under the entry of the model in
`wake.model`, or of `vosk-model-small-ru-0.22`, the only model of version 1, when it names
none. A version-1 document without phrases SHALL get the default English model. The two old
fields SHALL be removed, not kept as unknown fields.

#### Scenario: Version 1 with phrases
- **WHEN** a version-1 file selects the Russian model and has activation and deactivation phrases
- **THEN** after loading, both lists are under the Russian model's entry, the Russian model stays selected, the old fields are gone, and the next save writes version 2

#### Scenario: Version 1 without a model
- **WHEN** a version-1 file has phrases but no `wake.model`
- **THEN** the wake model becomes `vosk-model-small-ru-0.22` and the phrases are under its entry

#### Scenario: Version 1 without phrases
- **WHEN** a version-1 file has no phrase lists, or only empty ones, whether or not it names the Russian model
- **THEN** `wake.phrases` is empty, the wake model is `vosk-model-small-en-us-0.15`, and the old fields are gone
