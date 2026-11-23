# Spec Delta

## MODIFIED Requirements

### Requirement: Member panel
On a successful `/join` the bot SHALL post a panel message in the command's channel showing
the engine name, the assistant state, the assistant mode and the participants, with the
buttons the `discord-panel-controls` capability describes: Mute assistant, Leave channel
and the three assistant modes, pressed by the linked owner only. Mute assistant toggles the
app-wide mute. The panel SHALL be updated when the state changes, at most once per second,
and removed on `/leave` and on Leave channel.

#### Scenario: Mute from the panel
- **WHEN** the linked owner presses the panel's Mute assistant button
- **THEN** the app-wide mute toggles, the panel button label changes, and the app window shows the same state

#### Scenario: Panel message deleted by someone
- **WHEN** the panel message no longer exists and the state changes
- **THEN** no error is raised, and the next `/join` posts a new panel

#### Scenario: The engine on the panel
- **WHEN** the engine is Claude Code and a member runs `/join`
- **THEN** the panel's Engine field reads "Claude" and its button reads "Mute assistant", naming no other engine
