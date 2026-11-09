# Spec Delta

## Purpose

Runs the Codex app-server as the assistant: signed in with the user's ChatGPT account,
unable to act on the computer, with a voice call open only while a session is active.

## ADDED Requirements

### Requirement: App-server lifecycle
The app SHALL start `codex app-server --disable shell_tool` (the configured executable, or
an auto-detected `codex.exe`) when the bot connects to Discord and stop it when the app
exits. A missing executable or a failed handshake SHALL be shown in the status with the
error text.

#### Scenario: Codex not installed
- **WHEN** no `codex.exe` is found and none is configured
- **THEN** the status shows the assistant as unavailable with "codex not found", and Discord keeps working

#### Scenario: App exit
- **WHEN** the main window is closed
- **THEN** the app-server process is gone within two seconds

### Requirement: Nothing acts on the computer
Every thread SHALL be ephemeral with a read-only sandbox and `approvalPolicy: never`; every
approval, tool call, user-input and elicitation request from the server SHALL be declined
automatically and logged.

#### Scenario: Agent asks to run a command
- **WHEN** the delegated agent requests command execution approval
- **THEN** the request is declined within the same second and a chat-log entry records it

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
The call SHALL use `codex.voice`; a voice unavailable with the sign-in SHALL fall back to the
default voice, and the log SHALL name the voice actually used.

#### Scenario: Unavailable voice
- **WHEN** the configured voice is not in the list for the account
- **THEN** the default voice is used and the log says so once
