# Spec Delta

## MODIFIED Requirements

### Requirement: Phrases are lists
The phrase controls SHALL show and save the phrases of the model selected in the wake model
control, with that model's name in their labels. Phrases SHALL be separated by commas (a line
break also separates); a phrase is one or more words separated by spaces. After a save they
SHALL show the phrases trimmed and joined with ", ", the model's default phrases for a list
never set, and the words the wake model does not know. Selecting another model SHALL show
that model's phrases.

#### Scenario: Unknown word
- **WHEN** a saved phrase contains a word the model does not know
- **THEN** the word is listed under the field as unknown to the model

#### Scenario: Switching the model
- **WHEN** the user has Russian phrases and selects the English model
- **THEN** the phrase fields show the English model's phrases (its default phrases the first time) and their labels name the English model; selecting the Russian model again shows the Russian phrases

#### Scenario: Phrases saved for the selected model
- **WHEN** the user enters "bro" as an activation phrase with the English model selected and leaves the field
- **THEN** the phrase is saved for the English model only

#### Scenario: Several phrases of several words
- **WHEN** the user enters "Отбой братан, Заканчивай, Братан всё" in the deactivation field of the Russian model and leaves the field
- **THEN** three phrases are saved in that order ("Отбой братан", "Заканчивай", "Братан всё") and the field shows "Отбой братан, Заканчивай, Братан всё"

#### Scenario: Default phrases shown
- **WHEN** the window opens on a first start with the English model selected
- **THEN** the activation field shows "bro" and the deactivation field shows "peace out"

#### Scenario: A field emptied
- **WHEN** the user empties the deactivation field and leaves it
- **THEN** an empty list is saved, the field stays empty, and the default does not come back

#### Scenario: Help text
- **WHEN** the user reads the help under the phrase fields
- **THEN** it says that phrases are separated by commas, that a phrase may have several words, that every word must be in the model's vocabulary, and that each model comes with default phrases
