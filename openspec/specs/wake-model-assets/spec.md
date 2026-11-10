# wake-model-assets Specification

## Purpose
Gets the recognizer library and model onto the machine safely on first use, and keeps the
app working when they are absent.

## Requirements

### Requirement: Download on first use with pinned hashes
When wake-word mode is enabled and the assets for the selected model are missing, the app
SHALL download the recognizer library and the model from the catalog's URLs into `assets\`
next to the executable, verify each file against its pinned SHA-256, and extract it. A hash
mismatch SHALL abort with an error in the status and leave no extracted files.

#### Scenario: First enable
- **WHEN** the user turns on wake-word mode for the first time
- **THEN** the status shows download progress, then "ready", and the assets are under `assets\`

#### Scenario: Hash mismatch
- **WHEN** a downloaded archive does not match its pinned hash
- **THEN** the archive is deleted, nothing is extracted, and the status shows the error with the file name

### Requirement: Interrupted downloads leave no trace
Downloads SHALL go to a temporary name and be renamed when complete; partial files SHALL be
removed at start-up; a completed extraction SHALL be marked by a marker file.

#### Scenario: Killed during download
- **WHEN** the app is closed while a download is in progress and started again
- **THEN** the partial file is removed and the download starts over

### Requirement: The app runs without the assets
The recognizer library SHALL be loaded at runtime; the app SHALL start and work in always
mode when the library or model is absent.

#### Scenario: Assets absent, always mode
- **WHEN** the app starts with no assets and the session mode is always
- **THEN** nothing is downloaded and the app runs normally

### Requirement: Progress is visible
Download progress SHALL be reported as events and in the status in steps of at most 20 %.

#### Scenario: Progress
- **WHEN** a 46 MB model is downloading
- **THEN** the status shows the percentage changing at least every 20 %
