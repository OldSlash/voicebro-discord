# Spec Delta

## MODIFIED Requirements

### Requirement: Every build writes a file log
The app SHALL write a diagnostic log to `logs\` in its data directory (next to its executable unless `VOICEBRO_DATA_DIR` names another) in every build configuration, release included, with timestamps in local time, rotating at 1 MB with one old file kept. The file SHALL hold this app's event lines at Info level and above and third-party errors; the detail lines SHALL be added only in a debug build or while the verbose log is on. The headless app SHALL write the same file with the same filters, shared with the desktop app, and the same lines to stderr.

#### Scenario: Release build
- **WHEN** a release build starts
- **THEN** a log file exists in the logs directory and contains the start-up lines

#### Scenario: Rotation
- **WHEN** the current log file exceeds 1 MB
- **THEN** it is rotated and at most one old file is kept

#### Scenario: Third-party warning
- **WHEN** a third-party crate logs a warning in a release build with the verbose log off
- **THEN** the window shows it and the log file does not contain it

#### Scenario: Headless log
- **WHEN** the headless app runs with the verbose log off and someone speaks in a session
- **THEN** the log file and stderr hold the same event lines, and neither holds the detail lines with what people said

### Requirement: The log and the UI identify the build
The app SHALL compute a build stamp (package version, short git hash with a dirty marker, UTC build date) at build time, write it as the first log line of each start, and show it in the UI. The headless app SHALL compute the same stamp and write it as the first line of its log and of its terminal output.

#### Scenario: Build stamp in the log
- **WHEN** the app starts
- **THEN** the first line written to the log is the build stamp

#### Scenario: Build stamp in the UI
- **WHEN** the user opens the app window
- **THEN** the build stamp is visible without navigating, and it equals the first log line

#### Scenario: Build stamp in the terminal
- **WHEN** the headless app starts
- **THEN** the first line in the terminal is the build stamp, and it equals the first log line
