# diagnostic-logging Specification

## Purpose
Makes every installed build diagnosable from its own log file, so that a user's report can be
traced to a build and to what the app actually did.

## Requirements

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

### Requirement: The log and the UI identify the build
The app SHALL compute a build stamp (package version, short git hash with a dirty marker, UTC
build date) at build time, write it as the first log line of each start, and show it in the
UI.

#### Scenario: Build stamp in the log
- **WHEN** the app starts
- **THEN** the first line written to the log is the build stamp

#### Scenario: Build stamp in the UI
- **WHEN** the user opens the app window
- **THEN** the build stamp is visible without navigating, and it equals the first log line

### Requirement: The log records the values actually used
When the app replaces a requested value with another (a rejected model, an unavailable voice,
a default taken because a setting was invalid), the log SHALL state the value actually used
and why the requested one was not.

#### Scenario: Requested value replaced
- **WHEN** a requested value is rejected and a fallback is used
- **THEN** one log line names the requested value, the fallback and the reason, and no later line reports the requested value as in use

### Requirement: Connection errors reach the status, not only the log
Errors in starting or keeping an external connection (Discord, assistant process, call) SHALL
appear in the status snapshot the UI shows, in addition to the log.

#### Scenario: Connection error
- **WHEN** an external connection fails to start
- **THEN** the status snapshot carries the error text until the connection succeeds or the user changes the related setting

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
