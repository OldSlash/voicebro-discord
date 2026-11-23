# Spec Delta

## MODIFIED Requirements

### Requirement: Download on first use with pinned hashes
When wake-word mode is on and the recognizer library or selected catalog model is missing, the app SHALL show what is missing and its size, with or without configured phrases. It SHALL download only after operator approval, verify each archive against its pinned SHA-256, and extract it under `assets/` in the active data directory. A hash mismatch SHALL abort with an error and leave no extracted files from that archive.

#### Scenario: First enable
- **WHEN** wake-word mode is on and the selected model is not on disk, whether or not it has phrases
- **THEN** the status says a download is needed and lists the Vosk library and the model with their sizes, and no download starts

#### Scenario: The user agrees
- **WHEN** the user agrees to the download in that state
- **THEN** the status shows download progress and the assets end up under `assets/`; the wake engine then reports "ready" when the model has phrases and "no phrases configured" when it has none

#### Scenario: The user does not agree
- **WHEN** the user leaves the notice without approving the download
- **THEN** nothing is requested from the network, the app keeps working, and no session starts from a wake phrase

#### Scenario: Agreement covers one model
- **WHEN** the user agreed to download the German model and then selects the French model, which is not on disk either
- **THEN** the app asks again for the French model and downloads nothing for it before that approval

#### Scenario: Hash mismatch
- **WHEN** a downloaded archive does not match its pinned hash
- **THEN** the archive is deleted, nothing is extracted from it, and the status shows the error with the file name

### Requirement: Local models from the models folder
At start the app SHALL create `models/` in the active data directory when missing and SHALL list every subfolder holding `am/final.mdl` and `conf/mfcc.conf` as a local Vosk model with the id `local:<folder>`. A local model SHALL need only the recognizer library from the catalog and SHALL never be downloaded, changed or deleted by the app; any other subfolder SHALL be skipped with a log line naming it.

#### Scenario: Model added
- **WHEN** the user extracts `vosk-model-small-ca-0.4`, a language the catalog does not have, into `models/` and starts the app
- **THEN** the wake model list shows `local:vosk-model-small-ca-0.4` with the language `ca`, and selecting it loads that folder once the Vosk library is present

#### Scenario: Not a model
- **WHEN** `models/` holds a folder without `am/final.mdl`
- **THEN** the folder is not listed and the log names it as skipped

#### Scenario: Selected local model removed
- **WHEN** the settings select `local:<folder>` and that folder is gone at start
- **THEN** the wake engine shows an error naming the folder, and the model's phrases stay stored

### Requirement: Each missing file names where it is published
The app SHALL list the file name and official source for each missing asset. Windows SHALL use the existing GitHub release archive and load `libvosk.dll`; Linux x86-64 (not tested) SHALL use the pinned Vosk wheel from PyPI and load `libvosk.so`. Catalog models SHALL continue to come from the Vosk models page on alphacephei.com.

#### Scenario: The pages
- **WHEN** the catalog's Windows library and English model are missing
- **THEN** the library is listed as `vosk-win64-0.3.45.zip` with `https://github.com/alphacep/vosk-api/releases/tag/v0.3.45`, and the model as `vosk-model-small-en-us-0.15.zip` with `https://alphacephei.com/vosk/models`

#### Scenario: Every page offers its archive
- **WHEN** the catalog's pages are fetched
- **THEN** the platform's library page names its archive and the models page names the archive of each of the ten catalog models

#### Scenario: Linux Vosk assets installed (not tested)
- **WHEN** the Linux (not tested) runtime archive and selected model are installed
- **THEN** VoiceBro loads `libvosk.so` from the asset directory and uses the existing recognizer, configured wake phrases and selected model

### Requirement: An archive put into the assets folder is used
When the user approved installation and an archive with the expected file name is under `assets/` in the active data directory, the app SHALL verify it against its pinned SHA-256 and extract it instead of downloading it, and SHALL download only files that are not there. A mismatch SHALL delete the file, extract nothing from it and show the error with its name.

#### Scenario: Both archives put there
- **WHEN** `vosk-win64-0.3.45.zip` and `vosk-model-small-en-us-0.15.zip` are in `assets/` and the user presses Download for the English model
- **THEN** nothing is requested from the network, the status shows each archive being checked and unpacked, the archives are removed after extraction, and the wake engine reports "ready" with phrases or "no phrases configured" without

#### Scenario: One archive put there
- **WHEN** only `vosk-win64-0.3.45.zip` is in `assets/` and the user presses Download for the English model
- **THEN** the library is extracted from that file and only the English model is downloaded

#### Scenario: A wrong file
- **WHEN** `vosk-model-small-en-us-0.15.zip` does not match the pinned hash and the user approves its installation
- **THEN** the file is deleted, nothing is extracted from it, nothing is downloaded in its place, and the status shows the error with its name

#### Scenario: Extracted by hand
- **WHEN** the user extracted both archives into `assets/` instead and presses Download, or starts the app again
- **THEN** the files count as installed and nothing is downloaded

## ADDED Requirements

### Requirement: Headless wake asset provisioning
In the realtime voice mode with the wake word, `voicebro-server --install-assets` (requirement "Headless asset installation" of `headless-runtime`) SHALL explicitly authorize and install the platform's Vosk runtime and any missing catalog assets for the configured model, without connecting to Discord or starting an engine. Normal startup SHALL NOT download wake assets. A headless `/join` refused for missing wake files SHALL direct the operator to the terminal command.

#### Scenario: First headless setup
- **WHEN** headless wake-word mode is configured and the required assets are missing
- **THEN** a normal run identifies what is missing without downloading it, and the install command can prepare those files without a Discord token or an engine sign-in

#### Scenario: Local wake model selected
- **WHEN** the operator runs the install command with a valid local model selected
- **THEN** only the Vosk runtime is eligible for installation, and the local model is not modified
