# diagnostic-logging Specification

## Purpose
Makes every installed build diagnosable from its own log file, so that a user's report can be
traced to a build and to what the app actually did.

## Requirements

### Requirement: Every build writes a file log
The app SHALL write a diagnostic log to `logs\` next to its executable in every build
configuration, release included, at Info level for the app and Warn level for third-party
crates, rotating at 4 MB with two old files kept, timestamps in local time.

#### Scenario: Release build
- **WHEN** a release build starts
- **THEN** a log file exists in the logs directory and contains the start-up lines

#### Scenario: Rotation
- **WHEN** the current log file exceeds 4 MB
- **THEN** it is rotated and at most two old files are kept

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
