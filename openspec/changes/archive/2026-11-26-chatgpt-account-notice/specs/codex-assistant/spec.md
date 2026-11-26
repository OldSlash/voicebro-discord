## ADDED Requirements

### Requirement: The ChatGPT account notice
While the engine is Codex, Codex's sign-in is a ChatGPT account of any plan or not known yet, the access mode is not `owner`, and `codex.account_notice_hidden` is `false`, the window SHALL show a notice that a ChatGPT account may be used only by its holder, that ChatGPT's voice is for non-commercial use and may not be recorded and published, and how to let only the holder's voice through. An API key, Amazon Bedrock or Google Cloud sign-in SHALL show none.

#### Scenario: ChatGPT account, everyone heard
- **WHEN** the engine is Codex, Codex is signed in with a ChatGPT Plus account and the access mode is "Everyone"
- **THEN** the notice is shown

#### Scenario: Sign-in not read yet
- **WHEN** the app has just started with Codex as the engine, Codex has not run yet and the access mode is "People who agreed"
- **THEN** the notice is shown

#### Scenario: Owner only
- **WHEN** the engine is Codex with a ChatGPT account and the access mode is "Only my Discord account"
- **THEN** no notice is shown

#### Scenario: API key
- **WHEN** the engine is Codex, Codex is signed in with an OpenAI API key and the access mode is "Everyone"
- **THEN** no notice is shown

#### Scenario: Hidden
- **WHEN** `codex.account_notice_hidden` is `true`, the engine is Codex with a ChatGPT account and the access mode is "Everyone"
- **THEN** no notice is shown, also after a restart

#### Scenario: Another engine
- **WHEN** the engine is Claude Code or OpenCode
- **THEN** the ChatGPT account notice is not shown

### Requirement: The ChatGPT account notice advises and enforces nothing
The ChatGPT account notice SHALL NOT refuse a join, end a session, start Codex or change a setting by itself; only its buttons change settings, and only when pressed.

#### Scenario: Join with the notice shown
- **WHEN** the notice is shown and a member runs `/join`
- **THEN** the bot joins as it would without the notice

#### Scenario: The notice does not start Codex
- **WHEN** the window shows the notice before Codex has run
- **THEN** no Codex process is started for it
