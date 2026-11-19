# claude-assistant Specification

## Purpose
Runs Claude Code, Anthropic's command-line agent, as the assistant engine of the Whisper
mode: the user's own installation and sign-in, headless and locked down to web search, with
a notice that a Claude subscription is for its holder alone.

## Requirements

### Requirement: Finding Claude Code
The app SHALL use `claude.executable` when it is set, and only that file. Otherwise it SHALL
look for `claude.exe` on the PATH, then in winget's links folder
(`%LOCALAPPDATA%\Microsoft\WinGet\Links`), then in `%USERPROFILE%\.local\bin`, where
Claude Code's own installer puts it. The app SHALL run the file as found and never change
it.

#### Scenario: Installed by Claude Code's own installer
- **WHEN** `claude.executable` is empty, `claude.exe` is not on the PATH and `%USERPROFILE%\.local\bin\claude.exe` exists
- **THEN** the Status tab shows that file's version and the engine uses it

#### Scenario: Configured path missing
- **WHEN** `claude.executable` names a file that does not exist
- **THEN** the Status tab says Claude Code is not found at that path, and no other place is searched

### Requirement: The sign-in kind is known
The app SHALL read Claude Code's sign-in without a model call, at Check, at a join's
preparation and when Claude Code is chosen in the window, and SHALL tell apart: a Claude
subscription signed in through claude.ai (with its plan), an API key, a cloud provider
(Bedrock, Vertex, Foundry) and no sign-in. The read SHALL use the same environment as the
turns, so an API key in the environment shows as the key.

#### Scenario: Max subscription
- **WHEN** Claude Code is signed in through claude.ai with a Max plan and the user presses Check
- **THEN** the Status tab shows "Claude subscription (Max)"

#### Scenario: Environment key wins
- **WHEN** Claude Code is signed in with a subscription and `ANTHROPIC_API_KEY` is set in the environment the app runs in
- **THEN** the Status tab shows "API key" and the turns use the key

### Requirement: The app never handles Claude credentials
The app SHALL NOT read, store, pass or display Claude credentials or session tokens.
Signing in SHALL happen only through Claude Code's own sign-in, run in a console window the
user sees.

#### Scenario: Sign in pressed
- **WHEN** the user presses Sign in for Claude Code
- **THEN** a console window opens running Claude Code's own sign-in, and the app reads only the sign-in kind afterwards

### Requirement: Claude Code runs locked down
Claude Code SHALL run headless with only its web search and web fetch tools. It SHALL NOT
load the user's Claude Code settings, CLAUDE.md files, hooks or MCP servers, SHALL work in
an empty folder inside the app's folder, and SHALL write no conversation to disk.

#### Scenario: No local tools
- **WHEN** a member asks Claude to list the files on the PC
- **THEN** Claude has no tool that reads, writes or runs anything on the PC, and no file is touched

#### Scenario: Personal instructions not loaded
- **WHEN** the Windows user has a personal CLAUDE.md and a member asks a question
- **THEN** the answer is made without that file's content

### Requirement: Join prepares Claude Code
On `/join` with Claude Code as the engine, the app SHALL read the sign-in and start Claude
Code for the guild before the bot enters the channel. When Claude Code is missing, not
signed in or fails to start, the bot SHALL not join and SHALL reply with the reason and
what to do.

#### Scenario: Not installed
- **WHEN** a member runs `/join` with Claude Code as the engine and no Claude Code is found
- **THEN** the bot stays out of the channel and the reply says to press Install Claude Code on the app's Status tab, then run `/join` again

#### Scenario: Not signed in
- **WHEN** a member runs `/join` and Claude Code reports no sign-in
- **THEN** the bot stays out of the channel and the reply says to press Sign in for Claude Code on the Status tab, then run `/join` again

### Requirement: Claude Code stops after the last leave
Claude Code SHALL run only while the bot prepares a join or is in a voice channel with
Claude Code as the engine. Its processes SHALL stop 0.5 s after the last leave, at an
engine switch and at app exit; the guild's conversation SHALL stay in memory for the next
join.

#### Scenario: Last leave
- **WHEN** the bot leaves its only voice channel
- **THEN** no Claude Code process of the app runs a second later

### Requirement: Questions are Claude Code turns
Each question SHALL be sent to the guild's Claude Code conversation as a text turn, prefixed
with the speaker's name while speaker names are on. The answer's text SHALL be spoken as it
streams. A completed turn SHALL be an answer for posting; a failed or interrupted turn SHALL
add an error to the chat and post nothing.

#### Scenario: Spoken while streaming
- **WHEN** Alex asks "what time is it in London" with Claude Code as the engine
- **THEN** Claude Code gets "Alex: what time is it in London" and the voice starts on the first sentence before the whole answer has arrived

### Requirement: Barge-in interrupts Claude Code
While one speaker at a time is off, a question that comes while Claude Code answers, and
the end of a session, SHALL interrupt the running turn and flush the voice; the new
question SHALL then start its own turn.

#### Scenario: New question
- **WHEN** one speaker at a time is off and a member asks a new question while Claude Code still answers the first
- **THEN** the first turn is interrupted, its voice stops, and the new question is answered

### Requirement: Claude Code questions wait their turn
While one speaker at a time is on, a question that comes while Claude Code works SHALL NOT
interrupt it. Claude Code cannot add a question to a running turn, so the question SHALL be
answered in the next turn, right after the running one, and the answers SHALL be spoken in
order.

#### Scenario: Second question while Claude works
- **WHEN** one speaker at a time is on, Alex asks "What time is it in Bangkok?", and Sam asks "And what is the weather in London?" while Claude Code is still answering Alex
- **THEN** Alex's answer is spoken to its end and Sam's answer follows it

### Requirement: Claude Code context and history
The first question of each session SHALL carry the people in the channel and the caller as
a facts line, unless speaker names are off. A new Claude Code conversation SHALL get the
guild's earlier exchanges, at most 24 KB, with its first question. The user's
`assistant.instructions` SHALL be added to Claude Code's own system prompt, and the app
SHALL add no instruction of its own.

#### Scenario: Who is in the channel
- **WHEN** the first question of a session is "who is in the voice channel?"
- **THEN** the answer names the people the app sent

#### Scenario: Rejoin keeps the conversation
- **WHEN** a member asks Claude Code to remember 42, the bot leaves and joins again, and a member asks which number it was
- **THEN** the answer is 42

### Requirement: Claude Code model
`claude.model` SHALL be empty, meaning Claude Code's default for the sign-in, or a model
name or alias Claude Code accepts. Check SHALL read the models Claude Code offers for the
sign-in without a model call, and the model field SHALL offer them.

#### Scenario: Models listed
- **WHEN** the user presses Check with Claude Code signed in
- **THEN** the Claude Code card's model field offers the models Claude Code reported, and no question is sent to any model

### Requirement: The Claude subscription notice
While the engine is Claude Code, the sign-in is a Claude subscription or not known yet, and
the access mode is not `owner`, the window SHALL show a notice that a Claude subscription
may be used only by its holder, with links to Anthropic's Consumer Terms and Claude Code's
legal page, and how to let only the holder's voice through. With an API key or a cloud
provider it SHALL show none.

#### Scenario: Subscription, everyone heard
- **WHEN** the engine is Claude Code, Claude Code is signed in with a Pro plan and the access mode is "Everyone"
- **THEN** the Assistant and Access cards show the notice

#### Scenario: Owner only
- **WHEN** the engine is Claude Code with a subscription and the access mode is "Only my Discord account"
- **THEN** no notice is shown

#### Scenario: API key
- **WHEN** the engine is Claude Code signed in with an API key and the access mode is "Everyone"
- **THEN** no notice is shown

### Requirement: The notice advises and enforces nothing
The subscription notice and its dialog SHALL NOT refuse a join, end a session or change the
access mode by themselves.

#### Scenario: Join with the notice shown
- **WHEN** the notice is shown and a member runs `/join`
- **THEN** the bot joins as it would without the notice
