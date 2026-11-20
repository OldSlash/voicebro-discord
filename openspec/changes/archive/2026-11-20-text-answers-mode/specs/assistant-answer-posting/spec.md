# Spec Delta

## MODIFIED Requirements

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
