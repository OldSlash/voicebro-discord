# opencode-assistant Specification

## Purpose
Runs OpenCode, an open-source coding agent that works with many AI providers, as the
assistant engine of the Whisper mode: the user's own installation, providers and keys,
behind a local server the app starts, locked down to fetching pages and searching the web.

## Requirements

### Requirement: Finding OpenCode
The app SHALL use `opencode.executable` when it is set, and only that file. Otherwise, on Windows it SHALL look for `opencode.exe` on the PATH, then in winget's links folder (`%LOCALAPPDATA%\Microsoft\WinGet\Links`), then for the native executable inside npm's global `opencode-ai` package, and it SHALL run the native executable, never a script shim. On Linux (not tested) it SHALL look for `opencode` on `PATH`, with no Windows fallback; it MAY be a launcher script, because the process group it runs in is stopped as a whole.

#### Scenario: Installed with npm
- **WHEN** `opencode.executable` is empty and OpenCode was installed with `npm install -g opencode-ai` on Windows
- **THEN** the app runs the native `opencode.exe` from that package and the Status tab shows its version

#### Scenario: Linux executable on PATH (not tested)
- **WHEN** `opencode.executable` is empty on Linux (not tested) and an executable named `opencode` is on `PATH`
- **THEN** the engine runs it, and stopping OpenCode leaves none of its processes running

### Requirement: The app never handles provider keys
The app SHALL NOT read, store or display the keys or sign-ins OpenCode keeps for its
providers. The user SHALL set up providers in OpenCode itself; the app SHALL only list the
providers OpenCode reports and the model in use.

#### Scenario: Providers shown
- **WHEN** OpenCode has an OpenAI sign-in and an Anthropic key and the user presses Check
- **THEN** the Status tab names the two providers and shows no key or token

### Requirement: The local server is private
The app SHALL start OpenCode's server listening only on 127.0.0.1, on a free port, with a
password made for that run and never written to disk or the log.

#### Scenario: Another program on the PC
- **WHEN** another program on the PC calls the server's port without the password
- **THEN** the server refuses it

### Requirement: OpenCode runs locked down
OpenCode SHALL run with a configuration the app writes into its own folder: one agent that
may only fetch web pages and search the web, whatever the user's OpenCode configuration
allows, so that no skill or other tool can be used. It SHALL NOT load other agents'
instruction files (such as `~\.claude\CLAUDE.md`), and SHALL work in an empty folder inside
the app's folder.

#### Scenario: No local tools
- **WHEN** a member asks OpenCode to run a command on the PC
- **THEN** OpenCode has no tool that reads, writes or runs anything on the PC, and nothing is run

#### Scenario: Claude Code files not loaded
- **WHEN** the Windows user has `~\.claude\CLAUDE.md` and a member asks a question
- **THEN** the request OpenCode sends to the model holds none of that file's content

### Requirement: OpenCode searches the web
OpenCode's web search tool SHALL be on. Its queries go to the search service OpenCode uses,
which is a company other than the model's provider; the README, the user guide, the
consent text and the privacy policy template SHALL say so.

#### Scenario: A question that needs the web
- **WHEN** a member asks OpenCode about today's weather in London and a search service is available to OpenCode
- **THEN** OpenCode searches the web and the chat shows the search as a tool entry

#### Scenario: No search service
- **WHEN** no search service is available to OpenCode and a member asks about today's news
- **THEN** the answer still comes, and the log says that OpenCode's web search failed and why

### Requirement: Join prepares OpenCode
On `/join` with OpenCode as the engine, the app SHALL start OpenCode's server and check that OpenCode offers the model in use (`opencode.model`, or OpenCode's default when it is empty), before the bot enters the channel. When OpenCode is missing, fails to start or offers no such model, the bot SHALL not join and SHALL reply with the reason and what to do: on the desktop app through its window, on the headless app through the terminal and `settings.json`.

#### Scenario: No usable model
- **WHEN** a member runs `/join` on the desktop app with OpenCode as the engine and OpenCode offers no model, or not the one `opencode.model` names
- **THEN** the bot stays out of the channel and the reply says to set up a provider in OpenCode (`opencode auth login` in a terminal) or to pick a model it offers in the OpenCode card, then run `/join` again

#### Scenario: No usable model on the headless app
- **WHEN** a member runs `/join` on the headless app with OpenCode as the engine and OpenCode offers no model, or not the one `opencode.model` names
- **THEN** the bot stays out of the channel and the reply says to set up a provider in OpenCode (`opencode auth login` in a terminal, as the user that runs VoiceBro) or to set a model it offers in `opencode.model` in `settings.json`, then run `/join` again

#### Scenario: OpenCode missing on the headless app
- **WHEN** a member runs `/join` on the headless app with OpenCode as the engine and no OpenCode is found
- **THEN** the bot stays out of the channel and the reply says to install OpenCode or set `opencode.executable` in `settings.json`, then run `/join` again

#### Scenario: OpenCode's own free models
- **WHEN** OpenCode has no provider set up, offers its own free models, and `opencode.model` is empty
- **THEN** the bot joins, and the consent text says the speech goes to the provider set up in OpenCode or, for its free models, to the OpenCode project's own service

### Requirement: OpenCode stops after the last leave
OpenCode's server SHALL run only while the bot prepares a join or is in a voice channel
with OpenCode as the engine. The app SHALL delete the sessions it made when the bot leaves
the guild's channel. The server SHALL stop 0.5 s after the last leave, at an engine switch
and at app exit; the guild's conversation SHALL stay in the app's memory for the next join.

#### Scenario: Leave
- **WHEN** the bot leaves its only voice channel after two questions
- **THEN** OpenCode holds no session the app made, and no OpenCode process of the app runs a second later

### Requirement: Questions are OpenCode turns
Each question SHALL be sent to the guild's OpenCode session, prefixed with the speaker's
name while speaker names are on. The answer's text SHALL be spoken as it streams. A
completed answer SHALL be an answer for posting; a failed or aborted one SHALL add an error
to the chat and post nothing.

#### Scenario: Spoken while streaming
- **WHEN** Alex asks a question with OpenCode as the engine
- **THEN** OpenCode gets "Alex: " followed by the question, and the voice starts on the first sentence before the whole answer has arrived

### Requirement: Barge-in aborts OpenCode
While one speaker at a time is off, a question that comes while OpenCode answers, and the
end of a session, SHALL abort the running answer and flush the voice; the new question
SHALL then be sent.

#### Scenario: New question
- **WHEN** one speaker at a time is off and a member asks a new question while OpenCode still answers the first
- **THEN** the first answer is aborted, its voice stops, and the new question is answered

### Requirement: OpenCode questions wait their turn
While one speaker at a time is on, a question that comes while OpenCode works SHALL NOT
abort it. It SHALL be handed to the running session, which takes it up at its next step;
the answers SHALL be spoken in the order they come.

#### Scenario: Second question while OpenCode works
- **WHEN** one speaker at a time is on and Sam asks a question while OpenCode is still working on Alex's
- **THEN** Alex's answer is not aborted, and Sam's question is answered after it

### Requirement: OpenCode context and history
The first question of each session SHALL carry the people in the channel and the caller as
a facts line, unless speaker names are off. A new OpenCode session SHALL get the guild's
earlier exchanges, at most 24 KB, with its first question. The agent's instructions SHALL
be the user's `assistant.instructions`; the app SHALL add none of its own.

#### Scenario: Rejoin keeps the conversation
- **WHEN** a member asks OpenCode to remember 42, the bot leaves and joins again, and a member asks which number it was
- **THEN** the answer is 42

### Requirement: OpenCode model
`opencode.model` SHALL be empty, meaning OpenCode's own default model, or a
`provider/model` id OpenCode knows. Check SHALL read the models OpenCode offers and the
model field SHALL offer them.

#### Scenario: Models listed
- **WHEN** the user presses Check with OpenCode installed
- **THEN** the OpenCode card's model field offers the models OpenCode listed, as `provider/model`
