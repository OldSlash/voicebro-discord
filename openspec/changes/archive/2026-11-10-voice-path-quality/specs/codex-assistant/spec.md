# Spec Delta

## MODIFIED Requirements

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
