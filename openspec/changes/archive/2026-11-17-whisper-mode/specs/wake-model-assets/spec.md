# Spec Delta

## ADDED Requirements

### Requirement: One asset store for every downloaded file
The wake word, Whisper and the voices SHALL share one asset store with the same rules:

- a pinned SHA-256 per file;
- a `.part` file during a download, removed at start;
- a file the user put into the assets folder by hand is used instead of a download;
- a mismatch deletes the file and extracts nothing.

#### Scenario: A Whisper model put in by hand
- **WHEN** the user downloads `ggml-small-q8_0.bin` from Hugging Face into the assets folder and presses Download
- **THEN** the file is checked against its pinned SHA-256 and moved into its model folder without a download

### Requirement: Single files and trimmed archives in the asset store
The asset store SHALL take single files, which are moved into their folder after the hash
check and not unpacked. For an archive it SHALL take a list of files to keep, and SHALL
delete everything else that was extracted.

#### Scenario: Runtime zip trimmed
- **WHEN** the Whisper CPU runtime zip has been checked and extracted
- **THEN** only the libraries the app loads remain in its folder

### Requirement: Whisper and voice files need the user's OK
The Whisper runtime, the Whisper and VAD models, the voice engine and the voices SHALL be
downloaded only after the user presses Download for exactly that set of files. Until then
the status SHALL list each missing file with its label, size, site, file name and official
page. The OK SHALL be kept in memory only, and another model, device or voice SHALL ask
again for what it adds.

#### Scenario: GPU chosen
- **WHEN** the user picks the NVIDIA GPU on a PC that has only the CPU runtime
- **THEN** the notice lists the CUDA runtime (685 MB, github.com), and nothing is fetched until Download is pressed
