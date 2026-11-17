## MODIFIED Requirements

### Requirement: Whisper and voice files need the user's OK
The Whisper runtime, the Whisper and VAD models, the voice engine and the voices SHALL be
downloaded only after the user presses Download for exactly that set of files, in either
assistant mode. Until then the status SHALL list each missing file of the selected model,
device and voice with its label, size, site, file name and official page, in the realtime
mode too. The OK SHALL be kept in memory only, and another model, device or voice SHALL ask
again for what it adds. A download in the realtime mode SHALL load nothing.

#### Scenario: GPU chosen
- **WHEN** the user picks the NVIDIA GPU on a PC that has only the CPU runtime
- **THEN** the notice lists the CUDA runtime (685 MB, github.com), and nothing is fetched until Download is pressed

#### Scenario: Download ahead of the switch
- **WHEN** the assistant mode is realtime voice and the user presses Download in the Settings tab's Whisper card
- **THEN** the files of the Whisper model chosen for the Whisper mode are downloaded and checked, nothing is loaded, and a later switch to Whisper needs no download
