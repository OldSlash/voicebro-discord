# Spec Delta

## MODIFIED Requirements

### Requirement: Memory is measured and shown
While memory measurement is on, the status bar SHALL show the total private working set
(the memory Task Manager shows) of the app's own process, its WebView2 processes and its
Codex process, refreshed at least every 5 s, and the System tab SHALL show the same total
and its split between the three; each value SHALL be read from the running processes, never
a fixed label. While it is off, no process reading SHALL be taken, the status bar SHALL show
no memory item, and the System tab's "Native host" card SHALL say the measurement is off and
that it is turned on in the Diagnostics card of the Settings tab.

#### Scenario: Idle in wake-word mode
- **WHEN** the app runs in wake-word mode with the model loaded, the window open and memory measurement on
- **THEN** the status bar shows a number of megabytes within 10 % of the sum Task Manager shows for those processes

#### Scenario: Codex not running
- **WHEN** memory measurement is on, the bot is in no voice channel and Codex is stopped
- **THEN** the System tab shows the Codex share as not running and the total excludes it

#### Scenario: Measurement off
- **WHEN** the app runs with memory measurement off
- **THEN** the status bar has no memory item, the "Native host" card says the measurement is off and where to turn it on, and no reading thread runs

#### Scenario: Turned on
- **WHEN** the user turns memory measurement on
- **THEN** within 5 s the status bar and the card show a reading, without a restart
