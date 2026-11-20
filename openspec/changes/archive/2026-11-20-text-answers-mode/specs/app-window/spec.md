# Spec Delta

## ADDED Requirements

### Requirement: The install dialog in the text answers mode
When a join in the text answers mode was refused for missing files, the install dialog with
its Whisper subject SHALL list only Whisper's missing files (the runtime for the chosen
device, the Whisper model, the VAD model) with their sizes, sites and the total, and its
Download SHALL fetch only those. The voice's files SHALL not be listed or fetched.

#### Scenario: Refused join, voice missing too
- **WHEN** a member runs `/join` in the text answers mode with nothing of Whisper and no voice downloaded
- **THEN** the dialog lists the CPU runtime, the selected model and the VAD model with the total, and no voice file
