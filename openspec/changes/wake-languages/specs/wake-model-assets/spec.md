# Spec Delta

## MODIFIED Requirements

### Requirement: Download on first use with pinned hashes
When wake-word mode is on, the selected model has phrases and the recognizer library or the
catalog model is missing, the app SHALL show what is missing and its size and SHALL download
nothing until the user agrees. After the OK it SHALL download the files from the catalog's
URLs into `assets\` next to the executable, verify each against its pinned SHA-256, and
extract it. A hash mismatch SHALL abort with an error in the status and leave no extracted
files.

#### Scenario: First enable
- **WHEN** the user turns on wake-word mode with phrases for a model that is not on disk
- **THEN** the status says a download is needed and lists the Vosk library and the model with their sizes, and no download starts

#### Scenario: The user agrees
- **WHEN** the user presses Download in that state
- **THEN** the status shows download progress, then "ready", and the assets are under `assets\`

#### Scenario: The user does not agree
- **WHEN** the user leaves the notice without pressing Download
- **THEN** nothing is requested from the network, the app keeps working, and no session starts from a wake phrase

#### Scenario: Hash mismatch
- **WHEN** a downloaded archive does not match its pinned hash
- **THEN** the archive is deleted, nothing is extracted, and the status shows the error with the file name

## ADDED Requirements

### Requirement: Built-in models
The model catalog SHALL hold `vosk-model-small-en-us-0.15` (English) and
`vosk-model-small-ru-0.22` (Russian), each with its download URL, pinned SHA-256, size and
license. A new settings file SHALL select the English model.

#### Scenario: Catalog
- **WHEN** the window asks for the wake models
- **THEN** both catalog models are listed with their language, size and license

#### Scenario: First start
- **WHEN** the app starts without a settings file
- **THEN** the wake model is `vosk-model-small-en-us-0.15`

### Requirement: Local models from the models folder
At start the app SHALL create `models\` next to the executable when it is missing and SHALL
list every subfolder holding `am\final.mdl` and `conf\mfcc.conf` as a local Vosk model with
the id `local:<folder>`. A local model SHALL need only the recognizer library from the
catalog and SHALL never be downloaded, changed or deleted by the app; any other subfolder
SHALL be skipped with a log line naming it.

#### Scenario: Model added
- **WHEN** the user extracts `vosk-model-small-de-0.15` into `models\` and starts the app
- **THEN** the wake model list shows `local:vosk-model-small-de-0.15` with the language `de`, and selecting it loads that folder once the Vosk library is present

#### Scenario: Not a model
- **WHEN** `models\` holds a folder without `am\final.mdl`
- **THEN** the folder is not listed and the log names it as skipped

#### Scenario: Selected local model removed
- **WHEN** the settings select `local:<folder>` and that folder is gone at start
- **THEN** the wake engine shows an error naming the folder, and the model's phrases stay stored
