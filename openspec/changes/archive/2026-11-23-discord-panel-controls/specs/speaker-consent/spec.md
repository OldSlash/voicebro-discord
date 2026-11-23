# Spec Delta

## MODIFIED Requirements

### Requirement: The commands stay open to everyone
The `agreed` mode SHALL NOT limit who can use `/join`, `/leave`, `/status`, `/optin`,
`/optout` or `/privacy`, and SHALL change nothing about who hears the assistant's voice or
sees its posted answers. The panel's buttons, Mute assistant included, SHALL be pressed by
the linked owner only, as the `discord-panel-controls` capability describes.

#### Scenario: A member without a record runs `/join`
- **WHEN** the mode is `agreed` and a member without a record runs `/join` from a voice channel
- **THEN** the bot joins that channel, and only the owner and recorded members are heard there

#### Scenario: A member with a record presses Mute
- **WHEN** the mode is `agreed` and a member with a consent record who is not the owner presses the panel's Mute button
- **THEN** the mute state does not change, and only that member sees the reply that the panel's buttons are the owner's
