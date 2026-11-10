# Spec Delta

## MODIFIED Requirements

### Requirement: What mute silences
Mute SHALL end the current session and its call, as in the first version: from the moment
of muting nothing of the channel reaches the assistant, nothing the assistant says reaches
the channel, queued answer audio is discarded, no voice call stays open, and wake phrases
are ignored.

#### Scenario: Mute during an answer then unmute
- **WHEN** mute is turned on during an answer and off a few seconds later
- **THEN** none of the old answer is heard after the unmute

#### Scenario: Wake phrase while muted
- **WHEN** someone says the activation phrase while muted
- **THEN** no session starts

#### Scenario: No call while muted
- **WHEN** mute is on for a minute in always mode
- **THEN** no voice call is open during that minute, and unmute opens a new call

## ADDED Requirements

### Requirement: Mute and unmute tones
Turning mute on SHALL play the first version's mute tone and turning it off the unmute tone,
in every voice channel the bot is in, at the earcon volume, when earcons are on; the tones
SHALL play even though the assistant's output is muted.

#### Scenario: Mute from the hotkey
- **WHEN** the user presses the mute hotkey with earcons on
- **THEN** the mute tone is heard in the channel

#### Scenario: Earcons off
- **WHEN** earcons are off and mute is toggled
- **THEN** no tone plays
