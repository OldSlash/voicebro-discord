# Spec Delta

## MODIFIED Requirements

### Requirement: Updates keep the data
Installing a newer version over an installed one SHALL replace the program files and SHALL
keep `settings.json`, `owner-link.bin`, `logs\`, `assets\`, `models\`, `codex-workspace\`,
`webview-data\` and the stored Discord token.

#### Scenario: Update
- **WHEN** version 1.0.1 is installed over 1.0.0 with phrases set and models downloaded
- **THEN** the app starts with the same settings, models and token and shows 1.0.1

#### Scenario: Update keeps the owner
- **WHEN** a newer version is installed over one with an owner linked
- **THEN** the app starts with the same owner linked and no new code is needed

### Requirement: Uninstall removes the data only when asked
The uninstaller SHALL remove the program files and shortcuts. With "Delete the application
data" checked it SHALL also remove `settings.json`, `owner-link.bin`, `logs\`, `assets\`,
`models\`, `codex-workspace\`, `webview-data\`, the stored Discord token and the then empty
folder; unchecked, it SHALL leave them.

#### Scenario: Data kept
- **WHEN** the user uninstalls without checking the box
- **THEN** the folder still holds the settings, the owner link, logs and models, and a reinstall into it starts with them

#### Scenario: Data deleted
- **WHEN** the user uninstalls with the box checked
- **THEN** the folder is gone and the Credential Manager has no `VoiceBro/discord_token`
