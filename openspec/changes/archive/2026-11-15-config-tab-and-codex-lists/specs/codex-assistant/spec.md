## MODIFIED Requirements

### Requirement: App-server lifecycle
The app SHALL start `codex app-server --disable shell_tool` (the configured executable, or
an auto-detected `codex.exe`) on `/join`, on Sign in, Check or Refresh lists from the window,
and at app start only to read the lists as the requirement "Lists are read at start" says.
It SHALL stop the app-server when the bot leaves its last voice channel, one minute after
the last Sign in, Check, Refresh lists or read at start while the bot is in no channel, and
when the app exits. A missing executable or a failed handshake SHALL be shown in the status
with the error text.

#### Scenario: Codex not installed
- **WHEN** no `codex.exe` is found and none is configured
- **THEN** the status shows the assistant as unavailable with "codex not found", and Discord keeps working

#### Scenario: App exit
- **WHEN** the main window is closed
- **THEN** the app-server process is gone within two seconds

#### Scenario: App start
- **WHEN** the app starts with saved lists and the refresh at start off, and nobody has used `/join`, Sign in, Check or Refresh lists
- **THEN** no `codex.exe` process belongs to the app

#### Scenario: Read at start ends
- **WHEN** the app read the lists at start and the bot joins no voice channel
- **THEN** the app-server process is gone about one minute after the read

#### Scenario: Last channel left
- **WHEN** the bot leaves its last voice channel
- **THEN** the app-server process is gone within two seconds and the conversation of each guild is kept for the next join

## ADDED Requirements

### Requirement: Codex lists are saved
After every successful read of the model list from Codex the app SHALL save the models with
their reasoning efforts, the voices and the default voice, with the time of the read, to
`codex-lists.json` in the folder the executable runs from, written atomically. At start it
SHALL load that file, so the lists are offered before any Codex process runs. A missing or
unreadable file SHALL mean no saved lists, with a log line when it is unreadable.

#### Scenario: Saved after Check
- **WHEN** the user presses Check Codex while signed in
- **THEN** `codex-lists.json` holds the models, efforts and voices Codex returned and the time of the read

#### Scenario: Loaded at start
- **WHEN** the app starts with `codex-lists.json` from an earlier run and the refresh at start off
- **THEN** the assistant's capabilities list those models and voices with the saved read time, and no Codex process runs

#### Scenario: Broken file
- **WHEN** `codex-lists.json` holds no valid JSON
- **THEN** the app starts without saved lists, logs that the file could not be read, and replaces it at the next successful read

#### Scenario: Read failed
- **WHEN** a read of the model list fails
- **THEN** the saved lists and the file stay as they were

### Requirement: Lists are read at start
At start, when a Codex executable is found, the app SHALL read the account, the models and
the voices from Codex, as Check does, when `codex.refresh_lists_at_start` is on or when no
saved lists exist. Otherwise no Codex process SHALL start at app start. Without an
executable the app SHALL start nothing and report no error for the read.

#### Scenario: Switch on
- **WHEN** the app starts with `codex.refresh_lists_at_start` on and Codex signed in
- **THEN** the app-server starts, the lists are read and saved, the window's lists follow without a reload, and the log names the account

#### Scenario: Nothing saved yet
- **WHEN** the app starts for the first time with Codex installed and the switch off
- **THEN** the lists are read once and saved, and the next start with the switch off starts no Codex process

#### Scenario: Codex not installed
- **WHEN** the app starts with no saved lists and no Codex executable
- **THEN** no process is started and the status shows the usual "codex not found"

#### Scenario: A join during the read
- **WHEN** a member runs `/join` while the read at start is still running
- **THEN** the join uses the same app-server, and the app-server is not stopped while the bot is in the channel

### Requirement: The lists are read again from Codex
A join preparation SHALL read the models and voices from Codex when they have not been read
from Codex yet in this run, even when saved lists were loaded; later joins of the run SHALL
reuse them. Check and Refresh lists SHALL read the models and the voices every time. Each
successful read SHALL replace the saved lists.

#### Scenario: First join after start
- **WHEN** the app started with saved lists and the switch off, and a member runs `/join`
- **THEN** the join reads the model list and the voices from Codex and saves them

#### Scenario: Second join
- **WHEN** a second `/join` follows in the same run
- **THEN** it does not read the model list or the voices again

#### Scenario: New model offered
- **WHEN** Codex offers a model that the saved lists do not have and the user presses Refresh lists
- **THEN** the model control lists the new model and the saved file holds it
