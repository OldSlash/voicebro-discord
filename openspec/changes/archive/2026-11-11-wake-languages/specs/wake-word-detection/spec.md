# Spec Delta

## MODIFIED Requirements

### Requirement: Phrases come from the settings
The engine SHALL spot the activation and deactivation phrases stored for the selected model
in `wake.phrases`, lower-cased and trimmed, as whole words in the recognizer's
open-vocabulary output; phrases stored for other models SHALL be ignored. With both lists of
the selected model empty the engine SHALL stay idle and the status SHALL say that no
phrases are configured.

#### Scenario: No phrases configured
- **WHEN** wake-word mode is on and both phrase lists of the selected model are empty
- **THEN** the status shows "no phrases configured", nothing is loaded for recognition, and no session can start

#### Scenario: Phrases of another model
- **WHEN** only the Russian model has phrases and the user selects the English model
- **THEN** the status shows "no phrases configured" and the Russian phrases are not spotted

#### Scenario: Look-alike word
- **WHEN** a speaker says a longer word that contains an activation phrase as a prefix
- **THEN** the phrase does not count

#### Scenario: English activation
- **WHEN** the English model is selected with the activation phrase "bro" and a speaker says "hey bro, what time is it"
- **THEN** the activation is reported once and the question's words are still recognized
