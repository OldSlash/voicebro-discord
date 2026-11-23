# Spec Delta

## ADDED Requirements

### Requirement: Linux encryption setting
The settings SHALL hold `linux.encrypt_files`, a boolean, `true` by default; a settings file without it SHALL read as `true`, and a value that is not a boolean SHALL be rejected with an error naming the field. It SHALL decide only whether Linux headless (not tested) encrypts the stored Discord token, the owner link and the consent records, as the requirement "Optional encryption on Linux" of `headless-runtime` says. Windows SHALL ignore it. The window SHALL NOT show it, and a save from the window SHALL keep its stored value.

#### Scenario: Older settings file
- **WHEN** the app loads a settings file written before this field existed
- **THEN** `linux.encrypt_files` is `true`, and the file is not rewritten for it

#### Scenario: Not a boolean
- **WHEN** `settings.json` holds `"linux": {"encrypt_files": "no"}`
- **THEN** the settings are rejected with an error naming `linux.encrypt_files`

#### Scenario: Desktop save keeps the value
- **WHEN** the settings file holds `linux.encrypt_files` set to `false` and the user changes a setting in the window
- **THEN** the saved file still holds `linux.encrypt_files` set to `false`
