# app-window Specification

## Purpose
Gives the app the first version's window: the same tabs, cards and status bar, a System tab,
a Config tab, user-selectable appearance, and window data kept with the portable app.

## Requirements

### Requirement: Tabs
The window SHALL have the tabs Console, Codex chat, Status, Performance, System, Settings and
Config, in that order, and SHALL keep each tab's content and scroll position when the user
switches between them.

#### Scenario: Switching tabs
- **WHEN** the user scrolls the Settings tab, opens Status and returns to Settings
- **THEN** the Settings tab shows the same position and the same unsaved input as before

### Requirement: Status bar
A status bar at the bottom of every tab SHALL show the Discord connection state in colour
(connected, connecting, disconnected, error), the number of guilds, the number of voice
channels the bot is in, and the uptime, refreshed at least every 5 s.

#### Scenario: Bot connected and in one channel
- **WHEN** the bot is connected to one guild and in one voice channel
- **THEN** the status bar shows "Connected", "Guilds: 1", "Voice: 1" and a growing uptime

### Requirement: System tab
The System tab SHALL show the application version and build stamp, the process id, the
uptime and the runtime, and the operating system's platform, architecture and CPU core
count, all read from the running process.

#### Scenario: Opening System
- **WHEN** the user opens the System tab
- **THEN** every value is filled from the process and the build stamp equals the first log line

### Requirement: Appearance
The Settings tab SHALL offer the themes Dark, Darcula, Light and IntelliJ and a font size from
11 to 20 px. A change SHALL apply to the whole window at once and SHALL be stored in the
settings file.

#### Scenario: Theme changed
- **WHEN** the user picks Light
- **THEN** the window turns light at once, and after a restart it opens in Light

### Requirement: Config tab
The Config tab SHALL hold the Discord token (masked, write-only, with help on getting one),
the guild id (with help on copying it), the Codex executable, an "Auto-start enabled" note,
Save changes and Reset, and a read-only view of the stored settings with the file location.
The view SHALL never contain the token.

#### Scenario: Stored settings view
- **WHEN** the user opens the Config tab
- **THEN** the stored settings and the path of `settings.json` are shown and no secret appears in them

#### Scenario: Reset
- **WHEN** the user edits fields on the Config tab and presses Reset
- **THEN** the fields return to the stored values and nothing is saved

### Requirement: Window data stays with the app
The window's browser engine SHALL keep its data in a folder next to the executable; the app
SHALL write nothing under the user profile.

#### Scenario: First start on a clean machine
- **WHEN** the app runs for the first time
- **THEN** a `webview-data` folder appears next to the executable and no `com.voicebro.desktop` folder appears in `%LOCALAPPDATA%`

### Requirement: Memory is measured and shown
While memory measurement is on, the status bar SHALL show the total private working set
(the memory Task Manager shows) of the app's own process, its WebView2 processes and its
Codex process, refreshed at least every 5 s, and the System tab SHALL show the same total
and its split between the three; each value SHALL be read from the running processes, never
a fixed label.

#### Scenario: Idle in wake-word mode
- **WHEN** the app runs in wake-word mode with the model loaded, the window open and memory measurement on
- **THEN** the status bar shows a number of megabytes within 10 % of the sum Task Manager shows for those processes

#### Scenario: Codex not running
- **WHEN** memory measurement is on, the bot is in no voice channel and Codex is stopped
- **THEN** the System tab shows the Codex share as not running and the total excludes it

#### Scenario: Turned on
- **WHEN** the user turns memory measurement on
- **THEN** within 5 s the status bar and the card show a reading, without a restart

### Requirement: Memory is not measured while switched off
While memory measurement is off, no process reading SHALL be taken, the status bar SHALL
show no memory item, and the System tab's "Native host" card SHALL say the measurement is
off and that it is turned on in the Diagnostics card of the Settings tab.

#### Scenario: Measurement off
- **WHEN** the app runs with memory measurement off
- **THEN** the status bar has no memory item, the "Native host" card says the measurement is off and where to turn it on, and no reading thread runs

### Requirement: The install dialog opens when the wake word cannot work
The window SHALL open a modal install dialog when it starts in wake-word mode while the
selected model's files or the Vosk library are missing, when the bot joins a voice channel
while they are still missing, and when the user switches the activation to wake word with
them missing. The dialog SHALL name the model's language and each missing file with its
size and site. One dialog at most SHALL be open.

#### Scenario: Start on a clean folder
- **WHEN** the app starts for the first time in an empty folder
- **THEN** the dialog opens as soon as the window shows, says the English wake model is not installed, and lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com)

#### Scenario: The bot joins with files missing
- **WHEN** the user closed the dialog with Not now and the bot then joins a voice channel in wake-word mode
- **THEN** the dialog opens again

#### Scenario: Switched to wake word
- **WHEN** the activation is always listening, nothing is on disk, and the user switches it to wake word
- **THEN** the setting is saved and the dialog opens

#### Scenario: Always listening
- **WHEN** the app starts or the bot joins in always-listening mode with nothing on disk
- **THEN** no dialog opens

#### Scenario: Files on disk
- **WHEN** the app starts in wake-word mode and the selected model and the Vosk library are on disk
- **THEN** no dialog opens

### Requirement: The install dialog downloads only on Download
The dialog's Download button SHALL start the download of the named model and show each
file's progress in the dialog, which SHALL close when the files are installed; a failure
SHALL show the error with Try again and Close. Not now SHALL close it and download nothing,
leaving the Assistant card's notice. Closing the dialog during a download SHALL leave the
download running.

#### Scenario: Download
- **WHEN** the user presses Download in the dialog
- **THEN** the dialog shows the progress of the library and then of the model, and closes when both are installed

#### Scenario: Failure
- **WHEN** a download fails or its hash does not match
- **THEN** the dialog shows the error, Try again starts the download again, and Close closes the dialog with the notice left in the Assistant card

#### Scenario: A stray key when the dialog comes up
- **WHEN** the dialog opens while the user is typing and Enter or Space reaches it
- **THEN** that key closes it as Not now does and nothing is downloaded

#### Scenario: Not now
- **WHEN** the user presses Not now
- **THEN** the dialog closes, nothing is requested from the network, and the Assistant card shows the download notice

#### Scenario: Closed during the download
- **WHEN** the user closes the dialog while a file is downloading
- **THEN** the download goes on and its progress shows in the Assistant card's notice
