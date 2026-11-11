# Design

## Context

See proposal.md for the motivation. What the code does today:

- `apps/desktop/src/main.rs::log_plugin` sets up `tauri-plugin-log` with two targets: a
  `Folder` target (`logs\voicebro.log`, `max_file_size` 4,000,000, `KeepSome(2)`) and a
  `Dispatch` target that pushes each record into the 500-line ring the window reads; in a
  debug build also `Stdout`. Levels are global: Info, with `level_for` Warn for a list of
  chatty crates (`CHATTY_CRATES`) and Error for `songbird::driver::tasks::udp_rx`. The list
  names `rtc` but not the WebRTC sub-crates (`rtc-ice`, `rtc-dtls`, `rtc-sctp`, `rtc-srtp`,
  `rtc-rtp`, `rtc-rtcp`, `rtc-media`, `rtc-mdns`, `rtc-turn`, `rtc-stun`, … in
  `Cargo.lock`), so they log at Info. Both targets get the same records.
- `tauri-plugin-log` 2.10.1 has `Target::filter(Fn(&log::Metadata) -> bool)` per target,
  and `KeepSome(n)` keeps `n` archived files besides the active one.
- Detail lines today, all at Info: `[wake] … heard:` (`wake/worker.rs`), `[latency:…]`
  (`latency.rs`, `audio/output.rs`, `gate.rs`), `[rtc]` buffer, remote track and the
  end-of-call counters (`assistant/codex/rtc.rs`), `[delivery:…]`
  (`discord/delivery.rs`, `discord/session.rs`), the speaker names with their SSRC
  (`discord/receive.rs`), and the copy of every chat entry `[chat:…]`
  (`assistant/chat_log.rs`).
- `ChatLog::record` pushes to memory, publishes the event, logs the copy and appends to
  `logs\codex-chat-<date>.log` unconditionally.
- `CodexAssistant::status` calls `codex_version(path)` (a `codex --version` process)
  whenever the app-server is not running; the window reads the status every 5 s (every
  1 s with performance monitoring on). `system_status` runs `cmd /C ver` only when the
  System tab opens and at start, so it stays.
- The desktop shell already reacts to `AppEvent::SettingsChanged` in its event bridge (the
  hotkey, the memory reader). `Runtime::apply` sends the `assistant` section to
  `self.assistant.configure`.

## Goals / Non-Goals

**Goals:**

- A release log that a user can send with a bug report: small, bounded, without what other
  people said, but with every event and error.
- The window unchanged in what it shows.
- No periodic process for a status read.

**Non-Goals:**

- Removing the per-turn latency and the audio and delivery counters themselves: they are
  atomics, cost nothing measurable and feed the Performance tab and the Opus encoder.
- Pruning or deleting chat history files written before this change.
- Rewriting paths in log lines (the Windows user name in the data directory path).
- Any change to the Performance and Memory switches.

## Decisions

### D1. Detail lines carry their own log target

A macro in the core, `detail!(...)`, logs at Info with the target `detail::<module>`
(`detail::wake`, `detail::latency`, `detail::rtc`, `detail::delivery`,
`detail::receive`, `detail::chat`). Every line listed in Context moves to it; the event
lines next to them (connection state, call start and end, phrase decisions, dropped audio,
downloads) stay on `log::info!`. The window shows a detail line like any other.

Alternative considered: Debug level for detail lines. Rejected: the window must keep
showing them (spec "The window receives the app log"), and Debug would also let through
every crate's debug output once the level is lowered for the verbose log.

### D2. Levels by default for third parties, a filter on the file target

The global level becomes Warn, with `level_for` Info for this app's own targets
(`voicebro`, `voicebro_core`, `voicebro_desktop`, `detail`), and Error for
`songbird::driver::tasks::udp_rx` as today. Any third-party crate, the WebRTC sub-crates
included, is then at Warn without a list to keep up to date; `CHATTY_CRATES` goes.

The file target gets a filter:

```text
keep in file = debug build
            or VERBOSE (an AtomicBool)
            or (app target, not detail::*)
            or (third-party target, level Error, not songbird::driver::tasks::udp_rx*)
```

`VERBOSE` is set from `settings.diagnostics.verbose_log` at start and on every
`SettingsChanged` in the event bridge, next to the hotkey and the memory reader. The
`Dispatch` target to the window has no filter.

Rotation: `LOG_FILE_LIMIT` 1,000,000 bytes and `KeepSome(1)`: the active file plus one
archived, about 2 MB at most.

### D3. Chat history behind a switch in the chat log

`ChatLog` gets `save: AtomicBool`, set from `settings.assistant.save_chat_history` when the
assistant is created and in `CodexAssistant::configure` (already called for the
`assistant` section). `record` appends to the daily file only while it is set; the copy
into the app log becomes a `detail!` line. Nothing reads or deletes the old files.

### D4. The Codex version is cached by file identity

`CodexAssistant` keeps `version_cache: Mutex<Option<(PathBuf, u64, SystemTime, String)>>`.
`status()` resolves the executable as today (path checks only, no process), reads its
metadata and runs `codex --version` only when the path, size or modification time differs
from the cache. A running app-server keeps reporting its own version as today. Change
`codex-onboarding` relies on this to show a freshly installed Codex.

### D5. Settings and the window

- `DiagnosticsSettings` gains `verbose_log: bool`, `AssistantSettings` gains
  `save_chat_history: bool`, both `#[serde(default)]`; no schema version bump (missing
  fields take their defaults, as for `diagnostics` before). The `diagnostics` branch of
  `Runtime::apply` logs the three switches.
- The Diagnostics card gets a third switch, "Verbose log"; the Codex card gets "Save chat
  history" after Speaker names. Both use the existing switch markup with `data-path`, their
  own help line and the saved mark.
- `apps/desktop/ui-dev/mock.js` gets both fields and their type validation.

## Risks / Trade-offs

- [A user's report lacks the detail needed] → the verbose switch, described in its help
  line and in the README's troubleshooting part (change `release-packaging`).
- [A detail line is missed and keeps flooding the file] → the release-build test (task
  group 5) runs a session and counts the file's lines by target.
- [Turning the verbose log on writes what people said into the file] → the help line says
  so; it is off by default and meant for diagnosis.
- [Global Warn hides an Info line of a third-party crate someone relied on] → nothing in
  the docs or tests relies on one; serenity's and songbird's were already at Warn.

## Migration Plan

Settings files without the two fields load with both off: after the update nothing new is
written to `codex-chat-*.log` until the user turns the switch on. Existing chat files and
logs stay; the next rotation trims `voicebro*.log` to the new limit.
