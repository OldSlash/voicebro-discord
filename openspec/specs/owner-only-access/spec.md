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
Link in the window SHALL read the selected engine's signed-in account (the ChatGPT account
for Codex, the Claude account for Claude Code; OpenCode has none) and then show a code of 8
characters without look-alike characters, valid for 10 minutes. A `/link` with that code
SHALL make the Discord account that sent it the owner, record the engine and the account
read for the code (none for OpenCode), and reply only to the sender that the account is
linked.

#### Scenario: Successful link
- **WHEN** Codex is signed in, the user presses Link, and types `/link` with the shown code from their Discord account within 10 minutes
- **THEN** the window shows that account's name as the owner, the code disappears, and only the sender sees the reply that the account is linked

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
presses Cancel, or after 5 wrong codes in total. A wrong, ended or unknown code SHALL change
nothing and SHALL get a reply only the sender sees.

#### Scenario: Expired code
- **WHEN** the owner types the code 11 minutes after it was shown
- **THEN** no link is made, the sender is told the code expired, and the window shows that no code is waiting

#### Scenario: Wrong codes
- **WHEN** someone in the server sends 5 wrong codes while a code is waiting
- **THEN** the waiting code ends, the right code typed afterwards links nothing, and the window says the code was ended after 5 wrong tries

#### Scenario: Used code
- **WHEN** a second account sends the same code after it linked the first account
- **THEN** the first account stays the owner

### Requirement: Where `/link` works
`/link` SHALL work in every server the bot is in and in a direct message to the bot.

#### Scenario: Direct message
- **WHEN** the owner sends `/link` with the waiting code in a direct message to the bot
- **THEN** the account is linked as in a server

### Requirement: The code stays in the window
The app SHALL show the code only in its window. It SHALL NOT write the code to the log, the
settings file or any Discord message.

#### Scenario: Log after linking
- **WHEN** a link is made with the verbose log on
- **THEN** the log says a code was issued and that an account was linked, and holds neither the code nor the typed value

### Requirement: The link is stored encrypted in the app's folder
The app SHALL keep the link as one file in its own folder, encrypted so that only the same
Windows user on the same PC can read it. A file that cannot be decrypted or read as a link
SHALL count as no link and SHALL be reported in the log once at start.

#### Scenario: Restart
- **WHEN** an owner is linked and the app is closed and started again
- **THEN** the same owner is linked without a new code

#### Scenario: Edited file
- **WHEN** one byte of the link file is changed and the app starts
- **THEN** no owner is linked, the window shows "not linked", and the log names the file as unreadable

#### Scenario: Another Windows user
- **WHEN** the app's folder with the link file is copied to another Windows user's folder and started there
- **THEN** no owner is linked

### Requirement: The owner is set only by a code
The window and the settings file SHALL offer no way to set the owner's account other than a
code sent from that account.

#### Scenario: Owner id in the settings file
- **WHEN** `settings.json` is edited to hold a Discord account id under any name
- **THEN** the linked owner does not change

### Requirement: A link lasts until it is replaced or removed
A link SHALL have no expiry. It SHALL end only when the user presses Unlink, when another
Discord account completes a new code, which replaces it, when its file is deleted with the
app's folder or the app's data, or as the requirement on the assistant account says.

#### Scenario: Unlink
- **WHEN** the user presses Unlink
- **THEN** the link file is gone, the window shows "not linked", and the log says the link was removed

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
The mode SHALL NOT limit who can use `/join`, `/leave`, `/status` or the panel's Mute button.
They SHALL work for every member as without the mode.

#### Scenario: Another member runs `/join`
- **WHEN** the mode is on and a member who is not the owner runs `/join` from a voice channel
- **THEN** the bot joins that channel, and only the owner's voice reaches the assistant there

#### Scenario: Another member presses Mute
- **WHEN** the mode is on and a member who is not the owner presses the panel's Mute button
- **THEN** the app-wide mute toggles as without the mode

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
