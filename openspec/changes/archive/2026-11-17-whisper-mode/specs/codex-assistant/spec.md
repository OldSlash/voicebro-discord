# Spec Delta

## MODIFIED Requirements

### Requirement: Join prepares the assistant
On `/join` the app SHALL start the app-server if needed, read the account and open the
guild's thread before the bot enters the voice channel, so the first activation only places
the call or, in Whisper mode, sends the first question. In Whisper mode the join SHALL also
load the Whisper model and the voice. When Codex is missing, fails to start or is not signed
in, or, in Whisper mode, when the Whisper or voice files are not downloaded or fail to load,
the bot SHALL not join and SHALL reply with the reason and what to do.

#### Scenario: Signed in
- **WHEN** a member runs `/join` and Codex is signed in
- **THEN** the thread is open before the bot is in the channel, and no voice call exists until an activation

#### Scenario: Signed out
- **WHEN** a member runs `/join` and the account read says no account
- **THEN** the bot stays out of the channel and the reply says to use Sign in on the app's Status tab and run `/join` again

#### Scenario: Codex missing
- **WHEN** a member runs `/join` and no Codex executable is found
- **THEN** the bot stays out of the channel and the reply says that Codex CLI is not installed and to press Install Codex on the app's Status tab, then run `/join` again

#### Scenario: Whisper files missing
- **WHEN** a member runs `/join` in Whisper mode and the selected Whisper model is not downloaded
- **THEN** the bot stays out of the channel, the reply says to open VoiceBro's Whisper tab and press Download, then run `/join` again, and the window opens the install dialog

#### Scenario: Whisper mode join
- **WHEN** a member runs `/join` in Whisper mode with every file on disk
- **THEN** the thread is open and Whisper and the voice are loaded before the bot is in the channel, and no realtime call is ever opened

## ADDED Requirements

### Requirement: Text turns in Whisper mode
In Whisper mode the Codex session SHALL send each question as `turn/start` with a text
input on the guild's thread. It SHALL stream `item/agentMessage/delta` to the voice and
interrupt a running turn with `turn/interrupt` when a new question arrives or the session
ends. Every approval and tool request SHALL still be declined as in realtime mode.

#### Scenario: Interrupted turn
- **WHEN** a second question arrives while Codex is still answering the first
- **THEN** the app sends `turn/interrupt` for the first turn's id before `turn/start` for the second

### Requirement: Channel context in Whisper mode
In Whisper mode the Codex session SHALL send the people in the channel as
`additionalContext` with the first turn of a session, only after a live canary test has
shown that Codex uses that field; otherwise it SHALL send them as the first line of that
question. A new thread SHALL get the guild's last exchanges, at most 24 KB, the same way.

#### Scenario: Context canary
- **WHEN** the live probe asks "who is in the voice channel?" as the first question of a session
- **THEN** the answer names the people the app sent, and the design records which way the context went
