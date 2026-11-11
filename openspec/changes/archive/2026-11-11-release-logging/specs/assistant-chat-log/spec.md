# Spec Delta

## MODIFIED Requirements

### Requirement: Where the log lives
The log SHALL be kept in memory (bounded) and pushed to the UI as an event per entry. Only
while `assistant.save_chat_history` is on SHALL each entry also be appended to a daily file
`logs\codex-chat-YYYY-MM-DD.log` next to the executable; while it is off no chat file
SHALL be created or appended. Files written earlier SHALL be left as they are.

#### Scenario: Restart
- **WHEN** saving chat history is on and the app is restarted
- **THEN** the in-memory log starts empty and the day's file still holds the earlier entries

#### Scenario: Saving off
- **WHEN** saving chat history is off and a call records transcripts and answers
- **THEN** the Codex chat tab shows every entry and no `codex-chat-*.log` file is created or changed

#### Scenario: Turned on during a call
- **WHEN** the user turns saving chat history on while a call runs
- **THEN** the next chat entry is appended to the day's file, without a restart
