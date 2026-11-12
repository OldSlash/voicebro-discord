# Spec Delta

## ADDED Requirements

### Requirement: Opus codec control
The Diagnostics card SHALL have an "Opus codec (experiment)" list with the two codecs,
libopus first and opus-rs second, and a help line that says what the codec does, that the choice takes effect
at the next join and the next call, and that libopus is the default. Next to the list the
card SHALL show the codec in use on the Discord side and on the call, or that none is in use
while the bot is in no channel and no call is open. A change SHALL be saved automatically
like the other fields.

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `diagnostics.opus_codec`
- **THEN** the list shows libopus

#### Scenario: Changed while in a channel
- **WHEN** the bot is in a voice channel with libopus and the user picks opus-rs
- **THEN** the choice is saved without a button, and the card still shows libopus in use until the next join

#### Scenario: After the next join
- **WHEN** the user picked opus-rs, then `/leave` and `/join` ran
- **THEN** the card shows opus-rs in use on the Discord side
