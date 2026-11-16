# Spec Delta

## MODIFIED Requirements

### Requirement: Speaker identity
The app SHALL keep, per guild, a directory from audio source id to the Discord account it
belongs to, whether that account is a bot, and the display name (server nickname, then
display name, then user name; printable characters only, trimmed, at most 32 characters)
that outlives the voice connection. When a source Discord never announced speaks and the
channel has exactly one non-bot member, that member SHALL be taken as the speaker.

#### Scenario: Nickname preferred
- **WHEN** a member with a server nickname speaks
- **THEN** the directory holds the nickname, not the account name

#### Scenario: Unknown source, one listener
- **WHEN** audio arrives from a source id Discord never announced and exactly one non-bot member is in the channel
- **THEN** that member's name and account are used for the source

#### Scenario: A bot's stream
- **WHEN** a music bot plays in a channel where exactly one person is
- **THEN** the directory holds the bot's account for its source, marked as a bot, and the person's name is not given to that source

#### Scenario: Rejoin keeps names
- **WHEN** the bot leaves and rejoins the same guild
- **THEN** names and accounts learned before are still known
