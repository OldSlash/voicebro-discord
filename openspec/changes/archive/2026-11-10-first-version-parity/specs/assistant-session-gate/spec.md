# Spec Delta

## MODIFIED Requirements

### Requirement: Two modes
In `always` mode a session SHALL be wanted whenever the bot is in a voice channel and mute
is off. In `wake_word` mode a session SHALL start only on an activation phrase while mute is
off and end on a deactivation phrase, the idle timeout or mute.

#### Scenario: Always mode
- **WHEN** the bot joins a channel in always mode
- **THEN** the assistant session is wanted at once and the channel audio flows to it

#### Scenario: Wake-word mode, nobody called
- **WHEN** the bot is in a channel in wake-word mode and no phrase has been said
- **THEN** no audio reaches the assistant and the assistant's output is muted

#### Scenario: Always mode, muted
- **WHEN** mute is turned on in always mode
- **THEN** the session is no longer wanted, and turning mute off makes it wanted again
