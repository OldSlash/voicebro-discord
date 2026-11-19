## MODIFIED Requirements

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

## ADDED Requirements

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
