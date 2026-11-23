# Spec Delta

## ADDED Requirements

### Requirement: The app opens no console of its own
The desktop app SHALL be a window program in every build: starting it SHALL open no console
window. A debug build started from a terminal SHALL write its log lines to that terminal.
That terminal SHALL only show the log: Ctrl+C and Ctrl+Break there SHALL NOT stop the app,
and closing the terminal SHALL leave the app running. Output redirected to a file or a pipe
SHALL receive the log. A release build SHALL write no log to a terminal.

#### Scenario: Started from Explorer
- **WHEN** the user starts a debug or release build by double-clicking the exe
- **THEN** the window opens and no console window opens

#### Scenario: Started from a terminal
- **WHEN** a debug build is started from a PowerShell prompt or with `cargo run`
- **THEN** the log lines, starting with the build stamp, appear in that terminal

#### Scenario: The terminal is closed
- **WHEN** a debug build was started from a terminal and the user closes that terminal's window
- **THEN** the app keeps running with its window, and its log file goes on

#### Scenario: Ctrl+C in the terminal
- **WHEN** a debug build was started from a terminal and the user presses Ctrl+C there
- **THEN** the app keeps running

#### Scenario: Output redirected
- **WHEN** a debug build is started with its output redirected to a file
- **THEN** the file receives the log lines
