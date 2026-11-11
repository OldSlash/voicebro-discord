## MODIFIED Requirements

### Requirement: Download on first use with pinned hashes
When wake-word mode is on and the recognizer library or the selected catalog model is
missing, the app SHALL show what is missing and its size, with or without phrases for that
model, and SHALL download nothing until the user agrees for that model. After the OK it
SHALL download the files from the catalog's URLs into `assets\` next to the executable,
verify each against its pinned SHA-256, and extract it. A hash mismatch SHALL abort with an
error in the status and leave no extracted files.

#### Scenario: First enable
- **WHEN** wake-word mode is on and the selected model is not on disk, whether or not it has phrases
- **THEN** the status says a download is needed and lists the Vosk library and the model with their sizes, and no download starts

#### Scenario: The user agrees
- **WHEN** the user agrees to the download in that state
- **THEN** the status shows download progress and the assets end up under `assets\`; the wake engine then reports "ready" when the model has phrases and "no phrases configured" when it has none

#### Scenario: The user does not agree
- **WHEN** the user leaves the notice without pressing Download
- **THEN** nothing is requested from the network, the app keeps working, and no session starts from a wake phrase

#### Scenario: Agreement covers one model
- **WHEN** the user agreed to download the German model and then selects the French model, which is not on disk either
- **THEN** the app asks again for the French model and downloads nothing for it before that OK

#### Scenario: Hash mismatch
- **WHEN** a downloaded archive does not match its pinned hash
- **THEN** the archive is deleted, nothing is extracted, and the status shows the error with the file name

### Requirement: Local models from the models folder
At start the app SHALL create `models\` next to the executable when it is missing and SHALL
list every subfolder holding `am\final.mdl` and `conf\mfcc.conf` as a local Vosk model with
the id `local:<folder>`. A local model SHALL need only the recognizer library from the
catalog and SHALL never be downloaded, changed or deleted by the app; any other subfolder
SHALL be skipped with a log line naming it.

#### Scenario: Model added
- **WHEN** the user extracts `vosk-model-small-ca-0.4`, a language the catalog does not have, into `models\` and starts the app
- **THEN** the wake model list shows `local:vosk-model-small-ca-0.4` with the language `ca`, and selecting it loads that folder once the Vosk library is present

#### Scenario: Not a model
- **WHEN** `models\` holds a folder without `am\final.mdl`
- **THEN** the folder is not listed and the log names it as skipped

#### Scenario: Selected local model removed
- **WHEN** the settings select `local:<folder>` and that folder is gone at start
- **THEN** the wake engine shows an error naming the folder, and the model's phrases stay stored

## ADDED Requirements

### Requirement: Catalog models
The model catalog SHALL hold the small Vosk models for English (US), Russian, Czech, Dutch,
French, German, Italian, Polish, Portuguese (Brazilian) and Spanish, each with its download
URL, pinned SHA-256, size and license, listed English first, Russian second and the rest by
language name. A new settings file SHALL select the English model.

#### Scenario: Catalog
- **WHEN** the window asks for the wake models
- **THEN** it gets, in this order, `vosk-model-small-en-us-0.15`, `vosk-model-small-ru-0.22`, `vosk-model-small-cs-0.4-rhasspy`, `vosk-model-small-nl-0.22`, `vosk-model-small-fr-0.22`, `vosk-model-small-de-0.15`, `vosk-model-small-it-0.22`, `vosk-model-small-pl-0.22`, `vosk-model-small-pt-0.3` and `vosk-model-small-es-0.42`, each with its language, size and license

#### Scenario: First start
- **WHEN** the app starts without a settings file
- **THEN** the wake model is `vosk-model-small-en-us-0.15`

#### Scenario: Every catalog model loads
- **WHEN** a catalog model is downloaded through the app
- **THEN** its archive matches the pinned hash, it extracts to the folder its entry names, and the Vosk library loads it

## REMOVED Requirements

### Requirement: Built-in models
**Reason**: "Built-in" read as installed with the app, and nothing is; the catalog also
grows from two models to ten.
**Migration**: Replaced by "Catalog models", which keeps the English default and the
English and Russian entries and adds eight languages.
