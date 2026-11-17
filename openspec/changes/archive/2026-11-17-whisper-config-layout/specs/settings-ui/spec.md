## MODIFIED Requirements

### Requirement: Config tab fields
The Config tab's "Discord and Codex" card SHALL hold the Discord token (write-only, with set
and clear), the guild id and the Codex executable. The Access, Audio, Assistant, Local
voice, Codex and Hotkey cards follow it on the same tab, in that order.

#### Scenario: Config fields
- **WHEN** the Config tab is opened
- **THEN** the token field shows whether a token is stored but never the token, and the guild id and the Codex executable show their stored values

#### Scenario: Card order
- **WHEN** the user opens the Config tab
- **THEN** the cards after "Discord and Codex" are Access, Audio, Assistant, Local voice, Codex and Hotkey

## ADDED Requirements

### Requirement: Assistant mode in the Assistant card
The Config tab's Assistant card SHALL hold the assistant mode as its first field: "Realtime
voice" or "Whisper", with a help line saying what runs on the PC and what leaves it in each
mode. Below it a grey Recognition field SHALL show "Vosk" in the realtime mode and
"Whisper" in the Whisper mode; it follows the mode and cannot be edited.

#### Scenario: Switching to Whisper
- **WHEN** the user picks Whisper as the assistant mode on the Config tab
- **THEN** the mode is saved at once, and the Recognition field shows "Whisper"

### Requirement: Whisper model in the Assistant card
In the Whisper mode the Assistant card's model field SHALL list the Whisper models instead
of the Vosk models: grouped as Multilingual, English only, Russian-tuned and Your models,
each entry with its size, its note and whether it is downloaded. Its download notice and
Download button SHALL work as the Vosk model's do. In the realtime mode the field SHALL be
the Vosk wake model, as before.

#### Scenario: Whisper model not downloaded
- **WHEN** the mode is Whisper and the selected Whisper model is not on disk
- **THEN** the Assistant card shows the notice with the runtime, the model and the VAD model, their sizes and sites, and a Download button

### Requirement: Phrase fields follow the mode
The Assistant card's activation and deactivation phrase fields SHALL hold the selected Vosk
model's phrases in the realtime mode and the Whisper phrases (`whisper.phrases`) in the
Whisper mode. A save SHALL write the list of the mode shown. The help line SHALL say which
set the fields hold. The activation control (wake word or always) SHALL stay below them.

#### Scenario: Phrases per mode
- **WHEN** the user sets the activation phrase "hey bro" in the Whisper mode and switches back to the realtime mode
- **THEN** the fields show the Vosk model's phrases again, and "hey bro" is kept in `whisper.phrases`

### Requirement: Local voice card
The Config tab SHALL have a Local voice card after the Assistant card. It SHALL hold:

- the voice, grouped by engine and language, each entry with its size and whether it is
  downloaded or built into Windows, and the voice's data license;
- the speed;
- Preview;
- the voice's download notice with a Download button.

#### Scenario: Preview
- **WHEN** the user presses Preview with a downloaded voice
- **THEN** a short neutral sample sentence in the voice's language plays in the window, and nothing is sent to Discord

### Requirement: The Local voice card says when the voice is used
The Local voice card SHALL show a note that follows the assistant mode. In the realtime
voice mode it SHALL say that the local voice is not used, because the Codex call speaks
with its own voice. In the Whisper mode it SHALL say that this voice reads Codex's answers
into the channel.

#### Scenario: Realtime mode
- **WHEN** the assistant mode is realtime voice
- **THEN** the Local voice card says the local voice is not used in this mode, and its fields and Download still work

### Requirement: Whisper card on the Settings tab
The Settings tab SHALL have a Whisper card after Appearance and Diagnostics. It SHALL hold:

- a label "On" or "Off" that follows the assistant mode, saying where to switch it;
- the device (CPU, NVIDIA GPU), the CPU threads and the language;
- the line with what is loaded, on which device, and the last answer's timings;
- "Open Whisper models folder".

#### Scenario: Off
- **WHEN** the assistant mode is realtime voice and the user opens the Settings tab
- **THEN** the Whisper card's label says it is off and to switch it on in Config → Assistant, and its fields can still be changed

#### Scenario: GPU fallback shown
- **WHEN** the device is NVIDIA GPU and the status says the CPU is used
- **THEN** the Whisper card's line says "GPU requested, CPU used" with the reason

### Requirement: Whisper's files can be downloaded before the switch
While the assistant mode is realtime voice and the Whisper model chosen for the Whisper mode
(`whisper.model`) misses files, the Settings tab's Whisper card SHALL show a notice naming
that model and listing every missing file for the chosen device, with a Download button.
In the Whisper mode that notice SHALL be under the Whisper model on the Config tab instead.

#### Scenario: Realtime mode with nothing of Whisper downloaded
- **WHEN** the assistant mode is realtime voice and nothing of Whisper is on disk
- **THEN** the Settings tab's Whisper card names the model "base (q8_0)" and lists the runtime, the model and the VAD model with their sizes and a Download button

### Requirement: The CUDA runtime's notice sits under the device
When NVIDIA GPU is chosen on the Settings tab's Whisper card and the CUDA runtime is not on
disk, the card SHALL show the runtime's notice under the device field: its size, site and a
Download button.

#### Scenario: GPU chosen without the CUDA runtime
- **WHEN** the user chooses NVIDIA GPU on a PC with only the CPU runtime
- **THEN** the card shows the CUDA runtime, 685 MB from github.com, with Download, and nothing is fetched until Download is pressed

### Requirement: Realtime-only Codex fields say so
The Codex card's Voice and Answer handoff fields SHALL say in their help lines that they
apply to the realtime voice mode only.

#### Scenario: Voice help
- **WHEN** the user reads the help line of the Codex card's Voice field
- **THEN** it says the field applies to the realtime voice mode only

## REMOVED Requirements

### Requirement: Whisper tab fields
**Reason**: The Whisper tab is removed (the user's decision of 2026-11-17): it copied the
Config tab's cards instead of extending them.
**Migration**: The fields are on the Config tab's Assistant and Local voice cards and on the
Settings tab's Whisper card.

### Requirement: Whisper tab Mode card
**Reason**: The Whisper tab is removed.
**Migration**: The mode is the first field of Config → Assistant; the status line is on the
Settings tab's Whisper card.

### Requirement: Whisper tab Speech recognition card
**Reason**: The Whisper tab is removed.
**Migration**: The model is in Config → Assistant; the device, threads, language and the
models folder are on the Settings tab's Whisper card.

### Requirement: Whisper tab Phrases card
**Reason**: The Whisper tab is removed; the phrases are part of the Assistant card.
**Migration**: The Assistant card's phrase fields hold the Whisper phrases in the Whisper
mode.

### Requirement: Whisper tab Voice card
**Reason**: The Whisper tab is removed.
**Migration**: The voice, speed, Preview and the download notice are on the Config tab's
Local voice card.

### Requirement: Vosk fields in Whisper mode
**Reason**: The Assistant card's fields now follow the mode instead of being disabled with a
note pointing to another tab.
**Migration**: See "Whisper model in the Assistant card" and "Phrase fields follow the mode";
the Codex card's Voice and Answer handoff help lines keep "realtime mode only".
