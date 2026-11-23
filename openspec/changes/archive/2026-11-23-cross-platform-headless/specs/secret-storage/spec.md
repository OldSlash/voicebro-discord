# Spec Delta

## MODIFIED Requirements

### Requirement: Secrets are stored in the OS credential store
The Windows desktop app SHALL store each secret in Windows Credential Manager as a per-user generic credential under a `VoiceBro/<secret-name>` target, persisted across reboots, and SHALL read it from there on start. Windows headless SHALL read and, through `--set-token`, write the Discord token in that store. Linux headless (not tested), which has no such store, SHALL keep the Discord token as `discord-token.bin` in its data directory, encrypted as the requirement "Optional encryption on Linux" of `headless-runtime` says.

#### Scenario: Token survives a restart
- **WHEN** the user sets the Discord token and restarts the desktop app
- **THEN** the app reads the token from the credential store and can connect without the user entering it again

#### Scenario: Token survives a reboot
- **WHEN** the user sets the Discord token and reboots the computer
- **THEN** the token is still available to the app after sign-in

#### Scenario: Stored token on Linux (not tested)
- **WHEN** the operator ran `voicebro-server --set-token` on Linux (not tested) with encryption on, and restarts the bot without `DISCORD_TOKEN_FILE`
- **THEN** the bot connects with the stored token, and `discord-token.bin` holds no readable token

### Requirement: Credential store failures are reported
If the desktop app cannot read or write the credential store, it SHALL report the failure in the status and keep running without the secret. If headless cannot read its token source (the token file, Credential Manager or, on Linux (not tested), the stored token file), it SHALL report the failure and exit without connecting.

#### Scenario: Store unavailable
- **WHEN** the desktop app cannot read the credential store on start
- **THEN** the app starts, the status shows the token as not set with the store error, and nothing is written to the settings file as a fallback

#### Scenario: Stored token unreadable on Linux (not tested)
- **WHEN** Linux headless (not tested) starts without `DISCORD_TOKEN_FILE` and `discord-token.bin` cannot be decrypted
- **THEN** it exits before connecting, names the file as unreadable without revealing anything of it, and says to run `voicebro-server --set-token` again

## ADDED Requirements

### Requirement: Headless Discord token source
Headless SHALL read an operator-provided token file named by `DISCORD_TOKEN_FILE` on either platform. When set, this file SHALL take precedence over the stored token; otherwise headless SHALL use the stored token (Windows Credential Manager, or `discord-token.bin` on Linux (not tested)). VoiceBro SHALL NOT create or update the operator's token file or expose any token to other processes, logs, settings, events or status output.

#### Scenario: Token file configured on Windows
- **WHEN** both a token file and a Windows Credential Manager token are available
- **THEN** headless connects using the token file and leaves the stored credential unchanged

#### Scenario: Token missing or unreadable
- **WHEN** the configured source has no nonblank token or cannot be read
- **THEN** headless exits before connecting and reports the missing or unreadable source without revealing a token or falling back from an explicitly configured file

#### Scenario: No token anywhere
- **WHEN** headless starts without `DISCORD_TOKEN_FILE` and with no stored token
- **THEN** it exits before connecting and says to run `voicebro-server --set-token` or to set `DISCORD_TOKEN_FILE`

### Requirement: Setting the token from the terminal
`voicebro-server --set-token` SHALL read one line from standard input, with echo off when standard input is a terminal, trim it (a leading byte order mark too), refuse a blank value or one holding characters a Discord token never has, store it as the stored token of its platform, replacing any earlier one, and exit without connecting to Discord. It SHALL NOT accept the token as a command-line argument, and SHALL NOT print, log or pass on the token.

#### Scenario: Typed at a terminal
- **WHEN** the operator runs `voicebro-server --set-token` in a terminal and types the token
- **THEN** the typed characters are not shown, the token is stored, and the command says it was stored without showing it

#### Scenario: Blank input
- **WHEN** the operator runs `voicebro-server --set-token` and enters an empty line
- **THEN** nothing is stored, any earlier token is kept, and the command says the token was blank

#### Scenario: Empty pipe from PowerShell
- **WHEN** Windows PowerShell pipes an empty string into `voicebro-server --set-token`, which arrives as a byte order mark and a line end
- **THEN** nothing is stored, any earlier token is kept, and the command says the token was blank

### Requirement: Clearing the token from the terminal
`voicebro-server --clear-token` SHALL delete the stored token of its platform (on Windows the Credential Manager entry `VoiceBro/discord_token`, which the desktop app uses too; on Linux (not tested) `discord-token.bin`) without connecting to Discord, and SHALL say what it removed or that no token was stored. It SHALL NOT touch the file `DISCORD_TOKEN_FILE` names. On Windows it SHALL say that the desktop app loses the token too.

#### Scenario: Clear a stored token
- **WHEN** a token is stored and the operator runs `voicebro-server --clear-token`
- **THEN** the stored token is gone, the command says where it was removed from, and the next start without `DISCORD_TOKEN_FILE` stops with the message that no token is set

#### Scenario: Nothing stored
- **WHEN** no token is stored and the operator runs `voicebro-server --clear-token`
- **THEN** nothing changes and the command says no token was stored
