# Spec Delta

## RENAMED Requirements

- FROM: `### Requirement: Two assistant modes`
- TO: `### Requirement: Assistant modes`

## MODIFIED Requirements

### Requirement: Assistant modes
The setting `assistant.mode` SHALL choose how people talk to the assistant:

- `realtime` (the default): Vosk spots the phrases and a Codex realtime voice call hears the
  channel and speaks; only Codex offers it;
- `whisper`: Whisper turns speech into text and spots the phrases, the selected engine
  (Codex, Claude Code or OpenCode) answers in text, and a local voice speaks the answer;
- `text`: the text answers mode, as `whisper` but with the answers posted as text and no
  voice.

#### Scenario: A new settings file
- **WHEN** the app starts with no settings file
- **THEN** the assistant mode is realtime and the app behaves as before this change

#### Scenario: Whisper with Claude Code
- **WHEN** the engine is Claude Code and the assistant mode is Whisper
- **THEN** Whisper spots the phrases, Claude Code answers in text and the local voice speaks

#### Scenario: Text answers with OpenCode
- **WHEN** the engine is OpenCode and the assistant mode is text
- **THEN** Whisper spots the phrases, OpenCode answers in text, the answer is posted in a Discord chat, and nothing is played into the voice channel but the earcons

### Requirement: Whisper answers are posted
In Whisper mode, when a turn completes, its last agent message SHALL be handed to answer
posting, as a delegated task's answer is in realtime mode. In the text answers mode it
SHALL be posted also while `assistant.post_answers` is `off`, to the voice channel's chat.

#### Scenario: Answer posted
- **WHEN** posting answers is on and a turn completes
- **THEN** the turn's last agent message is posted to the chosen chat, with the question as the task line

#### Scenario: Text answers with posting off
- **WHEN** the assistant mode is text, `assistant.post_answers` is `off`, and a turn completes
- **THEN** the turn's last agent message is posted to the voice channel's chat, with the question as the task line

### Requirement: Each Whisper answer is measured
For each answer in Whisper mode the app SHALL log one latency line with:

- the time from the end of the speech to the text;
- the audio length, the model and the device;
- the time from the text to Codex's first words;
- the time from the first words to the voice;
- the time from the end of the speech to the voice.

The line SHALL be added to the status's recent turns. In the text answers mode the two
times of the voice SHALL be left out and the line SHALL say that no voice is used.

#### Scenario: One answer
- **WHEN** a question is answered in Whisper mode
- **THEN** the log has one `[latency:<guild>] whisper turn <n>: …` line with the four times, and the Settings tab's Whisper card shows them

#### Scenario: One answer in the text answers mode
- **WHEN** a question is answered in the text answers mode
- **THEN** the log has one `[latency:<guild>] whisper turn <n>: …` line with the time to the text and the time to the engine's first words, and it says that no voice is used

## ADDED Requirements

### Requirement: The text answers mode is the Whisper mode without the voice
In the text answers mode every requirement of the Whisper mode SHALL apply, except that no
local voice SHALL be loaded, required or played. `/status` and the member panel SHALL name
the mode "Whisper mode, text answers".

#### Scenario: A question in the text answers mode
- **WHEN** the mode is text, a member says the activation phrase and asks "What time is it in London?"
- **THEN** the session starts with the rising tone, the engine answers in a text turn, the answer is posted, and no voice is played

#### Scenario: Barge-in as in the Whisper mode
- **WHEN** the mode is text, one speaker at a time is off, and a member asks a new question while the engine still works on the previous one
- **THEN** the previous turn is interrupted and posts nothing, and the new question's answer is posted

#### Scenario: Status names the mode
- **WHEN** a member runs `/status` while the mode is text
- **THEN** the reply names the mode "Whisper mode, text answers"
