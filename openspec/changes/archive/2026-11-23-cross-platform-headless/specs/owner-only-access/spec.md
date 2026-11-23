# Spec Delta

## RENAMED Requirements

- FROM: `### Requirement: The code stays in the window`
- TO: `### Requirement: The code stays in the window or the terminal`

## MODIFIED Requirements

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

### Requirement: Linking with a one-time code
Link in the window, or `voicebro-server --link-owner` on the headless app, SHALL read the selected engine's signed-in account (the ChatGPT account for Codex, the Claude account for Claude Code; OpenCode has none) and then show a code of 8 characters without look-alike characters, valid for 10 minutes: in the window on the desktop app, in the terminal on the headless app. A `/link` with that code SHALL make the Discord account that sent it the owner, record the engine and the account read for the code (none for OpenCode), and reply only to the sender that the account is linked.

#### Scenario: Successful link
- **WHEN** Codex is signed in, the user presses Link, and types `/link` with the shown code from their Discord account within 10 minutes
- **THEN** the window shows that account's name as the owner, the code disappears, and only the sender sees the reply that the account is linked

#### Scenario: Successful link from the terminal
- **WHEN** Codex is signed in, the operator runs `voicebro-server --link-owner`, and types `/link` with the printed code from their Discord account within 10 minutes
- **THEN** the terminal shows that account's name as the owner, and only the sender sees the reply that the account is linked

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

### Requirement: The code stays in the window or the terminal
The app SHALL show the code only in its window, and the headless app only on the terminal's standard output. It SHALL NOT write the code to the log file, the stderr log lines, the settings file or any Discord message.

#### Scenario: Log after linking
- **WHEN** a link is made with the verbose log on
- **THEN** the log says a code was issued and that an account was linked, and holds neither the code nor the typed value

#### Scenario: Headless log after linking
- **WHEN** a link is made through `voicebro-server --link-owner` with the verbose log on
- **THEN** the code appears once on standard output, and neither the log file nor the stderr lines hold the code or the typed value

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
