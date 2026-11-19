## REMOVED Requirements

### Requirement: Owner-only setting
**Reason**: The single owner-only switch becomes one value of the access mode, which also
has the `agreed` value of the `speaker-consent` capability.
**Migration**: A file with `discord.owner_only: true` loads as `discord.access: "owner"`,
one with `false` or without the field as `"everyone"`; the next save writes
`discord.access` and drops `discord.owner_only` (requirement "Access mode setting").

## ADDED Requirements

### Requirement: Access mode setting
The settings SHALL hold `discord.access`, one of `everyone`, `agreed` or `owner`, with the
default `everyone`. A save that changes only this field SHALL restart nothing and SHALL apply
from the next voice frame. The settings SHALL hold no owner account and no consent record.

#### Scenario: Default
- **WHEN** the app starts with no settings file
- **THEN** `discord.access` is `everyone`

#### Scenario: Not a known value
- **WHEN** a save sends `discord.access` as `"friends"`
- **THEN** the save is rejected with an error naming `discord.access` and its three values, and the stored settings are unchanged

#### Scenario: Changed while in a channel
- **WHEN** the user saves `discord.access` as `agreed` while the bot is in a voice channel
- **THEN** the bot stays in the channel, no gate, wake engine or call is rebuilt, and the next frame is filtered

### Requirement: The owner-only switch of older files
A settings file without `discord.access` SHALL load the access mode from
`discord.owner_only`: `true` as `owner`, `false` or a missing field as `everyone`. The next
save SHALL write `discord.access` and SHALL NOT write `discord.owner_only`. When a file has
both, `discord.access` SHALL decide.

#### Scenario: Owner-only on in an older file
- **WHEN** the app loads a settings file from build 1.1.0 with `discord.owner_only: true`
- **THEN** the access mode is `owner`, and the next save writes `"access": "owner"` and no `owner_only`

#### Scenario: A file from build 1.0.1
- **WHEN** the app loads a settings file without either field
- **THEN** the access mode is `everyone`

### Requirement: Privacy policy URL setting
The settings SHALL hold `discord.privacy_url`, empty by default. A value SHALL be empty or
an `https://` URL with a host, at most 512 characters, stored trimmed. Any other value SHALL
be rejected with an error naming the field. A save that changes only this field SHALL
restart nothing.

#### Scenario: A valid URL
- **WHEN** the user saves `discord.privacy_url` as ` https://example.org/voicebro-privacy `
- **THEN** it is stored as `https://example.org/voicebro-privacy` and the next `/privacy` replies with it

#### Scenario: Not https
- **WHEN** a save sends `discord.privacy_url` as `http://example.org/privacy`
- **THEN** the save is rejected with an error naming `discord.privacy_url` and saying it must start with `https://`
