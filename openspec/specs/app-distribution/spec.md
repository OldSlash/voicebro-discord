# app-distribution Specification

## Purpose

Defines how VoiceBro reaches a user and leaves their machine: what a release contains, how
it is installed, updated and removed, and where the installed app keeps its files.

## Requirements

### Requirement: Installer for the current user
The installer SHALL install VoiceBro for the current user without administrator rights,
into `%LOCALAPPDATA%\VoiceBro` by default with the folder selectable, SHALL add a Start menu
entry, SHALL offer a desktop shortcut on its last page, and SHALL show its pages in Russian
on a Russian system and in English otherwise.

#### Scenario: Default install
- **WHEN** the user runs the installer and accepts the defaults
- **THEN** no administrator prompt appears, `voicebro.exe` is in `%LOCALAPPDATA%\VoiceBro`, and the Start menu has a VoiceBro entry

#### Scenario: Chosen folder
- **WHEN** the user installs into `D:\Games\VoiceBro` and starts the app
- **THEN** the app's settings, logs and downloads are created in `D:\Games\VoiceBro`

### Requirement: Updates keep the data
Installing a newer version over an installed one SHALL replace the program files and SHALL
keep `settings.json`, `logs\`, `assets\`, `models\`, `codex-workspace\`, `webview-data\`
and the stored Discord token.

#### Scenario: Update
- **WHEN** version 1.0.1 is installed over 1.0.0 with phrases set and models downloaded
- **THEN** the app starts with the same settings, models and token and shows 1.0.1

### Requirement: Uninstall removes the data only when asked
The uninstaller SHALL remove the program files and shortcuts. With "Delete the application
data" checked it SHALL also remove `settings.json`, `logs\`, `assets\`, `models\`,
`codex-workspace\`, `webview-data\`, the stored Discord token and the then empty folder;
unchecked, it SHALL leave them.

#### Scenario: Data kept
- **WHEN** the user uninstalls without checking the box
- **THEN** the folder still holds the settings, logs and models, and a reinstall into it starts with them

#### Scenario: Data deleted
- **WHEN** the user uninstalls with the box checked
- **THEN** the folder is gone and the Credential Manager has no `VoiceBro/discord_token`

### Requirement: Portable zip
Each release SHALL include a zip holding `voicebro.exe`, `README.md`, `LICENSE` and
`THIRD-PARTY-NOTICES.txt`. Extracted into any folder the user can write to, the app SHALL
run from there and keep its files next to `voicebro.exe`, without an installer.

#### Scenario: Run from a folder
- **WHEN** the user extracts the zip into `D:\Tools\VoiceBro` and starts `voicebro.exe`
- **THEN** `settings.json` and `logs\` appear in `D:\Tools\VoiceBro` and nothing is registered with Windows

### Requirement: The app's folder must be writable
At start, before anything else, the app SHALL check that it can write to its folder. When
it cannot, it SHALL show a message that names the folder and says to install or extract
VoiceBro into a folder the user can write to, and SHALL exit without creating anything.

#### Scenario: Started from a protected folder
- **WHEN** `voicebro.exe` is started from a folder the user cannot write to, such as one under Program Files
- **THEN** a message names the folder and what to do, the app exits, and no window, log or settings file is created

### Requirement: Files stay in the app's folder
The app SHALL create files only inside its own folder. Outside it, only the stored Discord
token in the Windows Credential Manager SHALL be written by the app; the user's Codex keeps
its own data in `~\.codex`, and Codex installed through winget lives where winget puts it.

#### Scenario: A run leaves nothing outside
- **WHEN** the app runs for a minute, connects to Discord, downloads the wake model after the OK and is closed
- **THEN** no file or folder created by the app is found in `%TEMP%`, `%APPDATA%` or `%LOCALAPPDATA%` outside the app's folder

### Requirement: Release artifacts
A release SHALL be built from `main` at the tag `v<version>`, where the version is the one
the app shows. It SHALL consist of the installer, the portable zip and `SHA256SUMS.txt`
with the SHA-256 of both. The release build SHALL refuse an untagged commit, a dirty tree
or a version mismatch; a trial build SHALL be marked as such and SHALL never be handed over.

#### Scenario: Tagged build
- **WHEN** the release script runs on `main` at `v1.0.0` with a clean tree and the app's version 1.0.0
- **THEN** the installer, the zip and `SHA256SUMS.txt` are written, and the checksums match the files

#### Scenario: Untagged build
- **WHEN** the release script runs on a commit without a matching tag
- **THEN** it stops with a message and writes no release artifact

### Requirement: Third-party notices
The installer and the zip SHALL include `THIRD-PARTY-NOTICES.txt` with the name, version,
license and license text of every crate built into `voicebro.exe`, and SHALL name Vosk and
its models as components the app downloads with the user's OK, with their licenses.

#### Scenario: Notices complete
- **WHEN** the notices file of a release is compared with the crates of the release build
- **THEN** every crate in the build appears with its license text, or with its license name and source link when it ships no license file

### Requirement: User guide
The repository and the zip SHALL include `README.md`, which SHALL take a new user from
nothing to a first answer: what VoiceBro needs, creating and inviting the Discord bot,
installing Codex and signing in to ChatGPT, installing VoiceBro and the SmartScreen warning,
the first settings, wake phrases per language and other languages, daily use, the
diagnostic switches, where the files live, updating, uninstalling and troubleshooting.

#### Scenario: A new user follows the guide
- **WHEN** a person without Codex, a Discord bot or VoiceBro follows the README step by step
- **THEN** they reach a spoken answer in their voice channel without needing anything the README does not explain
