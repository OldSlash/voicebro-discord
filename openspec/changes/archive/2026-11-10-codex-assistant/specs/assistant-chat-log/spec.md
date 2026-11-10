# Spec Delta

## Purpose

Shows the user what the assistant heard, said and delegated, so a wrong answer can be traced
to its cause.

## ADDED Requirements

### Requirement: Entries
The chat log SHALL record, per guild, transcripts of the user and the assistant, the call
context sent, call start and end, errors, and the delegated agent work (the delegated task,
agent messages, reasoning summaries and tool items), each with a kind, a time and the
speaker when known. Delegated tasks SHALL be shown unwrapped from their transport markup.

#### Scenario: A call with a delegated task
- **WHEN** the voice model delegates a question to the Codex agent
- **THEN** the log shows the delegated task text, then the agent's message, in order, with their kinds

### Requirement: Where the log lives
The log SHALL be kept in memory (bounded), pushed to the UI as an event per entry, and
appended to a daily file `logs\codex-chat-YYYY-MM-DD.log` next to the executable.

#### Scenario: Restart
- **WHEN** the app is restarted
- **THEN** the in-memory log starts empty and the day's file still holds the earlier entries

### Requirement: Chat tab
The Chat tab SHALL list the entries with filters for names and notes, agent work and call
events, auto-scroll, a copy button and a button that opens the log folder.

#### Scenario: Filter agent work
- **WHEN** the agent-work filter is off
- **THEN** delegated tasks, agent messages, reasoning and tool entries are hidden and transcripts remain
