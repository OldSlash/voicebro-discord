# mute-control Specification

## Purpose
Gives the user one switch, reachable from the app, a hotkey and Discord, that stops the
assistant from hearing and from being heard.

## Requirements

### Requirement: One mute state, three sources
The app SHALL keep one app-wide mute state. The app window's button, the global hotkey and
the Discord panel's button SHALL all toggle that same state, and every view SHALL show it.

#### Scenario: Toggle from the hotkey
- **WHEN** the user presses the mute hotkey
- **THEN** the app window, the Discord panel and `/status` all show muted within a second

### Requirement: What mute silences
Mute SHALL end the current session and its call: from the moment
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

### Requirement: Global hotkey
The hotkey from `hotkey.mute` SHALL be registered system-wide at start-up and again when
the settings are saved, the previous one unregistered first; an empty value SHALL register
nothing. The status SHALL show the shortcut, whether it is registered, and the error if not.

#### Scenario: Hotkey changed in settings
- **WHEN** the user saves a different shortcut
- **THEN** the old one no longer toggles mute and the new one does, without a restart

#### Scenario: Registration fails
- **WHEN** the shortcut cannot be registered (taken by another app)
- **THEN** the status shows registered = false with the error, and the other mute sources still work

#### Scenario: Game in front
- **WHEN** a full-screen game has focus and the user presses the hotkey
- **THEN** mute toggles (verified by the user; the agent cannot do this)

### Requirement: Mute and unmute tones
Turning mute on SHALL play the mute tone and turning it off the unmute tone,
in every voice channel the bot is in, at the earcon volume, when earcons are on; the tones
SHALL play even though the assistant's output is muted.

#### Scenario: Mute from the hotkey
- **WHEN** the user presses the mute hotkey with earcons on
- **THEN** the mute tone is heard in the channel

#### Scenario: Earcons off
- **WHEN** earcons are off and mute is toggled
- **THEN** no tone plays
