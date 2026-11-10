# voice-channel-session Specification

## Purpose
Puts the bot into the caller's voice channel on command, keeps speaker identity and the
participant list current, and leaves cleanly so nothing from one session leaks into the
next.

## Requirements

### Requirement: `/join` joins the caller's voice channel
On `/join`, the bot SHALL join the voice channel the invoking user is in, within that guild,
and reply in the channel of the command. If the user is not in a voice channel the bot SHALL
reply with that and join nothing.

#### Scenario: Caller in a voice channel
- **WHEN** a member in a voice channel runs `/join`
- **THEN** the bot joins that channel, the status lists the guild and channel name, and the reply says which channel it joined

#### Scenario: Caller not in a voice channel
- **WHEN** a member who is not in any voice channel runs `/join`
- **THEN** the bot stays where it is and the reply says the member must be in a voice channel

#### Scenario: Already in a channel of this guild
- **WHEN** `/join` is run while the bot is already in a voice channel of the same guild
- **THEN** the bot moves to the caller's channel and no old handler or track survives

### Requirement: Speaking handlers are registered before the join
The app SHALL register the speaking-state and voice-tick handlers on the call before joining
the channel, because Discord announces a user's audio source id only the first time they
speak.

#### Scenario: First words after join
- **WHEN** a member speaks for the first time after the bot joined
- **THEN** their audio source is mapped to their display name and that name is available to the rest of the app

### Requirement: Speaker identity
The app SHALL keep, per guild, a directory from audio source id to display name (server
nickname, then display name, then user name; printable characters only, trimmed, at most
32 characters) that outlives the voice connection. When an unknown source speaks and the
channel has exactly one non-bot member, that member SHALL be taken as the speaker.

#### Scenario: Nickname preferred
- **WHEN** a member with a server nickname speaks
- **THEN** the directory holds the nickname, not the account name

#### Scenario: Unknown source, one listener
- **WHEN** audio arrives from a source id with no entry and exactly one non-bot member is in the channel
- **THEN** that member's name is used for the source

#### Scenario: Rejoin keeps names
- **WHEN** the bot leaves and rejoins the same guild
- **THEN** names learned before are still known

### Requirement: Participants are kept current
The app SHALL maintain the sorted list of non-bot members in the bot's voice channel from
voice state updates and make it available to the session logic.

#### Scenario: Member joins or leaves the channel
- **WHEN** a member enters or leaves the bot's voice channel
- **THEN** the participant list changes accordingly within the next voice state update

### Requirement: `/leave` removes everything
On `/leave`, the bot SHALL close the output source, remove the voice connection entirely
(not merely leave), drop its handlers and reply that it left. A later `/join` SHALL start
with fresh handlers and a fresh track.

#### Scenario: Leave then rejoin
- **WHEN** `/leave` is followed by `/join`
- **THEN** the voice tick counter of the first connection does not move after the rejoin, and the new connection ticks

#### Scenario: Leave when not connected
- **WHEN** `/leave` is run while the bot is not in a voice channel of the guild
- **THEN** the reply says it was not in a channel and nothing else changes

### Requirement: `/status` reports the state
`/status` SHALL reply only to the caller with the bot's connection state, the voice channel
it is in (if any), the assistant state (waiting, listening, muted) and the mute state.

#### Scenario: Status while in a channel
- **WHEN** a member runs `/status` while the bot is in a voice channel
- **THEN** the ephemeral reply names the channel and shows the assistant and mute state

### Requirement: Member panel
On a successful `/join` the bot SHALL post a panel message in the command's channel showing
the engine name, the assistant state and the participant count, with a "Mute GPT" button
that toggles the app-wide mute. The panel SHALL be updated when the state changes, at most
once per second, and removed on `/leave`.

#### Scenario: Mute from the panel
- **WHEN** a member presses the panel's Mute GPT button
- **THEN** the app-wide mute toggles, the panel button label changes, and the app window shows the same state

#### Scenario: Panel message deleted by someone
- **WHEN** the panel message no longer exists and the state changes
- **THEN** no error is raised, and the next `/join` posts a new panel
