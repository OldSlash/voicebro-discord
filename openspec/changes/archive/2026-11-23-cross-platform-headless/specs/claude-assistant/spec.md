# Spec Delta

## MODIFIED Requirements

### Requirement: Finding Claude Code
The app SHALL use `claude.executable` when it is set, and only that file. Otherwise, on Windows it SHALL look for `claude.exe` on the PATH, then in winget's links folder (`%LOCALAPPDATA%\Microsoft\WinGet\Links`), then in `%USERPROFILE%\.local\bin`, where Claude Code's own installer puts it. On Linux (not tested) it SHALL look for `claude` on `PATH`, with no Windows fallback. The app SHALL run the file as found and never change it.

#### Scenario: Installed by Claude Code's own installer
- **WHEN** `claude.executable` is empty, `claude.exe` is not on the PATH and `%USERPROFILE%\.local\bin\claude.exe` exists
- **THEN** the Status tab shows that file's version and the engine uses it

#### Scenario: Configured path missing
- **WHEN** `claude.executable` names a file that does not exist
- **THEN** the Status tab says Claude Code is not found at that path, and no other place is searched

#### Scenario: Linux executable on PATH (not tested)
- **WHEN** `claude.executable` is empty on Linux (not tested) and an executable named `claude` is on `PATH`
- **THEN** the engine uses that executable

### Requirement: Join prepares Claude Code
On `/join` with Claude Code as the engine, the app SHALL read the sign-in and start Claude Code for the guild before the bot enters the channel. When Claude Code is missing, not signed in or fails to start, the bot SHALL not join and SHALL reply with the reason and what to do: on the desktop app through the Status tab, on the headless app through the terminal.

#### Scenario: Not installed
- **WHEN** a member runs `/join` on the desktop app with Claude Code as the engine and no Claude Code is found
- **THEN** the bot stays out of the channel and the reply says to press Install Claude Code on the app's Status tab, then run `/join` again

#### Scenario: Not signed in
- **WHEN** a member runs `/join` on the desktop app and Claude Code reports no sign-in
- **THEN** the bot stays out of the channel and the reply says to press Sign in for Claude Code on the Status tab, then run `/join` again

#### Scenario: Headless prerequisites missing at join
- **WHEN** a member runs `/join` on the headless app with Claude Code as the engine and Claude Code is not found or not signed in
- **THEN** the bot stays out of the channel and the reply says to install Claude Code or set `claude.executable` in `settings.json`, or to sign in to Claude Code from a terminal as the user that runs VoiceBro, as appropriate, then run `/join` again

### Requirement: The Claude subscription notice
While the engine is Claude Code, the sign-in is a Claude subscription or not known yet, the access mode is not `owner`, and the user has not hidden the notice, the window SHALL show a notice that a Claude subscription may be used only by its holder, with links to Anthropic's Consumer Terms and Claude Code's legal page, and how to let only the holder's voice through. With an API key or a cloud provider it SHALL show none. The headless app, which has no window, SHALL print the same notice once at start under the same conditions, except the Hide choice, which exists only in the window.

#### Scenario: Subscription, everyone heard
- **WHEN** the engine is Claude Code, Claude Code is signed in with a Pro plan and the access mode is "Everyone"
- **THEN** the Claude Code card shows the notice, and the Assistant and Access cards show none

#### Scenario: Owner only
- **WHEN** the engine is Claude Code with a subscription and the access mode is "Only my Discord account"
- **THEN** no notice is shown

#### Scenario: API key
- **WHEN** the engine is Claude Code signed in with an API key and the access mode is "Everyone"
- **THEN** no notice is shown

#### Scenario: Hidden
- **WHEN** the user pressed Hide in the notice before, and the engine is Claude Code with a subscription and the access mode is "Everyone"
- **THEN** no notice is shown, also after a restart

#### Scenario: Headless start with a subscription
- **WHEN** the headless app starts with Claude Code signed in with a Claude subscription and the access mode is "Everyone"
- **THEN** the terminal shows the notice with the links once, and the bot starts normally
