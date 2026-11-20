# Spec Delta

## MODIFIED Requirements

### Requirement: The voice mode needs Codex
The realtime voice mode SHALL be available only with Codex. Claude Code and OpenCode SHALL
work only in the Whisper mode and the text answers mode.

#### Scenario: OpenCode in the Whisper mode
- **WHEN** the engine is OpenCode, the assistant mode is Whisper and a member asks a question during a session
- **THEN** OpenCode answers in text and the local voice speaks the answer

#### Scenario: No call for Claude
- **WHEN** the engine is Claude Code and the bot is in a voice channel
- **THEN** no realtime voice call is ever opened

#### Scenario: Claude Code in the text answers mode
- **WHEN** the engine is Claude Code, the assistant mode is text and a member asks a question during a session
- **THEN** Claude Code answers in text and the answer is posted, with no voice
