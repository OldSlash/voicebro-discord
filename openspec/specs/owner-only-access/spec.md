# owner-only-access Specification

## Purpose
Lets the subscription holder link their own Discord account to the app once, and limits the
voice that reaches the assistant to that account while the owner-only mode is on, so the
subscription answers only its holder. The bot's commands and its answers stay open to
everyone.

## Requirements

### Requirement: The owner-only mode is optional
The owner-only mode SHALL be on while `discord.access` is `owner` and off for its other
values. While `discord.access` is `everyone`, the app SHALL let every member in the bot's
voice channel reach the wake word and the assistant, as without this mode. While it is
`agreed`, the `speaker-consent` capability decides which voices pass. A stored link SHALL
change nothing while the access mode is `everyone`.

#### Scenario: Mode off with a link
- **WHEN** the access mode is `everyone`, an owner is linked, and another member says the activation phrase
- **THEN** a session starts as it would without a link

#### Scenario: The agreed mode with a link
- **WHEN** the access mode is `agreed`, an owner is linked, and the owner says the activation phrase
- **THEN** a session starts

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

### Requirement: Where `/link` works
`/link` SHALL work in every server the bot is in and in a direct message to the bot.

#### Scenario: Direct message
- **WHEN** the owner sends `/link` with the waiting code in a direct message to the bot
- **THEN** the account is linked as in a server

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

### Requirement: The link is stored encrypted in the app's folder
The app SHALL keep the link as one file in its own folder, encrypted so that only the same user on the same machine can read it: on Windows with DPAPI for the current Windows user; on Linux (not tested) with authenticated encryption under a key that only the user running VoiceBro can read, kept in that user's home folder and bound to the machine. The app SHALL never write the link unencrypted, except on Linux (not tested) while `linux.encrypt_files` is `false`, when it SHALL be a plain file readable and writable only by the user running VoiceBro; when it cannot encrypt, it SHALL keep no link on disk and SHALL report why. A file that cannot be decrypted or read as a link SHALL count as no link and SHALL be reported in the log once at start.

#### Scenario: Restart
- **WHEN** an owner is linked and the app is closed and started again
- **THEN** the same owner is linked without a new code

#### Scenario: Edited file
- **WHEN** one byte of the link file is changed and the app starts
- **THEN** no owner is linked, the window (on the headless app, the terminal) shows "not linked", and the log names the file as unreadable

#### Scenario: Another Windows user
- **WHEN** the app's folder with the link file is copied to another Windows user's folder and started there
- **THEN** no owner is linked

#### Scenario: Another Linux user or machine (not tested)
- **WHEN** the data folder with the link file is copied to another Linux user, or to another machine together with the key file, and headless is started there (not tested)
- **THEN** no owner is linked, and the log names the file as unreadable

#### Scenario: Encryption turned off on Linux (not tested)
- **WHEN** an owner links on Linux (not tested) while `linux.encrypt_files` is `false`
- **THEN** the link file is plain and readable and writable only by the user running VoiceBro, and the terminal warned at start that it is not encrypted

#### Scenario: Key readable by others on Linux (not tested)
- **WHEN** the key file on Linux (not tested) can be read by other users and an owner links with `/link`
- **THEN** the link is not written, the sender's reply and the terminal say it could not be stored, and the message names the key file and its permissions

### Requirement: The owner is set only by a code
The window and the settings file SHALL offer no way to set the owner's account other than a
code sent from that account.

#### Scenario: Owner id in the settings file
- **WHEN** `settings.json` is edited to hold a Discord account id under any name
- **THEN** the linked owner does not change

### Requirement: A link lasts until it is replaced or removed
A link SHALL have no expiry. It SHALL end only when the user presses Unlink or, on the headless app, runs `voicebro-server --unlink-owner`, when another Discord account completes a new code, which replaces it, when its file is deleted with the app's folder or the app's data, or as the requirement on the assistant account says.

#### Scenario: Unlink
- **WHEN** the user presses Unlink
- **THEN** the link file is gone, the window shows "not linked", and the log says the link was removed

#### Scenario: Unlink from the terminal
- **WHEN** the operator runs `voicebro-server --unlink-owner` with the bot stopped
- **THEN** the link file is gone, and the log says the link was removed with that command

#### Scenario: Another Discord account
- **WHEN** account A is linked and the user links account B with a new code
- **THEN** B is the owner, A is not, and the log says the owner changed

### Requirement: A different assistant account ends the link
Whenever the app reads the selected engine's signed-in account (a join preparation, Check,
Link or the read at start) and the link was recorded with the same engine and a different
account, the app SHALL remove the link and show the reason in the window. A read that finds
no signed-in account SHALL keep the link. Removing the link SHALL NOT refuse or end a join.

#### Scenario: Codex signed in with another ChatGPT account
- **WHEN** the owner was linked while Codex was signed in as account X, the mode is on, Codex is now signed in as account Y, and someone runs `/join`
- **THEN** the bot joins, the link is removed, no member's voice reaches the assistant, and the window shows "not linked" with the reason that the Codex account changed

#### Scenario: Check finds another account
- **WHEN** an owner is linked with account X and the user presses Check while Codex is signed in as account Y
- **THEN** the window shows "not linked" with the reason that the Codex account changed

#### Scenario: Claude signed in with another account
- **WHEN** the owner was linked while Claude Code was signed in as account X and Check finds Claude Code signed in as account Y
- **THEN** the window shows "not linked" with the reason that the Claude account changed

#### Scenario: Signed out
- **WHEN** an owner is linked and Codex reports no signed-in account
- **THEN** the link stays

### Requirement: Only the owner's voice reaches the assistant
While the mode is on and an owner is linked, the app SHALL pass to the wake word, the
speech detection and the assistant only the audio of sources that belong to the owner's
account. Every other source, bots included, and every source with no known account SHALL be
dropped before the mix, counted, and logged once per source.

#### Scenario: Another member says the phrase
- **WHEN** a member who is not the owner says the activation phrase
- **THEN** no session starts and the counter of ignored frames grows

#### Scenario: Two people at once
- **WHEN** the owner asks a question during a session while another member talks over them
- **THEN** the assistant receives only the owner's audio

#### Scenario: A music bot
- **WHEN** a music bot plays in the channel during a session
- **THEN** its audio does not reach the assistant

### Requirement: An unknown source with the owner alone
A source with no known account SHALL be taken as the owner's only when the owner is the only
non-bot member in the bot's voice channel.

#### Scenario: Owner alone
- **WHEN** audio arrives from a source Discord never announced and the owner is the only person in the channel
- **THEN** the audio is treated as the owner's

#### Scenario: Owner not alone
- **WHEN** audio arrives from a source Discord never announced and the owner and another member are in the channel
- **THEN** the audio is dropped

### Requirement: Mode and link changes apply at once
Changing the access mode, linking and unlinking SHALL take effect from the next 20 ms
frame in a channel the bot is already in, without a rejoin. While the owner-only mode is
on and no owner is linked, no member's audio SHALL reach the assistant.

#### Scenario: Unlink during a session
- **WHEN** the mode is on and the user presses Unlink while a session is active
- **THEN** no audio reaches the assistant from the next frame on

#### Scenario: Mode turned off in a channel
- **WHEN** the user changes the access mode from `owner` to `everyone` while the bot is in a voice channel
- **THEN** every member's audio reaches the wake word from the next frame on

#### Scenario: Owner mode to the agreed mode
- **WHEN** the user changes the access mode from `owner` to `agreed` while the bot is in a voice channel and one other member has a consent record
- **THEN** from the next frame on the audio of the owner and of that member reaches the wake word, and no one else's does

#### Scenario: Mode on, nothing linked
- **WHEN** the mode is on, no owner is linked, and a member runs `/join` and says the activation phrase
- **THEN** the bot joins the channel and no session starts

### Requirement: The commands stay open to everyone
The mode SHALL NOT limit who can use `/join`, `/leave` or `/status`. They SHALL work for
every member as without the mode. The panel's buttons, Mute assistant included, SHALL be
pressed by the linked owner only, whether this mode is on or off, as the
`discord-panel-controls` capability describes.

#### Scenario: Another member runs `/join`
- **WHEN** the mode is on and a member who is not the owner runs `/join` from a voice channel
- **THEN** the bot joins that channel, and only the owner's voice reaches the assistant there

#### Scenario: Another member presses Mute
- **WHEN** the mode is on and a member who is not the owner presses the panel's Mute button
- **THEN** the mute state does not change, and only that member sees the reply that the panel's buttons are the owner's

#### Scenario: Another member runs `/leave`
- **WHEN** the mode is on and a member who is not the owner runs `/leave` while the bot is in a voice channel of that server
- **THEN** the bot leaves the channel as for the owner

### Requirement: `/status` names the mode
While the mode is on, the `/status` reply SHALL add that only the owner's voice reaches the
assistant.

#### Scenario: Status from another member
- **WHEN** a member who is not the owner runs `/status` while the mode is on
- **THEN** they get the usual status and a line saying that only the owner's voice reaches the assistant

### Requirement: The mode does not limit the answers
The mode SHALL change nothing about who hears the assistant's voice or sees its posted
answers.

#### Scenario: Answers in the channel
- **WHEN** the owner asks a question that the agent answers while answer posting is on
- **THEN** every member in the channel hears the voice and the answer is posted as without the mode

### Requirement: An engine change keeps the link
Changing `assistant.engine` SHALL keep the owner link. The first account the new engine
reads afterwards SHALL be recorded with the link in place of the old engine's, and from
then on requirement "A different assistant account ends the link" compares against it.

#### Scenario: Codex to Claude Code
- **WHEN** an owner was linked with Codex, the user picks Claude Code and presses Check with Claude Code signed in
- **THEN** the owner stays linked and the link now records the Claude account

#### Scenario: To OpenCode
- **WHEN** an owner is linked and the user picks OpenCode
- **THEN** the owner stays linked and no account is compared while OpenCode is the engine

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
