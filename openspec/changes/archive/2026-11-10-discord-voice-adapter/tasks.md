# Tasks

## 1. Audio core

- [x] 1.1 Add `audio/frame.rs` (`Frame`, `FRAME_SAMPLES`, `peak`, `is_silent`) and copy v1 `dsp.rs` as `audio/dsp.rs` with its tests; verify `cargo test -p voicebro-core audio::` passes
- [x] 1.2 Add `audio/queue.rs`: bounded `FrameQueue` (4 frames, drop oldest, counters); verify tests for overflow count and order
- [x] 1.3 Add `audio/output.rs`: `OutputQueue` (8 frames, jitter 3→8, relax after 750 stable frames, flush epoch, mute, `last_signal`, `close`) and `PcmSource` (`Read`+`Seek`+`MediaSource`, f32, one packet per read, silence on underrun, EOF only after close); verify tests for jitter growth and relax, flush, mute, EOF
- [x] 1.4 Add `audio/earcon.rs` (rising/falling tones, volume 0.18, mixed in even when muted); verify a test that a muted source still outputs the earcon samples
- [x] 1.5 Add `AudioPorts`, `FrameSender`, `OutputHandle`; verify a test that input frames reach the receiver and output frames reach the source

## 2. Discord connection (spec `discord-connection`, `app-settings`)

- [x] 2.1 Add `discord.guild_id` to `Settings` with digits-or-empty validation; verify tests for valid, empty and invalid values
- [x] 2.2 Add `discord/mod.rs` with `DiscordStatus`, `DiscordRuntime::new(events, handle)`, `start(token, guild_id)`, `stop()`, `status()`; the client uses `GUILDS | GUILD_VOICE_STATES` and songbird decoding to 48 kHz stereo; verify a unit test that `start` without a token is refused and a live run (token from `probe.local.json` through the runtime) that reaches `connected` with the bot's user name
- [x] 2.3 Register `/join`, `/leave`, `/status` on `ready`, guild-scoped when a guild id is set, global otherwise, with a log line either way; verify in the test guild that the commands appear right after connect
- [x] 2.4 Wire `Runtime`: tokio handle in `Runtime::new`, connect at start when a token is set and auto-connect is on, reconnect on `set_secret`, disconnect on `clear_secret`, `connect()`/`disconnect()` methods, `AppEvent::Discord`, `Status.discord`; verify a live run: start the app with the stored token and see `connected` in the status; clear the token and see `disconnected`
- [x] 2.5 Desktop: `discord_connect`/`discord_disconnect` commands, Status tab shows state, user, guilds, voice channels, counters and last error, Connect/Disconnect buttons, guild id field on Settings; verify in the running app that the state changes on the buttons

## 3. Voice session (spec `voice-channel-session`)

- [x] 3.1 Add `discord/speakers.rs`: `SpeakerDirectory` per guild, `clean_display_name`, name resolution (nickname → display name → user name, cache then HTTP), one-listener fallback; verify unit tests for cleaning and fallback
- [x] 3.2 Add `discord/session.rs`: `join_voice(guild, channel)` with the fixed order (get_or_insert, speaking handler, tick handler, join, track, panel) under a per-guild async mutex; `leave_voice` (close source, await `End` ≤ 200 ms, `remove`, drop gate, remove panel); verify a live run: `/join` from a voice channel joins it, `/leave` leaves, a rejoin's old tick counter stays flat
- [x] 3.3 Handle `/join` (caller's channel from the cache; refusal when not in voice), `/leave`, `/status` (ephemeral); verify each reply text in the test guild
- [x] 3.4 Keep participants from `voice_state_update` and expose them to the tick sink; verify a live run where a second account joins and leaves the channel and the list follows
- [x] 3.5 Add `discord/panel.rs`: embed, Mute GPT button, coalesced edits (≤ 1/s), removal on leave, tolerance of a deleted message; verify a live run: the button toggles mute and the app window reflects it within a second

## 4. Receive path (spec `audio-transport`)

- [x] 4.1 Add `discord/receive.rs`: `TickSink` trait, the `VoiceTick` handler that mixes in `i32` into reusable buffers, clamps, passes mixed and per-speaker frames, counts ticks, speakers, malformed frames; verify unit tests for the clamp arithmetic and a live run that logs tick and speaker counters while someone talks
- [x] 4.2 Attach the output track per join from `RawAdapter::new(PcmSource, 48_000, 2)` with `play_only`, log `End`/`Error` events; verify a live run that plays a 48 kHz WAV into the channel through the output queue and a second account hears it
- [x] 4.3 Put the counters into `DiscordStatus`; verify the status after a minute in the channel shows non-zero ticks and the drop counters

## 5. Close-out

- [x] 5.1 Live check in the test guild: connect, `/join`, speak, `/status`, mute from the panel, `/leave`, `/join` again; verify the log shows the sequence with no error and the old handler silent after rejoin

  Closed by the user's decision of 2026-11-10 at the merge into `main` (every remaining check counts as done); not verified separately as a task.
- [x] 5.2 Update `docs/ARCHITECTURE.md` 4.1 and 4.2 where the code differs, `AGENTS.md` commands and facts, and `LOCAL-TESTING.md` for the live checks; verify no stale statement remains
- [x] 5.3 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass

## Workflow follow-up

- Checkpoint commit per task group on `change/discord-voice-adapter`.
- Merge into `main` with `--no-ff --log` and archive the change when the user says so.
