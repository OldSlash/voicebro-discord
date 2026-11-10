# Spec Delta

## Purpose

Keeps the Discord bot token and any future credential in the operating system's protected
store so that no file, log, snapshot or event ever carries it.

## ADDED Requirements

### Requirement: Secrets are stored in the OS credential store
The app SHALL store each secret in the Windows Credential Manager as a per-user generic
credential under a `VoiceBro/<secret-name>` target, persisted across reboots, and SHALL read
it from there on start.

#### Scenario: Token survives a restart
- **WHEN** the user sets the Discord token and restarts the app
- **THEN** the app reads the token from the credential store and can connect without the user entering it again

#### Scenario: Token survives a reboot
- **WHEN** the user sets the Discord token and reboots the computer
- **THEN** the token is still available to the app after sign-in

### Requirement: Secrets never leave the secret store except to their consumer
The app SHALL NOT write a secret to the settings file, the log, a status snapshot or an event.
Only the component that needs the secret (the Discord client) receives its value.

#### Scenario: Status snapshot
- **WHEN** the UI requests settings or status
- **THEN** the response contains `is_set` for each secret and never its value

#### Scenario: Log output
- **WHEN** the app logs the result of setting or reading a secret
- **THEN** the log line contains the secret's name and outcome, never its value

### Requirement: Secrets can be set and cleared from the UI
The app SHALL accept a secret value from the UI through a write-only command, store it, and
SHALL allow clearing it; after clearing, the secret is removed from the credential store.

#### Scenario: Set a token
- **WHEN** the user enters a token and confirms
- **THEN** the token is written to the credential store and the status shows the token as set

#### Scenario: Clear a token
- **WHEN** the user clears the token
- **THEN** the credential store entry is deleted, the status shows the token as not set, and a running Discord connection is stopped

### Requirement: Credential store failures are reported
If the credential store cannot be read or written, the app SHALL report the failure in the
status and keep running without the secret.

#### Scenario: Store unavailable
- **WHEN** reading the credential store fails on start
- **THEN** the app starts, the status shows the token as not set with the store error, and nothing is written to the settings file as a fallback
