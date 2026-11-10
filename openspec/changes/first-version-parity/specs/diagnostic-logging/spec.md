# Spec Delta

## ADDED Requirements

### Requirement: The window receives the app log
Every log record of this app's own code at Info level or above SHALL reach the window as it
is written, with its time and level, including what the wake-word recognizer heard, each
phrase decision and dropped audio. Third-party crates SHALL reach it at Warn and above.

#### Scenario: Recognizer result
- **WHEN** the recognizer finishes an utterance during a session
- **THEN** the window shows the "heard:" line with the same text as the log file

#### Scenario: Window opened later
- **WHEN** the window is opened after the app has been running
- **THEN** it shows at least the last 500 log lines written before it opened
