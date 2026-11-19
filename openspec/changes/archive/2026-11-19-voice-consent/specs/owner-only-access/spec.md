## MODIFIED Requirements

### Requirement: The owner-only mode is optional
The owner-only mode SHALL be on while `discord.access` is `owner` and off for its other
values. While `discord.access` is `everyone`, the app SHALL let every member in the bot's
voice channel reach the wake word and the assistant, as without this mode. While it is
`agreed`, the `speaker-consent` capability decides which voices pass. A stored link SHALL
change nothing while the access mode is `everyone`.

#### Scenario: Mode off with a link
- **WHEN** the access mode is `everyone`, an owner is linked, and another member says the activation phrase
- **THEN** a session starts as it would without a link

#### Scenario: The agreed mode with a link
- **WHEN** the access mode is `agreed`, an owner is linked, and the owner says the activation phrase
- **THEN** a session starts

### Requirement: Mode and link changes apply at once
Changing the access mode, linking and unlinking SHALL take effect from the next 20 ms
frame in a channel the bot is already in, without a rejoin. While the owner-only mode is
on and no owner is linked, no member's audio SHALL reach the assistant.

#### Scenario: Unlink during a session
- **WHEN** the mode is on and the user presses Unlink while a session is active
- **THEN** no audio reaches the assistant from the next frame on

#### Scenario: Mode turned off in a channel
- **WHEN** the user changes the access mode from `owner` to `everyone` while the bot is in a voice channel
- **THEN** every member's audio reaches the wake word from the next frame on

#### Scenario: Owner mode to the agreed mode
- **WHEN** the user changes the access mode from `owner` to `agreed` while the bot is in a voice channel and one other member has a consent record
- **THEN** from the next frame on the audio of the owner and of that member reaches the wake word, and no one else's does

#### Scenario: Mode on, nothing linked
- **WHEN** the mode is on, no owner is linked, and a member runs `/join` and says the activation phrase
- **THEN** the bot joins the channel and no session starts
