# Proposal

## Why

The app today is a shell: settings, token storage, log and a window. It cannot connect to
Discord, so nothing can be tested as a bot. The Discord adapter and the audio core are the
first product behavior: the bot comes online with the stored token, joins the caller's voice
channel on `/join`, hears every speaker separately, and plays a track back. Every later
change (wake word, gate, Codex) plugs into the frames and events this one produces.

## What Changes

- `voicebro-core::audio`: fixed 48 kHz stereo 20 ms frames, the `dsp.rs` resamplers, bounded
  drop-oldest queues, the output queue with jitter buffer, flush epoch, mute and earcons, and
  the `PcmSource` songbird reads; `AudioPorts` for the assistant.
- `voicebro-core::discord`: the serenity client with songbird (`receive`, decode to 48 kHz
  stereo), slash commands `/join`, `/leave`, `/status`, the speaker directory (SSRC to
  display name) that outlives the call, the participant list, the member panel with a
  "Mute GPT" button, and the receive path that mixes speakers in `i32` and hands the mixed
  frame plus each speaker's frame to a tick sink (the gate, in the next change).
- Settings: `discord.guild_id` (slash commands registered in that guild so they appear at
  once; empty means global).
- `Runtime`: connects when a token is stored and `auto_connect` is on, reconnects when the
  token changes, disconnects when it is cleared, exposes `DiscordStatus` in the status
  snapshot and as `AppEvent::Discord`.
- Desktop: Discord state on the Status tab, Connect and Disconnect buttons, the guild id
  field on the Settings tab.

## Capabilities

### New Capabilities

- `discord-connection`: the bot's gateway connection: when it connects, how it reports its
  state and errors, how token and guild changes are applied.
- `voice-channel-session`: joining and leaving a voice channel on command, the order of
  handler registration, speaker identity, participants, and the member panel.
- `audio-transport`: the audio contracts between Discord and the rest of the app: frames,
  mixing, queues, the output track's lifetime, mute and earcons.

### Modified Capabilities

- `app-settings`: a new `discord.guild_id` field with validation (digits or empty).

## Impact

- New modules `crates/voicebro-core/src/audio/` and `crates/voicebro-core/src/discord/`;
  `settings`, `runtime`, `status`, `events` extended.
- New dependencies in the core: `serenity` 0.12.5, `songbird` 0.6.1 (`receive`),
  `symphonia` with `pcm`, `symphonia-core`. CMake is needed to build `opus2` through
  songbird.
- Desktop: new commands `discord_connect`, `discord_disconnect`; the UI shows Discord state.
- The bot needs the `bot` and `applications.commands` scopes and the permissions View
  Channels, Send Messages, Embed Links, Connect, Speak, Use Voice Activity in the guild.
