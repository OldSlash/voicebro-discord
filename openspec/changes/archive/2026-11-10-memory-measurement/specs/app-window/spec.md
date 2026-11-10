# Spec Delta

## ADDED Requirements

### Requirement: Memory is measured and shown
The status bar SHALL show the total private working set (the memory Task Manager shows) of
the app's own process, its WebView2 processes and its Codex process, refreshed at least
every 5 s. The System tab SHALL show the same total and its split between the three. Each
value SHALL be read from the running processes, never a fixed label.

#### Scenario: Idle in wake-word mode
- **WHEN** the app runs in wake-word mode with the model loaded and the window open
- **THEN** the status bar shows a number of megabytes within 10 % of the sum Task Manager shows for those processes

#### Scenario: Codex not running
- **WHEN** the bot is in no voice channel and Codex is stopped
- **THEN** the System tab shows the Codex share as not running and the total excludes it
