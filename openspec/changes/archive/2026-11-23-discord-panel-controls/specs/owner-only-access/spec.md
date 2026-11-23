# Spec Delta

## MODIFIED Requirements

### Requirement: The commands stay open to everyone
The mode SHALL NOT limit who can use `/join`, `/leave` or `/status`. They SHALL work for
every member as without the mode. The panel's buttons, Mute assistant included, SHALL be
pressed by the linked owner only, whether this mode is on or off, as the
`discord-panel-controls` capability describes.

#### Scenario: Another member runs `/join`
- **WHEN** the mode is on and a member who is not the owner runs `/join` from a voice channel
- **THEN** the bot joins that channel, and only the owner's voice reaches the assistant there

#### Scenario: Another member presses Mute
- **WHEN** the mode is on and a member who is not the owner presses the panel's Mute button
- **THEN** the mute state does not change, and only that member sees the reply that the panel's buttons are the owner's

#### Scenario: Another member runs `/leave`
- **WHEN** the mode is on and a member who is not the owner runs `/leave` while the bot is in a voice channel of that server
- **THEN** the bot leaves the channel as for the owner
