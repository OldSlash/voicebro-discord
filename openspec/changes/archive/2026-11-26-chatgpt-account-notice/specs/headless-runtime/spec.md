## MODIFIED Requirements

### Requirement: Headless engine guidance
Every message that sends the desktop user to a window control SHALL have a terminal variant in headless: Codex, Claude Code or OpenCode not found or not signed in, missing files, and a log line that asks to link the owner again, whose terminal variant names `voicebro-server --link-owner`. The terminal variant SHALL name the setting in `settings.json` or the terminal command to use. At startup with Claude Code as the engine, a Claude subscription or an unknown sign-in kind, and an access mode other than `owner`, headless SHALL print the subscription notice once; the notice SHALL NOT refuse a join or change a setting. At startup with Codex as the engine and an access mode other than `owner`, headless SHALL print the ChatGPT account notice once without starting Codex; it SHALL NOT refuse a join or change a setting.

#### Scenario: Claude Code missing at join
- **WHEN** a member runs `/join` on the headless app with Claude Code as the engine and no Claude Code is found
- **THEN** the bot stays out of the channel and the reply says to install Claude Code or set `claude.executable` in `settings.json`, then run `/join` again

#### Scenario: Subscription notice at start
- **WHEN** headless starts with Claude Code signed in with a Claude subscription and `discord.access` set to `everyone`
- **THEN** the terminal shows that a Claude subscription may be used only by its holder and how to let only the holder's voice through, and the bot starts normally

#### Scenario: ChatGPT account notice at start
- **WHEN** headless starts with Codex as the engine and `discord.access` set to `everyone`
- **THEN** the terminal shows once that a ChatGPT account may be used only by its holder, that ChatGPT's voice is for non-commercial use and may not be recorded and published, and how to let only the holder's voice through with `discord.access` and `--link-owner`, with the links to OpenAI's terms; no Codex process starts for it, and the bot starts normally

#### Scenario: Owner only at start
- **WHEN** headless starts with Codex as the engine and `discord.access` set to `owner`
- **THEN** no ChatGPT account notice is printed
