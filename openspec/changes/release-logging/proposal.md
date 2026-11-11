# Proposal

## Why

The release build writes what was collected for debugging into files that nobody asked
for and that keep growing. The release log of 2026-11-10 has 1975 lines, about 1100 of
them third-party noise from the voice transport; every final recognition result of every
person in the channel is written as `[wake] … heard: …`; every chat entry goes into a daily
`logs\codex-chat-<date>.log` that is never pruned and again into `voicebro.log`. And while
the Codex app-server is down, every status read starts `codex --version`, every 5 s. The
user decided on 2026-11-11 that none of this accumulates in a release: what the app does
stays visible in the window (chat and console), the file log keeps only what is needed to
diagnose a problem, and chat history goes to disk only when the user turns it on.

## What Changes

- **A lean file log in release builds.** The file keeps the app's event lines (start and
  stop, connections, joins and leaves, sessions, downloads, settings applied) and errors.
  Detail lines (recognized speech, per-turn latency, call and delivery counters, speaker
  mapping, copies of chat entries, third-party warnings and the voice receiver's
  per-packet errors) go to the window only. The file rotates at 1 MB with one old file
  kept, so the logs folder holds at most about 2 MB.
- **A "Verbose log" switch** in the Diagnostics card (`diagnostics.verbose_log`, off by
  default, kept across restarts, applied at once) writes the detail lines to the file too,
  for diagnosing a problem. A debug build always writes them.
- **Chat history on disk only when wanted.** A "Save chat history" switch in the Codex card
  (`assistant.save_chat_history`, off by default, applied at once). While it is off, no
  chat file is written; the Codex chat tab still shows every entry. Files written before
  stay where they are.
- **No process starts for a status read.** The Codex version is read once per executable
  and again only when the executable path or file changes.
- **The window's third-party level is fixed.** The WebRTC sub-crates (`rtc_ice`,
  `rtc_dtls`, `rtc_sctp`, `rtc_srtp` and the others) reach the window at Warn as the spec
  already says; today they log at Info.
- Unchanged: the Performance and Memory switches (off by default, nothing runs while off,
  kept across restarts since change `settings-help-and-optional-diagnostics`); the window's
  Console and Codex chat tab.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `diagnostic-logging`: what a release file log holds, the detail lines, the verbose
  switch, rotation at 1 MB with one old file; the window still receives every line.
- `assistant-chat-log`: the daily file is written only while saving chat history is on.
- `app-settings`: `diagnostics.verbose_log` and `assistant.save_chat_history`.
- `settings-ui`: the two switches, their cards and help lines.
- `codex-assistant`: a status read starts no process.

## Impact

- `apps/desktop/src/main.rs` (`log_plugin`: a filter on the file target, the rotation
  constants, the `rtc_*` levels), a shared verbose flag set from the settings.
- `crates/voicebro-core`: the detail lines marked so the file filter can tell them apart
  (`wake/worker.rs`, `latency.rs`, `audio/output.rs`, `assistant/codex/rtc.rs`,
  `discord/delivery.rs`, `discord/session.rs`, `discord/receive.rs`,
  `assistant/chat_log.rs`), `assistant/chat_log.rs` (the file only while saving is on),
  `assistant/codex/app_server.rs` and `codex/mod.rs` (the cached version),
  `settings/mod.rs` and `runtime.rs` (the two fields, applied at once).
- `apps/desktop/ui/index.html`, `main.js` (two switches with help lines),
  `apps/desktop/ui-dev/mock.js`.
- Docs: `docs/ARCHITECTURE.md` 1 and 7 (what the release log holds), `AGENTS.md` (the
  settings line for measurements gains the verbose switch where a test needs the detail
  lines), `LOCAL-TESTING.md` (the corpus and call tests read detail lines only with the
  switch on).
- No new dependencies.
