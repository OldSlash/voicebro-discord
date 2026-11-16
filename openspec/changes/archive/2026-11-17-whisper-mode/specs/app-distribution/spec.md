# Spec Delta

## MODIFIED Requirements

### Requirement: Uninstall removes the data only when asked
The uninstaller SHALL remove the program files and shortcuts. With "Delete the application
data" checked it SHALL also remove `settings.json`, `owner-link.bin`, `logs\`, `assets\`,
`models\`, `whisper-models\`, `codex-workspace\`, `webview-data\`, the stored Discord token
and the then empty folder; unchecked, it SHALL leave them.

#### Scenario: Data kept
- **WHEN** the user uninstalls without checking the box
- **THEN** the folder still holds the settings, the owner link, logs and models, and a reinstall into it starts with them

#### Scenario: Data deleted
- **WHEN** the user uninstalls with the box checked
- **THEN** the folder is gone and the Credential Manager has no `VoiceBro/discord_token`

### Requirement: Release artifacts carry no wake files
The installer and the portable zip SHALL contain no Vosk library, no wake model, no Whisper
runtime or model, and no voice engine or voice. The app SHALL download them only after the
user's OK.

#### Scenario: Release contents
- **WHEN** the installer of a release is unpacked and the zip is listed
- **THEN** neither contains `libvosk.dll`, a `vosk-model-*` folder, `whisper.dll`, a `ggml*.dll`, a `.bin` model, a voice engine library or an `assets\` folder

## ADDED Requirements

### Requirement: Notices for the Whisper mode's downloads
`THIRD-PARTY-NOTICES.txt` SHALL name, with their licenses, everything the Whisper mode
downloads after the user's OK:

- whisper.cpp with its MIT text, and NVIDIA's CUDA runtime and cuBLAS libraries of the GPU
  runtime with their terms;
- each Whisper catalog model and the Silero VAD model;
- the voice engine and its runtime;
- each catalog voice with its license and the attribution it requires.

#### Scenario: Notices of a release
- **WHEN** the notices file of a release is read
- **THEN** it names whisper.cpp with its MIT text, the CUDA runtime and cuBLAS with their terms, each Whisper catalog model, the Silero VAD model, the voice engine and its runtime, and each catalog voice with its license and attribution

### Requirement: The notices script checks the Whisper and voice catalogs
The notices script SHALL stop with an error when the Whisper or voice catalog has an entry
that the script does not list, or the script lists one that the catalog does not have.

#### Scenario: Catalog and notices differ
- **WHEN** a Whisper model is added to the catalog without a notices entry
- **THEN** the notices script stops with an error naming the model and writes no notices file
