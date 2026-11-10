# Spec Delta

## MODIFIED Requirements

### Requirement: Chat tab
The Codex chat tab SHALL list the entries as they are recorded, one row each with time,
source label and text, with filters for names and notes, agent work and call events,
auto-scroll, a copy button and a button that opens the log folder. Source labels SHALL be the
speaker's name for a user transcript, `GPT`, `Context → GPT`, `Task → Codex`, `Codex`,
`Codex thinking`, `Codex tool`, `Call` and `Error`.

#### Scenario: Filter agent work
- **WHEN** the agent-work filter is off
- **THEN** delegated tasks, agent messages, reasoning and tool entries are hidden and transcripts remain

#### Scenario: Live entries
- **WHEN** a call records a user transcript and the assistant's answer
- **THEN** both rows appear in the Codex chat tab within a second, labelled with the speaker's name and `GPT`, and nothing about them appears in the Console as raw data

#### Scenario: Delegated work
- **WHEN** the voice model delegates a question and the agent searches the web
- **THEN** the rows `Task → Codex`, `Codex tool` and `Codex` appear in that order

## ADDED Requirements

### Requirement: Event names do not collide
Every event sent to the window SHALL carry its type in a field that no event payload uses for
anything else.

#### Scenario: Chat entry event
- **WHEN** a chat entry of kind `agent` is sent to the window
- **THEN** the window reads the event type as the chat event and the entry kind as `agent`
