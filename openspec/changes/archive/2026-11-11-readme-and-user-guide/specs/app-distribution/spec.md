# Spec Delta

## MODIFIED Requirements

### Requirement: Portable zip
Each release SHALL include a zip holding `voicebro.exe`, `README.md`,
`docs\user-guide.md` with the images the two documents show, `LICENSE` and
`THIRD-PARTY-NOTICES.txt`. Extracted into any folder the user can write to, the app SHALL
run from there and keep its files next to `voicebro.exe`, without an installer.

#### Scenario: Run from a folder
- **WHEN** the user extracts the zip into `D:\Tools\VoiceBro` and starts `voicebro.exe`
- **THEN** `settings.json` and `logs\` appear in `D:\Tools\VoiceBro` and nothing is registered with Windows

#### Scenario: Guide offline
- **WHEN** the user opens `README.md` from the extracted zip and follows its link to the user guide
- **THEN** `docs\user-guide.md` and the images both documents show are in the extracted folder

### Requirement: User guide
`README.md` SHALL say what VoiceBro does, what it needs, where to download it, a quick
start of at most five steps, how to build it from source and its license. `docs/user-guide.md`
SHALL take a new user from nothing to a first answer in detail: the Discord bot, installing,
connecting, Codex and the ChatGPT sign-in, wake phrases and other languages, daily use,
settings, privacy and files, updating, uninstalling and troubleshooting.

#### Scenario: A new user follows the guide
- **WHEN** a person without Codex, a Discord bot or VoiceBro follows the README's quick start and the user guide it links to
- **THEN** they reach a spoken answer in their voice channel without needing anything the two documents do not explain

#### Scenario: A developer builds from source
- **WHEN** a developer with Windows and none of the tools follows the README's Building from source section
- **THEN** they know which tools and versions to install, and the listed commands build, test and run the app
