# Design

## Context

See proposal.md and `docs/NEXT-CHANGES.md` section 8 (the comparison tables A1 to F2). The
first version's references: `codex.rs` `start_guild` and `ensure_ready` (join preparation),
`gate.rs` `wants_session` and `set_muted` (mute ends the session), `audio.rs` `Earcon`
(four tones), `discord.rs` lines 860 to 940 (mixing by count, the "only person in the
channel" fallback) and 986 to 1017 (commands in the guild and globally), its Console tab
(the whole log in the window). This app today: `CodexAssistant::start` runs at
`Runtime::start`; `RelayGate::wants_session` ignores mute; `mix_into` sums and clamps; the
window's log pane receives only `AppEvent::Log` lines; the recognizer logs with `log::info!`
only; `discord.auto_connect` gates the start; commands go to the guild or globally.

## Goals / Non-Goals

**Goals:** the first version's behaviour for each listed item; the user's decision that the
Codex process starts at `/join`.

**Non-Goals:** the Console tab's design (change `chat-and-console-tabs` builds it on the
records this change forwards); any change to the wake rules or the call audio.

## Decisions

**D1. Join preparation through a host hook.** The `/join` handler awaits an asynchronous
preparation step from the host before `songbird.join`. The runtime implements it with
`CodexAssistant::prepare(guild)`: start the app-server if it is not running, read the
account, open or reuse the guild's thread. An error (not found, handshake failed, signed out)
is returned to the handler, which replies with it and does not join. `attach` after a
successful preparation never sees a stopped server, so the "Codex is starting" error path is
removed. *Alternative:* join first and prepare in the background. Rejected: the first
activation would race the thread opening, and a signed-out account would only show up when
someone speaks.

**D2. Stopping the app-server.** `detach` of the last attached guild stops every call and
kills the app-server; the per-guild conversations stay in memory so the next join continues
them (a new process means a new thread; the history carries over through `initialItems`, as
on a model change). Sign in and Check start the app-server on demand and arm a one-minute
timer that stops it again when no guild is attached and no sign-in is waiting for
`account/login/completed`.

**D3. Mute ends the session.** `wants_session` becomes `!muted && (always || active)`;
`set_muted(true)` ends the session, flushes the output, plays the mute tone and tells the
assistant the session is not wanted; `set_muted(false)` plays the unmute tone and, in always
mode, makes the session wanted again. The gate no longer sends silent frames while muted.
The registry mute flag and the three mute sources stay as they are.

**D4. Tones.** The first version's `earcon_samples` (`audio.rs` lines 228 to 260, the
user's own code by `git blame`) is ported as it is, all four tones: mute 660 Hz 90 ms then
440 Hz 110 ms, unmute the reverse, session start 523.25, 659.25 and 783.99 Hz (70, 70,
110 ms), session end 659.25 Hz 80 ms then 392 Hz 130 ms. Today's two-note start and end
tones are replaced by the first version's. The volume setting and the "play while muted"
rule stay.

**D5. Mixing.** `mix_into` divides each summed sample by the number of speakers in the tick
when there are two or more, in `i32`, before the single clamp; one speaker passes unchanged.
The speech threshold of 700 used by the gate is checked on a two-speaker recording to still
detect speech.

**D6. The app log reaches the window.** The desktop shell adds a log target that keeps the
last 500 formatted records in a ring buffer and emits each new record to the window; a
command returns the buffer when the window loads. The target uses the same level filters as
the file (this app's crates at Info, third-party crates at Warn, songbird's receive task at
Error per `voice-path-quality`). Before writing it, check in the `tauri-plugin-log` 2.10
source which target kind accepts a callback (`TargetKind::Dispatch` with a `fern` output
callback, or `TargetKind::Webview`); a webview-only target is not enough because it keeps no
history. The recognizer's lines already go through `log`; nothing in the wake worker
changes. The window stops listing `AppEvent::Log` separately, so lines are not doubled.

**D7. Unknown speaker.** In the receive handler, a speaking source without a directory entry
is tried once: a task reads the bot's voice channel from the cache and, when exactly one
non-bot member is in it, records that member's name for the source and logs "assigned as the
only person in the channel". A later speaking event with a real mapping replaces it.

**D8. Commands.** On `ready`, register the three commands globally and, when the guild id is
set, also in that guild; each registration's result is logged.

**D9. Start at launch.** `DiscordSettings::auto_connect` is removed; serde drops the field
from older files at the next save. `Runtime::start` connects whenever a token is stored;
Connect and Disconnect stay as manual controls.

**D10. Small cache.** The serenity client gets a cache configuration with no message cache
and only what names and voice channels need; the exact field names are taken from the
serenity 0.12.5 source. The private working set before and after is measured with
`scripts\measure.ps1` (release build, bot connected, nobody in the channel).

## Risks / Trade-offs

- [A join now waits for the app-server start and the thread, about 2 to 3 s on the first
  join] → the join reply is deferred as it is today; later joins reuse a running process.
- [Stopping the app-server on the last leave loses the thread] → the history carries over as
  on a model change; the first version kept the process until exit, the user asked for no
  idle Codex.
- [Averaging lowers each voice when two people talk at once] → it is the first version's
  tested behaviour; the input gain compensates.
- [Log forwarding at Info could be chatty] → the same filters as the file, and the ring
  buffer bounds memory.

## Migration Plan

Older settings files lose `discord.auto_connect` at the next save; nothing else migrates.
