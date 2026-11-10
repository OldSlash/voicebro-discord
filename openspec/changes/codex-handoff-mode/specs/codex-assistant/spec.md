# Spec Delta

## ADDED Requirements

### Requirement: Answer handoff mode
Each realtime call SHALL be started with the answer handoff mode set in `codex.handoff_mode`:
`items` SHALL add the Codex agent's answers to the voice conversation as their own items,
`commentary` and `bem_tags` SHALL select Codex's commentary or tagged-channel routing, and
`thinking` SHALL leave Codex's built-in routing in place. A changed mode SHALL apply from the
next call; a running call SHALL keep the mode it started with.

#### Scenario: Default mode
- **WHEN** a call starts with `codex.handoff_mode` set to `items`
- **THEN** the call is started with the agent's answers sent as conversation items and with no handoff routing field

#### Scenario: Built-in routing
- **WHEN** a call starts with `codex.handoff_mode` set to `thinking`
- **THEN** the call is started with neither the conversation-items field nor a handoff routing field, as the app did before this change

#### Scenario: Commentary routing
- **WHEN** a call starts with `codex.handoff_mode` set to `commentary`
- **THEN** the call is started with the handoff routing set to `commentary`

#### Scenario: Mode changed during a call
- **WHEN** the user saves another mode while a call is running
- **THEN** the running call is not restarted, and the next call uses the new mode

#### Scenario: Mode rejected by Codex
- **WHEN** Codex refuses to start a call with the chosen mode's field (for example a Codex version without it)
- **THEN** the call is started again with the built-in routing, and the log names the requested mode, the mode used and Codex's reason

### Requirement: The mode in use is visible
When a call starts, the log and the Codex chat SHALL name the answer handoff mode in use, in
the same line that names the voice and the Codex model.

#### Scenario: Call start line
- **WHEN** a call starts in the `items` mode
- **THEN** the call start line in the log and in the Codex chat says the answers are handed over as conversation items
