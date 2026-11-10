# Design

## Context

See proposal.md for motivation. The target is `docs/ARCHITECTURE.md` sections 2 (layout),
3 (topology, event bus), 4.6 (settings), 4.7 (runtime), 4.8 (desktop shell) and 8
(conventions). `probe-external-apis` leaves behind the workspace `Cargo.toml`, `apps/probe`
and the confirmed Credential Manager calls. Constraints:

- Tauri's test executable crashes without the Common-Controls v6 manifest; the core crate
  must not link Tauri so its tests run without it.
- The UI is static HTML/JS with `withGlobalTauri`, no bundler, as in the first version.
- Windows `std::fs::rename` replaces an existing file, so temp-and-rename works without
  `ReplaceFileW`.
- Line endings: `.gitattributes` from the probe change is already in place.

## Goals / Non-Goals

**Goals:**

- A core crate whose tests run with `cargo test -p voicebro-core` and no window.
- A desktop app that starts, logs, stores settings and the token, and shows the stamp.
- Command and event surfaces that later changes extend without restructuring.

**Non-Goals:**

- No Discord, wake, gate or assistant code; `Runtime` is a shell with start, stop and mute.
- No real Settings tab controls beyond what exercises the settings path (one or two fields);
  the full tab is `settings-ui-and-acceptance`.
- No installer build yet; `cargo tauri dev` and `cargo tauri build` must work, the NSIS
  bundle is produced but not tuned.

## Decisions

**D1. `voicebro-core` is a plain library with no Tauri dependency.** Everything the app does
lives there behind types and traits; `apps/desktop` only wires commands and events.
Alternative: one Tauri crate as in v1. Rejected: the manifest crash in tests and the
impossibility of a headless probe both come from that.

**D2. Events flow through `tokio::sync::broadcast<AppEvent>`.** The core publishes, the
desktop app has one bridge task that re-emits to the webview as one Tauri event
(`app-event` with the serialized enum). Lagged receivers drop old events and log the count.
Alternative: a callback trait injected into the core. Rejected: broadcast gives many
subscribers (UI bridge, panel refresher later, tests) for free.

**D3. Settings are a serde struct with `#[serde(default)]` on every section and
`#[serde(flatten)] extra: Map` to keep unknown fields.** Validation is one `validate()`
returning the first error with a field path. Migration is `version` plus `Default`; no
migration table until a field actually changes meaning. Alternative: a flat string map as in
v1. Rejected: seven places per setting.

**D4. Apply is a diff.** `Runtime::apply(new)` compares each section with the current
settings and calls only the affected subsystem (none exist yet in this change; the diff
and a log line per changed section are written now so later changes only add a branch).

**D5. `SecretStore` trait with two implementations.** `CredentialManagerStore` through
`windows-sys` (`CredWriteW`/`CredReadW`/`CredDeleteW`, generic type, local-machine
persistence, UTF-16 blob) as confirmed by the credential probe, and `MemoryStore` for tests. The UI gets
`set_secret(name, value)` and `clear_secret(name)`; `get_settings` returns `secrets: {
discord_token: { is_set } }`. The token value is read once at start and on change by
`Runtime`, which hands it to the Discord adapter later; nothing else sees it.

**D6. Logging through `tauri-plugin-log` configured in `apps/desktop`, `log` facade in the
core.** Targets: a file under `logs\voicebro.log` next to the executable (not the plugin's
default app-identifier folder, so every artifact of the app is in one directory) and stdout
in debug. Filters: Info for `voicebro*`, Warn for `serenity`, `songbird`, `tracing`, `hyper`,
`h2`, `rustls`, `webrtc*`, `reqwest`. Rotation 4 MB, keep two. First line: the build stamp.

**D7. Build stamp from `build.rs`.** `VOICEBRO_BUILD_STAMP` is `"<version>+<git short
hash>[.dirty] <YYYY-MM-DD>"` set with `cargo:rustc-env`; `git` is invoked by the build script
and falls back to `nogit` if unavailable. Exposed through the `app_info` command and printed
as the first log line.

**D8. Data directory from one function.** `voicebro_core::paths::data_dir()` returns
the executable's own folder (the app is portable; the installation folder holds everything),
overridable by `VOICEBRO_DATA_DIR` for tests and self-tests.

**D9. Capabilities file lists only `core:default` plus the log plugin's permissions.** The
global-shortcut permissions are added in `session-gate-and-mute`, where the hotkey lands.

## Risks / Trade-offs

- [Windows `rename` over an open file fails if another process holds it] → settings are
  written only from one place in the app; on failure the temp file is removed and the error
  surfaces in the status.
- [`broadcast` lag drops events for a slow UI] → the UI also pulls a full snapshot on demand;
  events are hints, the snapshot is truth.
- [`build.rs` calling `git` slows every build or fails on a machine without git] → cached by
  Cargo on unchanged inputs; `nogit` fallback keeps the build green.
- [Tauri CLI, plugin and core versions drift from the plan's table] → pin minors in
  `Cargo.toml`; the lock file is committed.

## Open Questions

None.
