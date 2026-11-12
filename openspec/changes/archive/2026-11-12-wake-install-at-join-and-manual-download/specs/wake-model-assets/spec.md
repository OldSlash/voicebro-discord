# Spec Delta

## ADDED Requirements

### Requirement: Each missing file names where it is published
Each file the app asks to download SHALL be listed with its archive's file name and the
official page it is published on: the Vosk library with its release page on GitHub, a
catalog model with the Vosk models page on alphacephei.com.

#### Scenario: The pages
- **WHEN** the Vosk library and the English model are missing
- **THEN** the library is listed as `vosk-win64-0.3.45.zip` with `https://github.com/alphacep/vosk-api/releases/tag/v0.3.45`, and the model as `vosk-model-small-en-us-0.15.zip` with `https://alphacephei.com/vosk/models`

#### Scenario: Every page offers its archive
- **WHEN** the catalog's pages are fetched
- **THEN** the release's asset list names `vosk-win64-0.3.45.zip`, and the models page names the archive of each of the ten catalog models

### Requirement: An archive put into the assets folder is used
When the user agreed to a download and an archive with the expected file name is in
`assets\` next to the executable, the app SHALL verify it against its pinned SHA-256 and
extract it instead of downloading it, and SHALL download only the files that are not
there. A mismatch SHALL delete the file, extract nothing and show the error with its name.

#### Scenario: Both archives put there
- **WHEN** `vosk-win64-0.3.45.zip` and `vosk-model-small-en-us-0.15.zip`, downloaded from their pages, are in `assets\` and the user presses Download for the English model
- **THEN** nothing is requested from the network, the status shows each archive being checked and unpacked, both are extracted, the archives are removed, and the wake engine reports "ready" with phrases or "no phrases configured" without

#### Scenario: One archive put there
- **WHEN** only `vosk-win64-0.3.45.zip` is in `assets\` and the user presses Download for the English model
- **THEN** the library is extracted from that file and only the English model is downloaded

#### Scenario: A wrong file
- **WHEN** a file named `vosk-model-small-en-us-0.15.zip` in `assets\` does not match the pinned hash and the user presses Download
- **THEN** the file is deleted, nothing is extracted, nothing is downloaded in its place, and the status shows the error with the file name

#### Scenario: Extracted by hand
- **WHEN** the user extracted both archives into `assets\` instead and presses Download, or starts the app again
- **THEN** the files count as installed and nothing is downloaded
