# assistant-answer-posting Specification

## Purpose
Posts the assistant agent's full answers to a Discord text chat, so the people in the voice
channel can read what the voice only retells, open its links and see their previews.

## Requirements

### Requirement: Only the final answer of a completed agent task is posted
While `assistant.post_answers` is not `off`, or the assistant mode is `text`, the bot SHALL
post one answer for each agent task that completes: the last agent message of that task, as
written. In the text answers mode with `post_answers` `off` the answer SHALL go to the
voice channel's chat. It SHALL NOT post the agent's earlier messages of the task, its tool
calls or reasoning, what the voice says, or what people say. A task that is interrupted or
fails, or whose final agent message is empty, SHALL post nothing. A task still open when the
bot leaves that server's voice channel SHALL post nothing, whatever the destination: the
leave ends the agent's session and its open tasks with it.

#### Scenario: Interim line and final answer
- **WHEN** the voice delegates a question, the agent writes "I'll look that up", searches the web, then writes its answer and the task completes
- **THEN** one answer is posted, holding the agent's last message and not the interim line or the search

#### Scenario: Interrupted task
- **WHEN** the agent's task ends with the status `interrupted` or with an error
- **THEN** nothing is posted for that task

#### Scenario: The voice answers on its own
- **WHEN** the voice answers a question without delegating it to the agent
- **THEN** nothing is posted

#### Scenario: Two tasks
- **WHEN** two delegated tasks complete one after the other
- **THEN** two answers are posted, in the order the tasks completed

#### Scenario: Task completes after the leave
- **WHEN** a member runs `/leave` while a delegated task is still running
- **THEN** nothing is posted for that task, in any destination

#### Scenario: Off
- **WHEN** `assistant.post_answers` is `off` in the realtime or Whisper mode and an agent task completes
- **THEN** nothing is posted and Discord receives no request for it

#### Scenario: Off in the text answers mode
- **WHEN** `assistant.post_answers` is `off`, the assistant mode is `text`, and a turn completes
- **THEN** its answer is posted to the chat of the voice channel the bot is in

### Requirement: Posting to the voice channel's chat
With `assistant.post_answers` set to `voice_channel`, an answer SHALL be posted in the
built-in text chat of the voice channel the bot is in for that server.

#### Scenario: Answer in the voice channel's chat
- **WHEN** the bot is in the voice channel "Games" and an agent task completes
- **THEN** the answer appears in the text chat of "Games"

### Requirement: Posting to the channel of the join
With `assistant.post_answers` set to `command_channel`, an answer SHALL be posted in the
channel where the `/join` of the current voice session was typed.

#### Scenario: Join typed in a text channel
- **WHEN** a member types `/join` in the text channel "bot" and an agent task completes
- **THEN** the answer appears in "bot", where the panel is

#### Scenario: Join typed in the voice channel's chat
- **WHEN** a member types `/join` in the text chat of the voice channel and an agent task completes
- **THEN** the answer appears in that voice channel's chat

### Requirement: Posting to a chosen channel
With `assistant.post_answers` set to `channel`, an answer SHALL be posted in the channel
`assistant.answers_channel` names, when the answer comes from that channel's server. An
answer from another server SHALL NOT be posted anywhere, and the log SHALL say so once per
voice session of that server.

#### Scenario: Same server
- **WHEN** the chosen channel is "answers" in server A and an agent task completes while the bot is in a voice channel of server A
- **THEN** the answer appears in "answers"

#### Scenario: Another server
- **WHEN** the chosen channel is in server A and an agent task completes while the bot is in a voice channel of server B
- **THEN** nothing is posted, in server A or B, and the log says the chosen channel belongs to another server

### Requirement: The answer is readable in Discord
A posted answer SHALL start with one small line naming the task, at most 150 characters of
it, followed by the agent's message with its Markdown kept. A Markdown table SHALL be posted
inside a code block so its columns stay aligned. Link previews SHALL stay on.

#### Scenario: Task line
- **WHEN** the voice delegates "find the weather in Bangkok tomorrow" and the agent answers
- **THEN** the post's first line is a small line naming that task and the answer follows with its bold text, lists and links as Discord Markdown

#### Scenario: A table
- **WHEN** the agent's answer contains a Markdown table
- **THEN** the table is posted inside a code block and the rest of the answer is unchanged

#### Scenario: A link
- **WHEN** the answer contains a link to a page that has a preview
- **THEN** Discord shows the link and the page's preview under the message

### Requirement: Long answers are split
An answer longer than one Discord message SHALL be posted as several messages of at most
2000 characters each, sent one after the other in order. A split SHALL fall at a paragraph
break, else at a line break, else at a space, and never inside a code block: a code block
longer than one message SHALL be closed at the end of a part and reopened, with its language,
at the start of the next.

#### Scenario: A long answer
- **WHEN** the agent's answer is 4500 characters of paragraphs
- **THEN** three messages are posted in order, each at most 2000 characters, each ending at the end of a paragraph

#### Scenario: A long code block
- **WHEN** the answer holds a 3000-character code block
- **THEN** every message that holds part of it opens and closes the block, so each message renders as code

### Requirement: Posted answers ping nobody
A posted answer SHALL mention nobody: `@everyone`, `@here`, role mentions and user mentions
in the agent's text SHALL appear as text and SHALL NOT notify anyone.

#### Scenario: The agent writes @everyone
- **WHEN** the agent's answer contains `@everyone` and a user mention
- **THEN** the message shows them as text and no member gets a notification or a mention badge

### Requirement: A refused post is visible
When Discord refuses a post (for example a missing permission or a deleted channel), the
status SHALL show a connection error that names the channel and Discord's reason, and the log
SHALL hold the same line. The remaining parts of that answer SHALL NOT be sent, the post
SHALL NOT be retried, and the next successful post SHALL clear the error.

#### Scenario: Missing permission
- **WHEN** the bot's role may not send messages in the chosen channel and an agent task completes
- **THEN** the status shows an answers error naming the channel and the missing permission, and the answer is not posted

#### Scenario: Recovered
- **WHEN** the permission is granted and the next agent task completes
- **THEN** the answer is posted and the answers error is cleared from the status

### Requirement: The log keeps no answer text by default
Each post SHALL be logged as one line naming the server, the channel, the number of messages
and the answer's length. The answer's text SHALL be in the log file only while the verbose
log is on.

#### Scenario: Release build, verbose log off
- **WHEN** an answer is posted
- **THEN** the log file has a line with the server, the channel, the number of messages and the length, and none of the answer's text
