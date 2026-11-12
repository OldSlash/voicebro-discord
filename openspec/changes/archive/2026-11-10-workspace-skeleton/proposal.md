# Proposal

## Why

Every later change (Discord adapter, wake engine, gate, Codex assistant, UI) needs the same
foundation: a Cargo workspace with a core crate that has no Tauri dependency, a desktop app
that starts, logs to a file in every build, keeps settings in one schema and the Discord
token in the Windows Credential Manager. Built piecemeal, this foundation goes wrong in known
ways: a setting that touches seven places, a token in plain JSON, release builds that write
no log, an installed build that cannot be told from the previous one. This change lays the
foundation once, as `docs/ARCHITECTURE.md` describes it, before any feature is built on it.

## What Changes

- Extend the workspace from `probe-external-apis` with `crates/voicebro-core` and
  `apps/desktop` (Tauri 2).
- `voicebro-core`: the settings schema with validation, migration and atomic file store; the
  `SecretStore` trait with the Credential Manager implementation and an in-memory one for
  tests; the `AppEvent` bus; an empty `Runtime` with start, stop and mute toggle.
- `apps/desktop`: Tauri shell with the Common-Controls manifest embedded in every target, a
  build stamp, `tauri-plugin-log` in every build, thin commands, the event bridge, and a
  static three-tab UI shell (Status, Settings, Chat) that shows the build stamp and the log.
- `AGENTS.md` gets the build, test, lint and single-test commands.

## Capabilities

### New Capabilities

- `app-settings`: one settings document for the whole app, validated in one place, stored
  atomically in the user's local data directory, migrated across schema versions, and applied
  at once when saved.
- `secret-storage`: secrets (the Discord bot token) kept in the operating system's credential
  store, never in files, snapshots, events or logs; the UI can set and clear a secret and
  learns only whether it is set.
- `diagnostic-logging`: a rotating file log in every build, release included, that starts with
  the build identity and records the values the app actually used.

### Modified Capabilities

None.

## Impact

- New crates and files: `crates/voicebro-core/`, `apps/desktop/` (manifest, `build.rs`,
  `tauri.conf.json`, capabilities, `ui/`), workspace `Cargo.toml` updated.
- New dependencies: `tauri` 2.12, `tauri-build`, `tauri-plugin-log` 2.10, `serde`,
  `serde_json`, `thiserror`, `log`, `windows-sys` 0.61 (`Win32_Security_Credentials`).
- `settings.json` and `logs\` are created next to the executable on first run; the app is
  portable and writes nothing under the user profile.
- `AGENTS.md` Commands section filled; `docs/ARCHITECTURE.md` updated only if the skeleton
  deviates from it.
