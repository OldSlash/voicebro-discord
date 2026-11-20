# whisper-assistant-mode Specification

## Purpose
Gives the app a second assistant mode next to the realtime voice call: Whisper's text
spots the phrases and carries the questions, Codex answers them in text turns, and a local
voice speaks the answers into the channel. Only text leaves the PC.

## Requirements

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

### Requirement: Both modes share the session rules
Both modes SHALL use the selected engine with the same sign-in, conversation,
instruction, speaker names and answer posting. The session mode (always or wake word), the
idle timeout, the earcons, the mute and the access filter SHALL work in both.

#### Scenario: Owner-only access in Whisper mode
- **WHEN** owner-only access is on and the assistant mode is Whisper
- **THEN** only the linked owner's voice can start a session or ask a question, as in realtime mode

### Requirement: The mode switches at once
A change of `assistant.mode` SHALL take effect at once:

- the wake engine unloads (to Whisper) or loads (to realtime);
- in every voice channel the bot is in, the session ends with the falling tone;
- the guild's assistant session is replaced by one in the new mode, on the same gate and
  output, without the bot leaving the channel.

`/status` and the member panel SHALL name the mode.

#### Scenario: Switched to Whisper while in a channel
- **WHEN** the bot is in a voice channel in realtime mode and the user picks Whisper
- **THEN** any realtime call ends, the bot stays in the channel, Vosk unloads, Whisper and the voice load, and the next activation phrase is spotted by Whisper

#### Scenario: Files missing at the switch
- **WHEN** the user switches to Whisper and the model or the voice is not downloaded
- **THEN** the mode is saved, no dialog opens, Config → Assistant says what is not downloaded with a button to Settings, and nothing is downloaded until Download is pressed there

### Requirement: Phrases in Whisper's text
In Whisper mode the activation and deactivation phrases SHALL come from
`whisper.phrases`. They SHALL be matched in Whisper's text as whole words, ignoring case
and punctuation, with "ё" read as "е". In always mode the phrases SHALL not be used.

#### Scenario: Case and punctuation
- **WHEN** the activation phrase is "hey bro" and Whisper's text of an utterance is "HEY... BRO!"
- **THEN** the phrase is matched

### Requirement: Activation phrase in Whisper mode
In Whisper mode, while waiting, an utterance holding an activation phrase SHALL start the
session, and the words after the phrase in that utterance SHALL be the first question.

#### Scenario: Phrase and question in one breath
- **WHEN** the activation phrase is "hey bro" and a member says "Hey bro, what is the weather in Bangkok?"
- **THEN** the session starts with the rising tone and Codex gets the question "what is the weather in Bangkok?"

#### Scenario: Phrase alone
- **WHEN** a member says only "Hey bro."
- **THEN** the session starts, nothing is sent to Codex, and the member's next utterance is the question

#### Scenario: Talk while waiting
- **WHEN** members talk without the activation phrase while no session is active
- **THEN** nothing reaches Codex, and what Whisper heard appears only as detail lines

### Requirement: Deactivation phrase in Whisper mode
In Whisper mode, while a session is active, an utterance holding a deactivation phrase
SHALL end the session at once, and the words before the phrase SHALL be dropped.

#### Scenario: Deactivation inside a sentence
- **WHEN** the deactivation phrase is "goodbye" and a member says "ok thanks, goodbye" during a session
- **THEN** the session ends with the falling tone and nothing of that utterance reaches Codex

### Requirement: Questions are text turns of the selected engine
In Whisper mode each question SHALL be sent to the guild's conversation with the selected
engine as a text turn, or, while one speaker at a time is on, handed over as requirement
"Questions wait their turn" says. While speaker names are on, the text SHALL be prefixed
with the speaker's display name. The first turn of each session SHALL carry the people in
the voice channel and the caller as context.

#### Scenario: Speaker named
- **WHEN** Alex asks "what time is it in London" during a session with speaker names at call start
- **THEN** the engine gets "Alex: what time is it in London"

#### Scenario: Added question named
- **WHEN** one speaker at a time is on and Sam's question is handed to the engine while it works, with speaker names at call start
- **THEN** the engine gets it as "Sam: " followed by the question

### Requirement: Whisper mode adds no instruction of its own
In Whisper mode the app SHALL add no instruction of its own: the instruction the engine
gets from the app SHALL be the user's `assistant.instructions` only.

#### Scenario: Developer instructions
- **WHEN** a question is sent to Codex in Whisper mode
- **THEN** the thread's developer instructions are the user's `assistant.instructions` and nothing else

#### Scenario: Claude Code's instruction
- **WHEN** a question is sent to Claude Code
- **THEN** the only instruction the app added to Claude Code's own system prompt is the user's `assistant.instructions`

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

### Requirement: Barge-in on words
While `assistant.one_speaker_at_a_time` is off, a new question during a session SHALL
interrupt a running turn and stop the voice at once, and SHALL then start a new turn. While
it is on, requirement "Questions wait their turn" SHALL apply instead. While
`assistant.interrupt_on_speech` is off, sound without words SHALL interrupt nothing in
either case; while it is on, requirement "The answer stops when someone starts talking"
SHALL apply as well.

#### Scenario: Follow-up while the answer plays
- **WHEN** one speaker at a time is off and a member asks a new question while the voice is still reading the previous answer
- **THEN** the voice stops, the previous turn is interrupted, and the answer to the new question follows

#### Scenario: Cough during an answer
- **WHEN** stopping the answer at the start of speech is off and a member coughs while the voice speaks
- **THEN** the answer goes on

### Requirement: The end of a session stops everything
A deactivation, the idle timeout, mute and a mode change SHALL interrupt a running turn,
stop the voice within one frame, and drop questions that were not sent. A running turn
SHALL count as activity for the idle timeout.

#### Scenario: Long web search
- **WHEN** Codex searches the web for 30 s with an idle timeout of 20 s
- **THEN** the session stays active until the answer has been spoken and the timeout then elapses

#### Scenario: Mute during an answer
- **WHEN** mute is turned on while the voice speaks
- **THEN** the voice stops at once, the turn is interrupted, and nothing more is sent to Codex

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

### Requirement: Questions wait their turn
While `assistant.one_speaker_at_a_time` is on, a question during a session SHALL NOT
interrupt a running turn or stop the voice. A question that comes while the engine works on
a turn SHALL be handed to the engine without interrupting it: Codex adds it to the running
turn, OpenCode takes it up at its next step, and Claude Code answers it in the next turn. A
question that comes while an answer is spoken SHALL start the next turn. Answers SHALL be
spoken in the order of their turns.

#### Scenario: Second question while Codex works
- **WHEN** one speaker at a time is on, the engine is Codex, Alex asks "What time is it in Bangkok?", and Sam asks "And what is the weather in London?" while Codex is still working on Alex's question
- **THEN** Alex's turn is not interrupted, Sam's question is added to it, and the answer covers both questions

#### Scenario: Follow-up while the answer plays, one at a time
- **WHEN** one speaker at a time is on and a member asks a new question while the voice is still reading the previous answer
- **THEN** the voice reads the previous answer to its end, and the answer to the new question follows it

#### Scenario: The running turn ends at the same moment
- **WHEN** one speaker at a time is on and a question comes just as the engine finishes the running turn and cannot add it there
- **THEN** the question starts the next turn and is answered

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

### Requirement: The answer stops when someone starts talking
In the Whisper mode, while `assistant.interrupt_on_speech` is on and one speaker at a time
is off, when one person's speech in an utterance reaches 600 ms of frames at or above the
speech threshold during a session, while a turn runs or the voice speaks, the voice SHALL
stop at once and the running turn SHALL be interrupted, once per utterance. The utterance
SHALL still be written down and SHALL be the next question as usual.

#### Scenario: "Wait" and a long phrase
- **WHEN** the setting is on, the voice reads an answer, and a member says "Wait, I meant the other server" without a pause
- **THEN** the voice stops about 600 ms after the member started talking, before the phrase ends, the turn is interrupted, and the phrase becomes the next question when Whisper has written it down

#### Scenario: A sneeze
- **WHEN** the setting is on and a member sneezes for 300 ms while the voice speaks
- **THEN** the answer goes on

#### Scenario: Outside a session
- **WHEN** the setting is on, the activation is the wake word, no session is active, and a member talks for two seconds
- **THEN** nothing is stopped

#### Scenario: Nothing to stop
- **WHEN** the setting is on during a session, no turn runs and the voice is silent, and a member talks for two seconds
- **THEN** nothing is interrupted, and the utterance becomes a question as usual

#### Scenario: One speaker at a time
- **WHEN** the setting is on, one speaker at a time is on, and a member talks for two seconds while the voice speaks
- **THEN** the answer goes on, as requirement "Questions wait their turn" says

#### Scenario: Off
- **WHEN** the setting is off and a member talks for two seconds while the voice speaks
- **THEN** the voice stops only when Whisper has written the utterance down and it is a question

#### Scenario: Logged
- **WHEN** the answer is stopped at the start of speech
- **THEN** the log has a line naming the speaker and saying the answer stopped after 600 ms of speech
