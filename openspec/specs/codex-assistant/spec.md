# codex-assistant Specification

## Purpose
Runs the Codex app-server as the assistant: signed in with the user's ChatGPT account,
unable to act on the computer, with a voice call open only while a session is active.

## Requirements

### Requirement: App-server lifecycle
The app SHALL start `codex app-server --disable shell_tool` (the configured executable, or
an auto-detected `codex.exe`) on `/join`, and on Sign in or Check from the window, never at
app start. It SHALL stop the app-server when the bot leaves its last voice channel, one
minute after the last Sign in or Check while the bot is in no channel, and when the app
exits. A missing executable or a failed handshake SHALL be shown in the status with the
error text.

#### Scenario: Codex not installed
- **WHEN** no `codex.exe` is found and none is configured
- **THEN** the status shows the assistant as unavailable with "codex not found", and Discord keeps working

#### Scenario: App exit
- **WHEN** the main window is closed
- **THEN** the app-server process is gone within two seconds

#### Scenario: App start
- **WHEN** the app starts and nobody has used `/join`, Sign in or Check
- **THEN** no `codex.exe` process belongs to the app

#### Scenario: Last channel left
- **WHEN** the bot leaves its last voice channel
- **THEN** the app-server process is gone within two seconds and the conversation of each guild is kept for the next join

### Requirement: Nothing acts on the computer
Every thread SHALL be ephemeral with a read-only sandbox and `approvalPolicy: untrusted`, as
in the first version; every approval, tool call, user-input and elicitation request from the
server SHALL be declined automatically and logged. The agent's own server-side tools, web
search included, SHALL stay available.

#### Scenario: Agent asks to run a command
- **WHEN** the delegated agent requests command execution approval
- **THEN** the request is declined within the same second and a chat-log entry records it

#### Scenario: Question about current facts
- **WHEN** the voice model delegates a question that needs current information from the web
- **THEN** the agent may run a web search, and the chat log shows the search as a tool entry

### Requirement: Sign-in through Codex
The UI SHALL offer "Sign in" (opens Codex's own ChatGPT sign-in in the browser) and "Check"
(reads the account); the status SHALL show the account type when signed in and "not signed
in" otherwise. The app SHALL store no OpenAI credential.

#### Scenario: Not signed in
- **WHEN** the account read says no account
- **THEN** the status shows not signed in, a session start fails with that reason, and no call is attempted

### Requirement: Model and reasoning settings
The thread SHALL be created with `codex.model` and `codex.reasoning_effort` when set; a
change to either SHALL open a new thread at the next call and carry the history over; a
rejected model SHALL fall back to the account defaults and the log SHALL name both values.
The status SHALL show the model and effort actually in use.

#### Scenario: Model changed between calls
- **WHEN** the user picks another model and starts a new session
- **THEN** the new call runs on a new thread with that model and the previous conversation continues

#### Scenario: Model rejected
- **WHEN** the configured model is rejected by Codex
- **THEN** the thread is opened with the defaults, the status shows the defaults, and one log line names the requested and the used model

### Requirement: Call lifecycle
A voice call SHALL exist only while a session is wanted; `ready` SHALL be true only when
the call is connected; a stop SHALL complete before any restart; restarts after failures
SHALL be capped at three per session.

#### Scenario: Session wanted
- **WHEN** the gate wants a session
- **THEN** the call is set up, `ready` becomes true once media flows, and the gate's held audio is released

#### Scenario: Session not wanted
- **WHEN** the gate ends the session
- **THEN** the call is stopped, the output is flushed, and no audio is sent until the next session

#### Scenario: Call fails repeatedly
- **WHEN** the call fails to connect four times in one session
- **THEN** the session ends with the error in the status

### Requirement: Voice
The call SHALL use `codex.voice`; an empty value SHALL mean the server's default voice and
the call SHALL then be started without a voice. A voice unavailable with the sign-in SHALL
fall back to the server's default voice. The log SHALL name the voice actually used.

#### Scenario: Unavailable voice
- **WHEN** the configured voice is not in the list for the account
- **THEN** the default voice is used and the log says so once

#### Scenario: Server default chosen
- **WHEN** `codex.voice` is empty
- **THEN** the call is started without a voice field, and the log says the server default voice is used

### Requirement: Join prepares the assistant
On `/join` the app SHALL start the app-server if needed, read the account and open the
guild's thread before the bot enters the voice channel, so the first activation only places
the call. When Codex is missing, fails to start or is not signed in, the bot SHALL not join
and SHALL reply with the reason and what to do.

#### Scenario: Signed in
- **WHEN** a member runs `/join` and Codex is signed in
- **THEN** the thread is open before the bot is in the channel, and no voice call exists until an activation

#### Scenario: Signed out
- **WHEN** a member runs `/join` and the account read says no account
- **THEN** the bot stays out of the channel and the reply says to use Sign in on the app's Status tab and run `/join` again
