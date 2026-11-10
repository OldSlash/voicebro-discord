# Design

## Context

See proposal.md for motivation. The target is `docs/ARCHITECTURE.md` sections 4.1 (audio
core), 4.2 (Discord adapter) and 5 (data flow). Facts fixed by the probes on 2026-11-09:
songbird 0.6.1 decodes with `DecodeMode::Decode(DecodeConfig::new(Channels::Stereo,
SampleRate::Hz48000))`; a `RawAdapter` output track needs symphonia's `pcm` feature in the
owning crate; `on_track`-style events and `VoiceTick` fire every 20 ms while connected;
`Songbird::remove` stops old handlers for good; closing the source (`read` returns 0) ends the
track with one `End` event; the handler's own work costs about 40 µs at p99. The core has no
async runtime of its own: the desktop passes a tokio `Handle`.

## Goals / Non-Goals

**Goals:**

- The bot connects, joins, hears speakers separately and plays a track, with the lifetimes
  that the first version got wrong fixed.
- The frames and events the gate, the wake engine and the assistant will consume exist and
  are tested.

**Non-Goals:**

- No wake word, no session logic, no assistant: the tick sink in this change is a `TickSink`
  trait with a counting implementation; the gate implements it next.
- No persistent speaker directory across restarts.
- No Opus passthrough.

## Decisions

**D1. `audio/` is written new from the architecture, `dsp.rs` is copied.** The queues,
output source and earcons are small and written new; the resamplers are ours and
tested. Alternative: port `audio.rs`. Rejected by the reuse policy.

**D2. The receive path mixes into reusable buffers and pushes `Frame`s.** `Frame` is a fixed
`[i16; 1920]` boxed once per queue slot; the tick handler fills a scratch `Vec<i32>`, clamps
into a `Frame` taken from a pool, and pushes to the `TickSink`. No allocation after warm-up.

**D3. `OutputQueue` + `PcmSource` as in the architecture, f32 output.** songbird's
`RawAdapter` carries f32 PCM; the source converts i16 to f32 on read and applies the earcon
mix there. The flush epoch is an `AtomicU64`: a frame queued before the current epoch is
skipped on read. `close()` sets a flag so the next `read` returns 0.

**D4. The output track is created once per join and closed on leave.** `/leave` calls
`OutputHandle::close()`, waits up to 200 ms for the `End` event, then `Songbird::remove`.
Alternative: `leave`. Rejected: it keeps handlers and tracks (v1 defect).

**D5. Join order is enforced by one function.** `join_voice(guild, channel)` does, in this
order: `get_or_insert`, `add_global_event(SpeakingStateUpdate)`,
`add_global_event(VoiceTick)`, `join`, attach the track, post the panel. A per-guild async
mutex is held across the whole function so concurrent commands cannot interleave.

**D6. Speaker directory per guild in `DiscordRuntime`, preloaded into the session.** Names
are resolved on `SpeakingStateUpdate` through the cache, then HTTP if the cache misses. The
one-listener fallback runs in the tick handler when a source has no name.

**D7. Member panel edits are coalesced.** A small task per guild wakes on state events, edits
the message at most once per second with the latest state, tolerates a deleted message, and
is cancelled on leave. The Mute GPT button's custom id is `voicebro:mute:<guild_id>`.

**D8. `DiscordRuntime` owns the client task and the shard manager.** `start(token, guild_id)`
spawns on the tokio handle; `stop()` shuts the shards down and awaits the task. `Runtime`
calls `start` at start-up when a token is set and auto-connect is on, and on `set_secret`;
`stop` on `clear_secret` and `discord_disconnect`. A changed `discord.guild_id` applies at
the next connection and the log says so.

**D9. Slash commands: guild-scoped when a guild id is set, global otherwise.** Guild commands
appear at once; global ones can take an hour, which the log states. Both sets have the same
three commands.

**D10. `DiscordStatus` is published as `AppEvent::Discord` and embedded in `Status`.** The UI
reads counters from it; nothing in it is a label.

## Risks / Trade-offs

- [The `davey` crate prints `[DAVE Binary]` lines to stdout] → the log goes to a file, stdout
  is only the debug console; noted, not fixed here.
- [Cache misses for member names right after connect] → HTTP fallback with a per-guild
  rate-limited lookup; an unresolved name stays "unknown" in the log, never invented.
- [Panel edits hit rate limits] → coalescing to one edit per second; `/status` carries the
  same information.
- [`remove` while the mixer is mid-read] → close the source first and wait for `End` before
  `remove`.

## Open Questions

None.
