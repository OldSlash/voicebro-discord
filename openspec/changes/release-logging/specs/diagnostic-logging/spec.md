# Spec Delta

## MODIFIED Requirements

### Requirement: Every build writes a file log
The app SHALL write a diagnostic log to `logs\` next to its executable in every build
configuration, release included, with timestamps in local time, rotating at 1 MB with one
old file kept. The file SHALL hold this app's event lines at Info level and above and
third-party errors; the detail lines SHALL be added only in a debug build or while the
verbose log is on.

#### Scenario: Release build
- **WHEN** a release build starts
- **THEN** a log file exists in the logs directory and contains the start-up lines

#### Scenario: Rotation
- **WHEN** the current log file exceeds 1 MB
- **THEN** it is rotated and at most one old file is kept

#### Scenario: Third-party warning
- **WHEN** a third-party crate logs a warning in a release build with the verbose log off
- **THEN** the window shows it and the log file does not contain it

### Requirement: The window receives the app log
Every log record of this app's own code at Info level or above SHALL reach the window as it
is written, with its time and level, including what the wake-word recognizer heard, each
phrase decision and dropped audio, whether or not the record is written to the file.
Third-party crates SHALL reach it at Warn and above.

#### Scenario: Recognizer result
- **WHEN** the recognizer finishes an utterance during a session
- **THEN** the window shows the "heard:" line, and the log file holds the same line only in a debug build or while the verbose log is on

#### Scenario: Window opened later
- **WHEN** the window is opened after the app has been running
- **THEN** it shows at least the last 500 log lines written before it opened

#### Scenario: WebRTC sub-crates
- **WHEN** a WebRTC sub-crate such as the ICE or DTLS layer logs at Info
- **THEN** the line reaches neither the window nor the file

## ADDED Requirements

### Requirement: Detail lines
Recognized speech, per-turn latency, the call's buffer and counter lines, delivery reports,
the speaker-to-stream mapping, copies of chat entries, third-party warnings and the voice
receiver's per-packet errors SHALL be detail lines. In a release build they SHALL reach the
window and SHALL be written to the file only while the verbose log is on.

#### Scenario: A call in a release build
- **WHEN** a release build with the verbose log off runs a session with a wake phrase, a question and an answer
- **THEN** the file holds the session start and end and the call start and end, and no recognized text, latency line, counter line or chat text, while the window shows all of them

#### Scenario: Debug build
- **WHEN** a debug build runs the same session
- **THEN** the file holds the detail lines as well

### Requirement: Verbose log
Turning `diagnostics.verbose_log` on SHALL add the detail lines to the file from the next
line on, and turning it off SHALL stop them, both without a restart.

#### Scenario: Turned on during a session
- **WHEN** the user turns the verbose log on while a session runs
- **THEN** the next recognized utterance appears in the file as a "heard:" line

#### Scenario: Turned off
- **WHEN** the user turns the verbose log off
- **THEN** no further detail line is written to the file
