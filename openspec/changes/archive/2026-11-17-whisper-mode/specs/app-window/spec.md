# Spec Delta

## MODIFIED Requirements

### Requirement: Tabs
The window SHALL have the tabs Console, Codex chat, Status, Performance, System, Settings,
Config and Whisper, in that order, and SHALL keep each tab's content and scroll position
when the user switches between them.

#### Scenario: Switching tabs
- **WHEN** the user scrolls the Settings tab, opens Status and returns to Settings
- **THEN** the Settings tab shows the same position and the same unsaved input as before

#### Scenario: Whisper tab
- **WHEN** the window opens
- **THEN** the last tab is Whisper

## ADDED Requirements

### Requirement: The install dialog for the Whisper mode
The install dialog SHALL have a second subject, Whisper. It SHALL list every missing file of
the Whisper mode with its size and site and the total:

- the runtime for the chosen device;
- the Whisper model;
- the VAD model;
- the voice engine;
- the voice.

Download, the progress, the errors, Not now and the download by hand SHALL behave as for
the wake word.

#### Scenario: Whisper files listed
- **WHEN** the dialog opens with its Whisper subject and the runtime, the Whisper model, the VAD model, the voice engine and the voice are all missing
- **THEN** it lists each of them with its size and site, and the total

### Requirement: When the Whisper install dialog opens
The install dialog with its Whisper subject SHALL open:

- when a join in Whisper mode was refused for missing files;
- when the assistant mode is switched to Whisper with files missing;
- when a Whisper model, device or voice that needs files is chosen on the Whisper tab.

The window's start SHALL NOT open it.

#### Scenario: Refused join
- **WHEN** a member runs `/join` in Whisper mode on a PC with nothing of Whisper downloaded
- **THEN** the dialog opens listing the CPU runtime, the selected model, the VAD model and the voice with their sizes and the total

#### Scenario: Start
- **WHEN** the app starts in Whisper mode with files missing
- **THEN** no dialog opens, and the Whisper tab's notices list what is missing
