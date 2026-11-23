# Spec Delta

## MODIFIED Requirements

### Requirement: Finding OpenCode
The app SHALL use `opencode.executable` when it is set, and only that file. Otherwise, on Windows it SHALL look for `opencode.exe` on the PATH, then in winget's links folder (`%LOCALAPPDATA%\Microsoft\WinGet\Links`), then for the native executable inside npm's global `opencode-ai` package, and it SHALL run the native executable, never a script shim. On Linux (not tested) it SHALL look for `opencode` on `PATH`, with no Windows fallback; it MAY be a launcher script, because the process group it runs in is stopped as a whole.

#### Scenario: Installed with npm
- **WHEN** `opencode.executable` is empty and OpenCode was installed with `npm install -g opencode-ai` on Windows
- **THEN** the app runs the native `opencode.exe` from that package and the Status tab shows its version

#### Scenario: Linux executable on PATH (not tested)
- **WHEN** `opencode.executable` is empty on Linux (not tested) and an executable named `opencode` is on `PATH`
- **THEN** the engine runs it, and stopping OpenCode leaves none of its processes running

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
