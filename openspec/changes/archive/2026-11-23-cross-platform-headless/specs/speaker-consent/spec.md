# Spec Delta

## RENAMED Requirements

- FROM: `### Requirement: Remove in the window acts as `/optout``
- TO: `### Requirement: Remove in the window or the terminal acts as `/optout``

## MODIFIED Requirements

### Requirement: Remove in the window or the terminal acts as `/optout`
Remove next to a person in the Access card, or `voicebro-server --remove-consent <Discord user id>` on the headless app, SHALL delete their record as `/optout` does. The app SHALL NOT send that person any message about it.

#### Scenario: Remove
- **WHEN** the user presses Remove next to a recorded member
- **THEN** the member leaves the list, the record is deleted from the file, their audio is dropped from the next frame on in the `agreed` mode, and no message is sent to them

#### Scenario: Remove from the terminal
- **WHEN** the operator runs `voicebro-server --remove-consent` with a recorded member's id while the bot is stopped
- **THEN** the record is deleted from the file, the member is not heard in the `agreed` mode from the next start on, and no message is sent to them

### Requirement: The records are stored encrypted in the app's folder
The app SHALL keep the records as one file in its own folder, encrypted so that only the same user on the same machine can read it, and never in the settings file: on Windows with DPAPI for the current Windows user; on Linux (not tested) with the same authenticated encryption and key as the owner link. The app SHALL never write the records unencrypted, except on Linux (not tested) while `linux.encrypt_files` is `false`, when it SHALL be a plain file readable and writable only by the user running VoiceBro; when it cannot encrypt, it SHALL report why. A file that cannot be decrypted or read SHALL count as no records and SHALL be reported in the log once at start and in the Access card, or on the headless app in the terminal.

#### Scenario: Restart
- **WHEN** two members are recorded and the app is closed and started again
- **THEN** both are still recorded

#### Scenario: Edited file
- **WHEN** one byte of the records file is changed and the app starts
- **THEN** no member is recorded, the Access card (on the headless app, the terminal) says the records could not be read, and the log names the file

#### Scenario: Records file on Linux (not tested)
- **WHEN** a member agrees with `/optin` on Linux headless (not tested) and the bot is restarted
- **THEN** the member is still recorded, and the records file holds no readable Discord id or name
