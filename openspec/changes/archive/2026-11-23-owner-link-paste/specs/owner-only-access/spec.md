## ADDED Requirements

### Requirement: A code sent in a message
A message SHALL be a link attempt when a person (not a bot or a webhook) sends it in a
direct message to the bot, or in a server channel with a mention of the bot, and its text
without the bot's mentions is `link` or `/link` followed by the code, or the code alone,
in any case, with or without spaces and dashes. The app SHALL handle a link attempt as a
`/link` with that code from the message's author.

#### Scenario: The copied message in a direct message
- **WHEN** a code waits and the owner pastes the message Copy put on the clipboard into a direct message to the bot and sends it
- **THEN** the owner's account is linked and the bot answers in that direct message that it is linked

#### Scenario: The copied message in the voice channel's chat
- **WHEN** a code waits and the owner pastes the copied message into the text chat of the bot's voice channel and sends it
- **THEN** the owner's account is linked and the bot answers in that chat

#### Scenario: The code alone in a direct message
- **WHEN** `K7QF-2M9X` waits and the owner sends `k7qf2m9x` in a direct message to the bot
- **THEN** the owner's account is linked

#### Scenario: No mention in a server channel
- **WHEN** a code waits and a member writes `link K7QF-2M9X` in a server channel without mentioning the bot
- **THEN** nothing is linked, the bot does not answer, and no wrong code is counted

#### Scenario: A mention that is no link attempt
- **WHEN** a code waits and a member sends "hello" with a mention of the bot in a server channel
- **THEN** nothing is linked, the bot does not answer, and no wrong code is counted

#### Scenario: A wrong code in a message
- **WHEN** `K7QF-2M9X` waits and a member sends `link AAAA-BBBB` with a mention of the bot
- **THEN** nothing is linked, one wrong code is counted, and the bot answers how many tries are left

#### Scenario: A bot posts the message
- **WHEN** a code waits and another bot or a webhook posts the bot's mention with `link` and the right code
- **THEN** nothing is linked, the bot does not answer, and the code keeps waiting

### Requirement: The bot answers a pasted code in the same chat
The bot SHALL answer a link attempt from a message in the chat the message came from, with
the text a `/link` with the same code is answered with, and its answer SHALL notify nobody.
When the bot cannot post there, the link attempt SHALL still take effect, the log SHALL say
that the answer could not be posted, and the window SHALL show the result.

#### Scenario: The answer in a server channel
- **WHEN** the owner links with the copied message in a server channel
- **THEN** the bot's answer appears in that channel and notifies no one

#### Scenario: No right to post
- **WHEN** the bot can read a channel but may not send messages there, and the owner sends the copied message in it
- **THEN** the owner's account is linked, the window shows the owner's name, and the log says the answer could not be posted in that channel

### Requirement: Messages are read only for a link code
The app SHALL use a message it receives from Discord only to find a link attempt. It SHALL
keep nothing of any message, SHALL NOT pass a message to the assistant, and SHALL do
nothing with a message that is not a link attempt.

#### Scenario: Chat in a channel the bot can see
- **WHEN** members chat in a channel the bot can see during a session, and one of them asks the bot a question with a mention of it
- **THEN** the bot does not answer, the log holds nothing about these messages, and the assistant does not receive them

## MODIFIED Requirements

### Requirement: Linking with a one-time code
Link in the window, or `voicebro-server --link-owner` on the headless app, SHALL read the selected engine's signed-in account (the ChatGPT account for Codex, the Claude account for Claude Code; OpenCode has none) and then show a code of 8 characters without look-alike characters, valid for 10 minutes: in the window on the desktop app, in the terminal on the headless app. A `/link` with that code, or a message with it as requirement "A code sent in a message" describes, SHALL make the Discord account that sent it the owner, record the engine and the account read for the code (none for OpenCode), and tell the sender that the account is linked: the answer to `/link` only the sender sees, the answer to a message is posted in the chat it came from.

#### Scenario: Successful link
- **WHEN** Codex is signed in, the user presses Link, and types `/link` with the shown code from their Discord account within 10 minutes
- **THEN** the window shows that account's name as the owner, the code disappears, and only the sender sees the reply that the account is linked

#### Scenario: Successful link from the terminal
- **WHEN** Codex is signed in, the operator runs `voicebro-server --link-owner`, and types `/link` with the printed code from their Discord account within 10 minutes
- **THEN** the terminal shows that account's name as the owner, and only the sender sees the reply that the account is linked

#### Scenario: Successful link with the copied message
- **WHEN** Codex is signed in, the user presses Link, then Copy, and sends the copied message from their Discord account in a direct message to the bot within 10 minutes
- **THEN** the window shows that account's name as the owner, the code disappears, and the bot answers in that direct message that the account is linked

#### Scenario: Codex not signed in
- **WHEN** the user presses Link while Codex reports no signed-in account
- **THEN** no code is shown and the window says to sign in to Codex first

#### Scenario: Claude Code not signed in
- **WHEN** the engine is Claude Code and the user presses Link while Claude Code reports no sign-in
- **THEN** no code is shown and the window says to sign in to Claude Code first

#### Scenario: OpenCode
- **WHEN** the engine is OpenCode and the user presses Link
- **THEN** a code is shown without reading any account, and a `/link` with it links the sender

#### Scenario: Code typed loosely
- **WHEN** the window shows `K7QF-2M9X` and the owner types `/link k7qf2m9x`
- **THEN** the code is accepted

### Requirement: A code works once
A code SHALL be accepted once. It SHALL end when it is used, after 10 minutes, when the user
presses Cancel, or after 5 wrong codes in total, whether they came with `/link` or in
messages. A wrong, ended or unknown code SHALL change nothing and SHALL get an answer: to
`/link` one only the sender sees, to a message one posted in the chat it came from.

#### Scenario: Expired code
- **WHEN** the owner types the code 11 minutes after it was shown
- **THEN** no link is made, the sender is told the code expired, and the window shows that no code is waiting

#### Scenario: Wrong codes
- **WHEN** someone in the server sends 5 wrong codes while a code is waiting
- **THEN** the waiting code ends, the right code typed afterwards links nothing, and the window says the code was ended after 5 wrong tries

#### Scenario: Wrong codes from both ways
- **WHEN** a code waits and someone sends 3 wrong codes with `/link` and then 2 wrong codes in messages that mention the bot
- **THEN** the waiting code ends and the window says the code was ended after 5 wrong tries

#### Scenario: Used code
- **WHEN** a second account sends the same code after it linked the first account
- **THEN** the first account stays the owner

#### Scenario: The copied message sent twice
- **WHEN** the owner linked with the copied message and sends the same message again
- **THEN** the owner stays linked and the bot answers that no code is waiting

### Requirement: The code stays in the window or the terminal
The app SHALL show the code only in its window, and the headless app only on the terminal's standard output. The app SHALL put the code on the clipboard only when the user presses Copy. It SHALL NOT write the code to the log file, the stderr log lines, the settings file or any Discord message the bot sends, and SHALL NOT write the text of a message it reads to the log.

#### Scenario: Log after linking
- **WHEN** a link is made with the verbose log on
- **THEN** the log says a code was issued and that an account was linked, and holds neither the code nor the typed value

#### Scenario: Headless log after linking
- **WHEN** a link is made through `voicebro-server --link-owner` with the verbose log on
- **THEN** the code appears once on standard output, and neither the log file nor the stderr lines hold the code or the typed value

#### Scenario: Log after a link by message
- **WHEN** a link is made with the copied message and the verbose log on
- **THEN** the log says a code came in a message and that an account was linked, and holds neither the code nor the message's text

#### Scenario: The bot's answer
- **WHEN** the bot answers a link attempt from a message, right or wrong
- **THEN** its answer holds neither the code nor the message's text
