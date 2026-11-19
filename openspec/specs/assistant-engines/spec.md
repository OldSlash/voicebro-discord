# assistant-engines Specification

## Purpose
Lets the user choose which AI engine answers in the voice channel (Codex, Claude Code or
OpenCode), says which assistant modes each engine supports, and keeps what the engines share
the same whichever one runs.

## Requirements

### Requirement: Three assistant engines
The setting `assistant.engine` SHALL choose the engine that answers: `codex` (the default),
`claude` (Claude Code) or `opencode` (OpenCode). Every engine SHALL be optional: nothing of
an engine runs unless it is the chosen one and the bot is preparing a join, is in a voice
channel, or the user pressed one of that engine's buttons on the Status tab.

#### Scenario: A new settings file
- **WHEN** the app starts with no settings file
- **THEN** the engine is Codex and the app behaves as before this change

#### Scenario: Claude chosen, Codex untouched
- **WHEN** the engine is Claude Code and a member runs `/join`
- **THEN** no Codex process is started for the join

### Requirement: The voice mode needs Codex
The realtime voice mode SHALL be available only with Codex. Claude Code and OpenCode SHALL
work only in the Whisper mode.

#### Scenario: OpenCode in the Whisper mode
- **WHEN** the engine is OpenCode, the assistant mode is Whisper and a member asks a question during a session
- **THEN** OpenCode answers in text and the local voice speaks the answer

#### Scenario: No call for Claude
- **WHEN** the engine is Claude Code and the bot is in a voice channel
- **THEN** no realtime voice call is ever opened

### Requirement: The engine switches at once
A change of `assistant.engine` SHALL take effect at once: in every voice channel the bot is
in, a running session SHALL end with the falling tone, the guild's assistant session SHALL
be replaced by one of the new engine on the same gate and output, and the bot SHALL stay in
the channel. The old engine's processes SHALL stop as after the last leave.

#### Scenario: Codex to Claude in a channel
- **WHEN** the bot is in a voice channel in the Whisper mode with Codex and the user picks Claude Code
- **THEN** any running answer stops, the bot stays in the channel, the Codex app-server stops shortly after, and the next question goes to Claude Code

#### Scenario: The new engine cannot start
- **WHEN** the user switches to Claude Code while the bot is in a channel and Claude Code is not signed in
- **THEN** the bot stays in the channel, a connection error names the engine and what to do, and no question is sent anywhere until the engine is ready

### Requirement: The engines share the session rules
Whichever engine runs, the session mode, the wake and Whisper phrases, the idle timeout,
the earcons, the mute, the access mode, speaker names, one speaker at a time, answer
posting, the chat log and the user's instruction SHALL work as they do with Codex.

#### Scenario: Owner-only with OpenCode
- **WHEN** the engine is OpenCode, the access mode is "Only my Discord account" and another member says the activation phrase
- **THEN** no session starts

#### Scenario: Answers posted from Claude
- **WHEN** the engine is Claude Code, answer posting is on and Claude completes an answer
- **THEN** the answer is posted to the chosen channel as Codex answers are

### Requirement: The engine is named where people see it
The member panel's Engine field, `/status`, the label of a posted answer, the Chat tab's
labels and the latency log line SHALL name the engine in use: "Codex", "Claude" or
"OpenCode". `/optin`'s command description SHALL name no provider.

#### Scenario: Posted label
- **WHEN** Claude Code completes an answer and answer posting is on
- **THEN** the posted message's small label line names Claude, not Codex

#### Scenario: Command description
- **WHEN** a member opens the list of the bot's commands in Discord
- **THEN** `/optin`'s description says the speech goes to the bot's AI assistant and names no company
