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
