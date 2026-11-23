# Spec Delta

## Purpose

Run the existing Discord voice bot from a native terminal without requiring the Windows desktop app. Linux support is added but not tested: every Linux statement below is marked (not tested) until the user checks it on Linux.

What VoiceBro keeps on disk about people and the bot (the Discord token, the owner link and the consent records) is encrypted on both platforms by default. On Windows this is always so. On Linux (not tested) the encryption is optional: the setting `linux.encrypt_files` can turn it off, and then those files are plain, protected only by file permissions. The docs and the privacy policy template say so, and the terminal warns at every start while it is off.

## ADDED Requirements

### Requirement: Native console startup
`voicebro-server` SHALL run from a Windows terminal or a glibc-based Linux x86-64 terminal (not tested) without Tauri, a desktop session or Docker. It SHALL connect to Discord using the existing bot runtime and report startup and connection errors in the terminal.

#### Scenario: Run the bot without a window
- **WHEN** an operator starts the binary with valid settings and a valid Discord token
- **THEN** the bot connects and handles its slash commands as the desktop runtime does, without opening a window

#### Scenario: Realtime voice session
- **WHEN** the bot joins in the realtime voice mode with Codex signed in and the configured wake assets installed, and an eligible speaker uses the activation phrase
- **THEN** the existing realtime voice flow sends the speaker's audio to Codex, plays the answer in Discord, and ends the session on the configured deactivation phrase

#### Scenario: Gateway rejects the connection
- **WHEN** Discord rejects the bot's connection
- **THEN** the terminal reports the connection failure without exposing the token, and the process exits with an error after its cleanup

### Requirement: Headless settings and supported modes
The binary SHALL use the existing `settings.json` schema and defaults in `VOICEBRO_DATA_DIR` or, when unset, beside the executable. It SHALL support every assistant mode (realtime voice, Whisper, text answers), every engine (Codex, Claude Code, OpenCode) and both session modes, with the same rules as the desktop app. It SHALL reject invalid settings before connecting, without rewriting them. On Linux (not tested) it SHALL also reject a Windows voice in the Whisper mode, since the Windows voices exist only on Windows.

#### Scenario: Restart with the same data directory
- **WHEN** the operator restarts with the same data directory after customizing wake phrases or other settings
- **THEN** the existing settings, the selected models, voice and phrases are used again

#### Scenario: Whisper mode with Claude Code
- **WHEN** the settings select the Whisper mode and Claude Code, its files are installed and Claude Code is signed in, and a member runs `/join`
- **THEN** the bot joins and answers spoken questions with Claude Code's text spoken by the local voice, as the desktop app does

#### Scenario: Text answers mode with OpenCode
- **WHEN** the settings select the text answers mode and OpenCode with a usable model, and a member asks a question after the activation phrase
- **THEN** the answer is posted in the Discord chat the settings name, and nothing is spoken

#### Scenario: Invalid settings
- **WHEN** `settings.json` holds a value the settings validation rejects
- **THEN** startup fails before connecting with a message naming the field, and the settings file is unchanged

#### Scenario: Windows voice on Linux (not tested)
- **WHEN** Linux headless (not tested) starts in the Whisper mode with a Windows voice selected
- **THEN** startup fails before connecting, says that Windows voices exist only on Windows and to pick another voice, and the settings file is unchanged

### Requirement: Headless access modes
Headless SHALL support every `discord.access` value (`everyone`, `agreed`, `owner`) on Windows and on Linux (not tested), with the same access rules, `/link`, `/optin`, `/optout` and `/privacy` as the desktop app. At start the terminal SHALL show the access mode, the linked owner's name or that no owner is linked, and how many people have a consent record, and it SHALL show each later change of the link or the records. Windows headless SHALL apply the records the desktop app keeps for the same Windows user and data directory.

#### Scenario: Existing Windows access records
- **WHEN** Windows headless starts with the same data directory and under the same Windows user as the desktop app
- **THEN** existing owner and consent records are applied under the same access rules

#### Scenario: Owner-only access on Linux (not tested)
- **WHEN** Linux headless (not tested) runs with `discord.access` set to `owner` and a linked owner
- **THEN** only the owner's voice reaches the wake word and the assistant, as on the desktop app

#### Scenario: Access at start
- **WHEN** headless starts with `discord.access` set to `agreed`, a linked owner and two consent records
- **THEN** the terminal shows the mode, the owner's name and that two people agreed

### Requirement: Owner link from the terminal
`voicebro-server --link-owner` SHALL start the bot as a normal start does, then read the selected engine's signed-in account as Link in the window does (OpenCode has none) and issue a one-time code under the rules of `owner-only-access`. It SHALL print the code only to the terminal's standard output, never to the log file, the stderr log lines, the settings file or a Discord message. Without a signed-in account it SHALL issue no code, say to sign in to that engine from a terminal first, and keep the bot running. The terminal SHALL report when the code is used and who was linked, when it expires and when it ends after five wrong tries; stopping the process SHALL end a waiting code.

#### Scenario: Link from the terminal
- **WHEN** the operator runs `voicebro-server --link-owner` with Codex signed in, and sends `/link` with the printed code from their Discord account within 10 minutes
- **THEN** that account becomes the owner, the terminal says so with the account's name, the log file holds neither the code nor the typed value, and the bot keeps running

#### Scenario: Engine not signed in
- **WHEN** the operator runs `voicebro-server --link-owner` with Claude Code as the engine and Claude Code reports no sign-in
- **THEN** no code is printed, the terminal says to sign in to Claude Code from a terminal first, and the bot keeps running

#### Scenario: Code expires
- **WHEN** nobody uses the printed code for 10 minutes
- **THEN** the terminal says the code expired, no owner changes, and the bot keeps running

### Requirement: Access records from the terminal
Headless SHALL offer the window's access controls as commands that connect to nothing and start no engine: `voicebro-server --unlink-owner` SHALL remove the owner link as Unlink does, `voicebro-server --list-consents` SHALL print each consent record's Discord user id, name and time, and `voicebro-server --remove-consent <Discord user id>` SHALL remove that record as Remove does, sending the person nothing. Each SHALL say what it changed, or that there was nothing to change. They act on the data directory's files and SHALL be documented to run while the bot is stopped.

#### Scenario: Unlink from the terminal
- **WHEN** an owner is linked and the operator runs `voicebro-server --unlink-owner` with the bot stopped
- **THEN** the link file is gone, the command names the owner it removed, and the next start shows that no owner is linked

#### Scenario: Nothing linked
- **WHEN** no owner is linked and the operator runs `voicebro-server --unlink-owner`
- **THEN** nothing changes and the command says no owner was linked

#### Scenario: Remove a consent record
- **WHEN** `voicebro-server --list-consents` shows a record with the id 123456789012345678 and the operator runs `voicebro-server --remove-consent 123456789012345678`
- **THEN** that record is deleted from the file, the command names the person, no message is sent to them, and the next start counts one person fewer

#### Scenario: Unknown id
- **WHEN** the operator runs `voicebro-server --remove-consent` with an id that has no record, or with something that is not a Discord user id
- **THEN** nothing changes and the command says so

### Requirement: Optional encryption on Linux
On Linux (not tested) headless SHALL encrypt the stored Discord token, the owner link and the consent records while `linux.encrypt_files` is `true`, the default, and SHALL never fall back to plain files then. While it is `false` it SHALL write them as plain files readable and writable only by the user that runs VoiceBro, and SHALL warn in the terminal at every start that they are not encrypted. At start it SHALL rewrite each readable file whose form differs from the setting in the setting's form. Windows SHALL ignore the setting and always encrypt.

#### Scenario: Default on Linux (not tested)
- **WHEN** Linux headless (not tested) starts with no `linux.encrypt_files` in the settings and an owner links
- **THEN** the link file is encrypted, and no warning is shown

#### Scenario: Encryption turned off on Linux (not tested)
- **WHEN** the operator sets `linux.encrypt_files` to `false` and restarts Linux headless (not tested) with an encrypted link file and records file
- **THEN** both files are rewritten as plain files readable only by that user, the owner and the records are kept, and the terminal warns that they are not encrypted

#### Scenario: Encryption turned on again (not tested)
- **WHEN** the operator sets `linux.encrypt_files` back to `true` and restarts Linux headless (not tested)
- **THEN** the plain files are rewritten encrypted, the owner and the records are kept, and the warning is gone

#### Scenario: Setting on Windows
- **WHEN** Windows headless starts with `linux.encrypt_files` set to `false`
- **THEN** the token stays in Credential Manager and the link and records files stay DPAPI-encrypted

### Requirement: Headless asset installation
`voicebro-server --install-assets` SHALL install every file the current settings need and that is missing: the Vosk runtime and catalog model for the realtime voice mode with the wake word; the Whisper runtime for the chosen device, the model and the VAD model for the Whisper and text answers modes; the voice engine, the voice and, for a Piper or Kokoro voice, the phonemizer for the Whisper mode. Running the command SHALL count as the operator's approval. It SHALL print each file with its source and size, the phonemizer's GPL-3.0 license when it installs it, and the progress, verify each pinned SHA-256, and SHALL NOT connect to Discord, start an engine, change the settings or touch the operator's own models. Normal startup SHALL list the missing files and download nothing. A `/join` refused for missing files SHALL name the command.

#### Scenario: First headless setup in the Whisper mode
- **WHEN** the settings select the Whisper mode and its files are missing
- **THEN** a normal run lists the missing Whisper runtime, model, VAD model and voice files without downloading them, and the install command installs them without a Discord token or an engine sign-in

#### Scenario: Text answers mode installs no voice
- **WHEN** the operator runs the install command in the text answers mode
- **THEN** only Whisper's files are installed, and no voice engine or voice is downloaded

#### Scenario: Join refused for missing files
- **WHEN** a member runs `/join` on the headless app and the current mode's files are missing
- **THEN** the bot stays out of the channel and the reply lists the missing files and says to stop the bot and run `voicebro-server --install-assets`, then run `/join` again

#### Scenario: GPU chosen on Linux (not tested)
- **WHEN** Linux headless (not tested) runs the install command with `whisper.device` set to `gpu`
- **THEN** the CPU runtime is installed, since there is no Linux CUDA runtime, and Whisper later runs on the CPU with the existing fallback message

### Requirement: Headless engine guidance
Every message that sends the desktop user to a window control SHALL have a terminal variant in headless: Codex, Claude Code or OpenCode not found or not signed in, missing files, and a log line that asks to link the owner again, whose terminal variant names `voicebro-server --link-owner`. The terminal variant SHALL name the setting in `settings.json` or the terminal command to use. At startup with Claude Code as the engine, a Claude subscription or an unknown sign-in kind, and an access mode other than `owner`, headless SHALL print the subscription notice once; the notice SHALL NOT refuse a join or change a setting.

#### Scenario: Claude Code missing at join
- **WHEN** a member runs `/join` on the headless app with Claude Code as the engine and no Claude Code is found
- **THEN** the bot stays out of the channel and the reply says to install Claude Code or set `claude.executable` in `settings.json`, then run `/join` again

#### Scenario: Subscription notice at start
- **WHEN** headless starts with Claude Code signed in with a Claude subscription and `discord.access` set to `everyone`
- **THEN** the terminal shows that a Claude subscription may be used only by its holder and how to let only the holder's voice through, and the bot starts normally

### Requirement: Graceful console shutdown
The binary SHALL invoke shared runtime shutdown on Ctrl+C on either platform and SIGTERM on Linux (not tested). On a Windows console close notification, it SHALL attempt the same cleanup within the time the OS allows. On Linux (not tested) every engine's child process SHALL run in its own process group and SHALL be stopped as a group, so a terminal Ctrl+C reaches only `voicebro-server` and no launcher script leaves its native process running.

#### Scenario: Stop during a voice session
- **WHEN** the operator presses Ctrl+C, or sends SIGTERM on Linux (not tested), while the bot is in a voice channel
- **THEN** the bot leaves the channel, shuts down its Discord, audio, Whisper and voice resources and stops the engine's processes before exiting

#### Scenario: Engine started through a launcher on Linux (not tested)
- **WHEN** Linux headless (not tested) stops an engine whose executable on `PATH` is an npm launcher script
- **THEN** both the launcher and the native process it started are gone after the stop
