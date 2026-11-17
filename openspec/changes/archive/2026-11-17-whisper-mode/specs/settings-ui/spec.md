# Spec Delta

## ADDED Requirements

### Requirement: Whisper tab fields
The Whisper tab SHALL hold four cards, in this order: Mode, Speech recognition, Phrases and
Voice. Every field SHALL save on change, with a help line and a validation message line
like the Config tab's cards, and the tab SHALL have its own feedback line.

#### Scenario: Card order
- **WHEN** the user opens the Whisper tab
- **THEN** it shows the cards Mode, Speech recognition, Phrases and Voice, in that order

#### Scenario: Saved on change
- **WHEN** the user changes the CPU threads on the Whisper tab
- **THEN** the new value is saved without a further step

### Requirement: Whisper tab Mode card
The Mode card SHALL hold the assistant mode, with a help line saying what runs where and
what leaves the PC in each mode, and a status line with what is loaded, on which device,
and the last answer's timings.

#### Scenario: GPU fallback shown
- **WHEN** the device is NVIDIA GPU and the status says the CPU is used
- **THEN** the Mode card's status line says "GPU requested, CPU used" with the reason

### Requirement: Whisper tab Speech recognition card
The Speech recognition card SHALL hold:

- the Whisper model, grouped as Multilingual, English only, Russian-tuned and Your models,
  each entry with its size, its note and whether it is downloaded;
- the device (CPU, NVIDIA GPU);
- the CPU threads;
- the language;
- the download notice;
- "Open Whisper models folder".

#### Scenario: Choosing a model that is not downloaded
- **WHEN** the user picks a Whisper model that is not on disk
- **THEN** the install dialog opens with its Whisper subject, listing what that model needs and the total, and nothing is saved or fetched until Download

### Requirement: Whisper tab Phrases card
The Phrases card SHALL hold the activation and deactivation phrases, separated by commas or
line breaks as on the Config tab, and a second control for the activation (wake word or
always) that shares `session.mode` with the Config tab.

#### Scenario: Activation shared with the Config tab
- **WHEN** the user sets the activation to always on the Phrases card
- **THEN** the Config tab's activation control shows always too

### Requirement: Whisper tab Voice card
The Voice card SHALL hold:

- the voice, grouped by engine and language, each entry with its size and whether it is
  downloaded or built into Windows;
- the speed;
- Preview;
- the download notice.

#### Scenario: Preview
- **WHEN** the user presses Preview with a downloaded voice
- **THEN** a short neutral sample sentence in the voice's language plays in the window, and nothing is sent to Discord

### Requirement: Vosk fields in Whisper mode
While the assistant mode is Whisper, the Config tab's Assistant card SHALL disable the
Vosk fields (wake phrases, wake engine and wake model) and show the note "Whisper mode:
the phrases are on the Whisper tab". The Codex card's Voice and Answer handoff fields SHALL
say in their help lines that they apply to the realtime mode only.

#### Scenario: Switching modes
- **WHEN** the user switches the mode to Whisper and opens the Config tab
- **THEN** the Vosk fields are disabled with the note, and switching back enables them with their stored values

## MODIFIED Requirements

### Requirement: Collapsible cards on the Settings and Config tabs
Every card of the Settings tab (Appearance, Diagnostics), of the Config tab ("Discord and
Codex", Access, Audio, Assistant, Codex, Hotkey) and of the Whisper tab (Mode, Speech
recognition, Phrases, Voice) SHALL collapse to its title row and expand again when the
user activates the title, by mouse or keyboard. The title SHALL show whether the card is
collapsed. The state SHALL be saved and restored at the next start; every card SHALL be
expanded by default.

#### Scenario: Collapsing a card
- **WHEN** the user clicks the title of the Audio card
- **THEN** the card shows only its title row, and the cards below move up

#### Scenario: Collapsing a Settings card
- **WHEN** the user clicks the title of the Appearance card on the Settings tab
- **THEN** the card shows only its title row, the Diagnostics card moves up, and a restart shows Appearance collapsed

#### Scenario: Keyboard
- **WHEN** the title of the Hotkey card has the focus and the user presses Enter or Space
- **THEN** the card collapses, and the same key expands it again

#### Scenario: After a restart
- **WHEN** the user collapsed the Assistant and Hotkey cards and restarts the app
- **THEN** the Config tab opens with those two cards collapsed and the others expanded

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `ui.collapsed_cards`
- **THEN** every card of the Settings, Config and Whisper tabs is expanded

#### Scenario: A collapsed card keeps its fields
- **WHEN** the guild id has an unsaved edit and the user collapses and expands "Discord and Codex"
- **THEN** the edit is still in the field and marked as changed

#### Scenario: A collapsed Access card keeps its code
- **WHEN** a link code is waiting and the user collapses and expands the Access card
- **THEN** the same code and its countdown are shown again

#### Scenario: A Whisper card
- **WHEN** the user collapses the Voice card on the Whisper tab and restarts the app
- **THEN** the Whisper tab opens with the Voice card collapsed
