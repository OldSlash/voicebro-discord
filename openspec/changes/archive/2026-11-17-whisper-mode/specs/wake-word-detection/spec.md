# Spec Delta

## MODIFIED Requirements

### Requirement: Resource behavior
A recognizer SHALL exist only while its speaker is talking and be dropped after 30 s idle.
The engine and model SHALL be loaded only while the session mode is wake word and the
assistant mode is realtime; in Whisper mode the engine SHALL be off, and its state SHALL
say that Whisper spots the phrases. The audio queue to the worker SHALL be bounded, and
drops SHALL be counted and logged.

#### Scenario: Mode switched to always
- **WHEN** the user switches the session mode from wake word to always
- **THEN** the model is unloaded and the engine's memory is released

#### Scenario: Assistant mode switched to Whisper
- **WHEN** the user switches the assistant mode to Whisper while Vosk is loaded
- **THEN** Vosk is unloaded, the wake word state reads off with the reason "the Whisper mode spots the phrases", and no Vosk download is asked for
