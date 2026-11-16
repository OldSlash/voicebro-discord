# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Audio (both gains, earcons and their volume); Assistant (session mode,
idle timeout, wake engine and model, activation and deactivation phrases); Codex (refresh
the lists at start, model, reasoning effort, voice, answer handoff, speaker labels, save
chat history, post answers to Discord and its channel, instructions); Hotkey (mute), in
that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab
- **THEN** it shows the cards "Discord and Codex", Audio, Assistant, Codex and Hotkey, in that order

## ADDED Requirements

### Requirement: Answer posting control
The Codex card SHALL have a "Post answers to Discord" list after "Save chat history", with
Off, The voice channel's chat, The channel of /join and A chosen channel, saved on change.
Its help line SHALL say that the agent's final answers are posted there and that the bot
needs View Channel, Send Messages and Embed Links in that channel.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.post_answers`
- **THEN** the list shows Off and no channel list is shown

#### Scenario: Voice channel's chat chosen
- **WHEN** the user picks The voice channel's chat
- **THEN** it is saved without a button, no channel list is shown, and a restart shows the choice

### Requirement: Channel list for a chosen channel
While A chosen channel is picked, the card SHALL show a list of the text channels of every
server the bot is in, grouped by server. A channel the bot may not post in SHALL be marked
so. While the bot is not connected, the list SHALL show the stored channel by its id and say
that the names appear once the bot is connected.

#### Scenario: Picking a channel
- **WHEN** the user picks A chosen channel, then the channel "answers" of server A
- **THEN** `assistant.post_answers` and `assistant.answers_channel` are saved together with that channel's id, and the list shows "answers" under server A

#### Scenario: No channel picked yet
- **WHEN** the user picks A chosen channel while no channel is stored and leaves the channel list empty
- **THEN** nothing is saved, the stored destination stays as it was, and the card says to pick a channel

#### Scenario: A channel the bot may not post in
- **WHEN** the bot's role may not send messages in the channel "rules"
- **THEN** "rules" is shown in the list marked as a channel the bot cannot post in

#### Scenario: Bot not connected
- **WHEN** the window opens while the bot is not connected and a channel id is stored
- **THEN** the list shows that id as the chosen channel and a note that the channel names appear once the bot is connected
