# Spec Delta

## MODIFIED Requirements

### Requirement: App-server lifecycle
The app SHALL start `codex app-server --disable shell_tool` (using the configured or auto-detected executable) on `/join`, on Sign in, Check, Refresh lists or Link from the window, and at app start only to read the lists as the requirement "Lists are read at start" says. It SHALL stop the app-server when the bot leaves its last voice channel, one minute after the last Sign in, Check, Refresh lists, Link or read at start while the bot is in no channel, and when the app exits. A missing executable or a failed handshake SHALL be shown in the status with the error text.

#### Scenario: Codex not installed
- **WHEN** no Codex executable is found and none is configured
- **THEN** the status shows the assistant as unavailable with "codex not found", and Discord keeps working

#### Scenario: App exit
- **WHEN** the main window is closed
- **THEN** the app-server process is gone within two seconds

#### Scenario: App start
- **WHEN** the app starts with saved lists and the refresh at start off, and nobody has used `/join`, Sign in, Check, Refresh lists or Link
- **THEN** no Codex process belongs to the app

#### Scenario: Read at start ends
- **WHEN** the app read the lists at start and the bot joins no voice channel
- **THEN** the app-server process is gone about one minute after the read

#### Scenario: Link reads the account
- **WHEN** the user presses Link in the Access card while the bot is in no voice channel
- **THEN** the app-server starts, the account is read, and the process is gone about one minute later whether or not the code is used

#### Scenario: Last channel left
- **WHEN** the bot leaves its last voice channel
- **THEN** the app-server process is gone within two seconds and the conversation of each guild is kept for the next join

### Requirement: Sign-in through Codex
The UI SHALL offer "Sign in" (opens Codex's own ChatGPT sign-in in the browser) and "Check" (reads the account); the status SHALL show the account type when signed in and "not signed in" otherwise. The headless app SHALL use the Codex CLI sign-in of the user running it, including the CLI's default credential location or `CODEX_HOME` when set. VoiceBro SHALL store no OpenAI credential.

#### Scenario: Not signed in
- **WHEN** the account read says no account
- **THEN** the status shows not signed in, a session start fails with that reason, and no call is attempted

#### Scenario: Headless restart after CLI sign-in
- **WHEN** the operator signs in with Codex CLI and restarts headless under the same user and Codex credential location
- **THEN** the app-server can reuse that sign-in without a VoiceBro credential file

### Requirement: Join prepares the assistant
On `/join` the app SHALL start the app-server if needed, read the account and open the guild's thread before the bot enters the voice channel, so the first activation only places the call or, in Whisper mode, sends the first question. When Codex is missing, fails to start or is not signed in, the bot SHALL not join and SHALL reply with the reason and what to do.

#### Scenario: Signed in
- **WHEN** a member runs `/join` and Codex is signed in
- **THEN** the thread is open before the bot is in the channel, and no voice call exists until an activation

#### Scenario: Signed out
- **WHEN** a member runs `/join` on the desktop app and the account read says no account
- **THEN** the bot stays out of the channel and the reply says to use Sign in on the app's Status tab and run `/join` again

#### Scenario: Codex missing
- **WHEN** a member runs `/join` on the desktop app and no Codex executable is found
- **THEN** the bot stays out of the channel and the reply says that Codex CLI is not installed and to press Install Codex on the app's Status tab, then run `/join` again

#### Scenario: Headless startup prerequisites missing at join
- **WHEN** a member runs `/join` on the headless app and Codex is unavailable or signed out
- **THEN** the bot stays out of the channel and the reply directs the operator to install or configure Codex CLI, or to sign in from a terminal, as appropriate

### Requirement: Finding Codex
The app SHALL look for Codex in this order: the configured executable, then the platform's lookup when no path was configured. On Windows this SHALL remain `codex.exe` on `PATH`, winget's command links folder `%LOCALAPPDATA%\Microsoft\WinGet\Links`, npm's global folder, then the `codex.cmd` shim. On Linux (not tested) it SHALL look for `codex` on `PATH`. Each status read SHALL look again so a Codex installed while the app runs is found without a restart.

#### Scenario: Installed by winget while the app runs
- **WHEN** Codex is installed with winget after the app started and the app's PATH does not contain winget's links folder
- **THEN** the next status read finds `codex.exe` in that folder and shows its version

#### Scenario: Configured path wins
- **WHEN** the Codex executable is set in Config and Codex is also installed by winget
- **THEN** the configured executable is used

#### Scenario: Linux executable on PATH (not tested)
- **WHEN** no Codex executable is configured on Linux (not tested) and an executable named `codex` is on `PATH`
- **THEN** VoiceBro uses that executable without looking for Windows installs or shims

### Requirement: Join prepares the Whisper mode
In Whisper mode the join SHALL also load the Whisper model and the voice. When their files are not downloaded or fail to load, the bot SHALL not join and SHALL reply with the reason and what to do: on the desktop app, the window's downloads; on the headless app, `voicebro-server --install-assets`.

#### Scenario: Whisper files missing
- **WHEN** a member runs `/join` in Whisper mode on the desktop app and the selected Whisper model is not downloaded
- **THEN** the bot stays out of the channel, the reply says to open VoiceBro's Settings tab and press Download in the Whisper and Local voice cards, then run `/join` again, and the window opens the install dialog

#### Scenario: Whisper files missing on the headless app
- **WHEN** a member runs `/join` in Whisper mode on the headless app and the selected Whisper model or voice is not downloaded
- **THEN** the bot stays out of the channel and the reply lists the missing files and says to stop the bot and run `voicebro-server --install-assets`, then run `/join` again

#### Scenario: Whisper mode join
- **WHEN** a member runs `/join` in Whisper mode with every file on disk
- **THEN** the thread is open and Whisper and the voice are loaded before the bot is in the channel, and no realtime call is ever opened

### Requirement: Join prepares the text answers mode
In the text answers mode the join SHALL load the Whisper model as in the Whisper mode and SHALL neither load nor require the voice. When Whisper's files are not downloaded or fail to load, the bot SHALL not join and SHALL reply that they are missing and what to do: on the desktop app, to open VoiceBro's Settings tab and press Download in the Whisper card; on the headless app, to run `voicebro-server --install-assets`; then to run `/join` again.

#### Scenario: Only Whisper's files on disk
- **WHEN** a member runs `/join` in the text answers mode with Whisper's files on disk and the selected voice not downloaded
- **THEN** the bot joins, Whisper is loaded, and no voice is loaded

#### Scenario: Whisper's files missing
- **WHEN** a member runs `/join` in the text answers mode on the desktop app and the selected Whisper model is not downloaded
- **THEN** the bot stays out of the channel, the reply says to press Download in the Whisper card of VoiceBro's Settings tab and run `/join` again, and the window opens the install dialog

#### Scenario: Whisper's files missing on the headless app
- **WHEN** a member runs `/join` in the text answers mode on the headless app and the selected Whisper model is not downloaded
- **THEN** the bot stays out of the channel and the reply says to stop the bot and run `voicebro-server --install-assets`, then run `/join` again
