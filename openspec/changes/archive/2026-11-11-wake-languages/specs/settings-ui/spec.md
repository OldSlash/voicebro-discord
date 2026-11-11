# Spec Delta

## MODIFIED Requirements

### Requirement: Lists come from the live sources when available
The model and reasoning effort controls SHALL offer the values from Codex's model list when
the assistant is available, and free text otherwise; the voice control SHALL offer the
voices for the account; the wake model control SHALL offer the entries of the wake model
list. Next to the model and reasoning controls the tab SHALL say that they affect only the
work the voice model delegates to the Codex agent.

#### Scenario: Codex available
- **WHEN** the assistant process is running and signed in
- **THEN** the model control lists the models with their reasoning efforts and the current one is selected

#### Scenario: Codex unavailable
- **WHEN** the assistant process is not running
- **THEN** the model and effort controls accept free text and the last saved values are shown

### Requirement: Phrases are lists
The phrase controls SHALL show and save the phrases of the model selected in the wake model
control, with that model's name in their labels. They SHALL accept one phrase per line or
comma-separated, show them trimmed and lower-cased after save, and show the words the wake
model does not know. Selecting another model SHALL show that model's phrases.

#### Scenario: Unknown word
- **WHEN** a saved phrase contains a word the model does not know
- **THEN** the word is listed under the field as unknown to the model

#### Scenario: Switching the model
- **WHEN** the user has Russian phrases and selects the English model
- **THEN** the phrase fields show the English model's phrases (empty the first time) and their labels name the English model; selecting the Russian model again shows the Russian phrases

#### Scenario: Phrases saved for the selected model
- **WHEN** the user enters "bro" as an activation phrase with the English model selected and leaves the field
- **THEN** the phrase is saved for the English model only

## ADDED Requirements

### Requirement: Wake model list
The wake model control SHALL list the catalog models and the local models found at start,
each with its language and whether it is downloaded, still to be downloaded (with its size)
or from the models folder. Its help line SHALL say where the models folder is, that small
Vosk models are the ones to use and that a restart picks up a new folder, and an "Open
models folder" button SHALL open the folder.

#### Scenario: Fresh install
- **WHEN** the app runs for the first time
- **THEN** the list shows the English and the Russian model, each marked as still to be downloaded with its size

#### Scenario: Local model
- **WHEN** a model folder was found in `models\` at start
- **THEN** the list shows it marked as from the models folder

#### Scenario: Opening the folder
- **WHEN** the user presses "Open models folder"
- **THEN** the folder `models\` next to the executable opens in Explorer

### Requirement: Download notice
While the wake engine waits for a download, the Assistant card SHALL show under the wake
model control what will be downloaded, from which site and how many megabytes, with a
Download button. The download SHALL start only when the button is pressed; the notice SHALL
then show the progress, and an error in place with the button again.

#### Scenario: Notice
- **WHEN** wake-word mode is on and the English model, with phrases, is selected while nothing is on disk
- **THEN** the notice lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com), and nothing downloads

#### Scenario: Download pressed
- **WHEN** the user presses Download
- **THEN** the notice shows the progress of each file and disappears when the wake engine is ready

#### Scenario: Download failed
- **WHEN** a download fails or its hash does not match
- **THEN** the notice shows the error and the Download button again
