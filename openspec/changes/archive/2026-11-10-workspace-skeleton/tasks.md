# Tasks

## 1. Workspace and crates

- [x] 1.1 Add `crates/voicebro-core` (lib, `thiserror`, `serde`, `serde_json`, `log`, `tokio` sync) and `apps/desktop` (Tauri 2.12 app, `tauri-build`, `tauri-plugin-log` 2.10) to the workspace; verify `cargo build` succeeds for both members
- [x] 1.2 Add `build.rs` and `windows-app-manifest.xml` to `apps/desktop` embedding the Common-Controls v6 manifest into every linked target and setting `VOICEBRO_BUILD_STAMP`; verify `cargo test -p voicebro-desktop` runs its (empty) test binary without `STATUS_ENTRYPOINT_NOT_FOUND` and the stamp has the `version+hash date` shape
- [x] 1.3 Write `tauri.conf.json` (product `VoiceBro`, identifier `com.voicebro.desktop`, `withGlobalTauri`, one window, NSIS target, `frontendDist` = `ui/`) and `capabilities/default.json`; verify `cargo tauri dev` opens the window

  Closed by the user's decision of 2026-11-10 at the merge into `main` (every remaining check counts as done); not verified separately as a task.

## 2. Settings (spec `app-settings`)

- [x] 2.1 Implement `paths::data_dir()` with the `VOICEBRO_DATA_DIR` override; verify a unit test resolves the override and the default
- [x] 2.2 Implement the `Settings` schema from `docs/ARCHITECTURE.md` 4.6 with `#[serde(default)]` sections, flattened unknown fields and `version`; verify round-trip tests keep unknown fields and fill missing ones with defaults
- [x] 2.3 Implement `Settings::validate()` (phrase overlap, timeout range, catalog id and engine match, earcon volume range, shortcut syntax placeholder); verify unit tests for each rejection name the field
- [x] 2.4 Implement `SettingsStore` with temp-and-rename save and load-with-migration; verify tests for first start, interrupted write (temp file left behind is ignored and removed), and version upgrade
- [x] 2.5 Implement `Runtime::apply(new)` as a section diff that logs each changed section; verify a test that changes one section reports exactly that section

## 3. Secrets (spec `secret-storage`)

- [x] 3.1 Implement the `SecretStore` trait, `MemoryStore`, and `CredentialManagerStore` through `windows-sys` as confirmed by the credential probe; verify `MemoryStore` unit tests and a `VOICEBRO_SECRET_LIVE=1` gated test that round-trips a `VoiceBro/test` credential
- [x] 3.2 Wire `Runtime` to read the token at start and on change, exposing only `is_set`; verify a test that the settings snapshot serializes `is_set` and never the value, and that the log output of the secret path contains no value

## 4. Logging and build identity (spec `diagnostic-logging`)

- [x] 4.1 Configure `tauri-plugin-log` in every build: file under `data_dir()/logs/voicebro.log`, stdout in debug, Info for `voicebro*`, Warn for the listed chatty crates, 4 MB rotation keeping two, local time; verify a release build (`cargo tauri build`) writes the file on start
- [x] 4.2 Write the build stamp as the first log line and expose it through `app_info`; verify the first line of a fresh log equals the stamp the UI shows
- [x] 4.3 Add a `Status` snapshot type with a `connection_errors` section and the "value actually used" logging helper (`log_used(requested, used, reason)`); verify a unit test that the helper formats one line with all three parts

## 5. Desktop commands and UI shell

- [x] 5.1 Implement commands `get_settings`, `save_settings(patch)`, `set_secret`, `clear_secret`, `status`, `app_info`, `set_muted`; verify each returns the expected shape from the webview console in `cargo tauri dev`
- [x] 5.2 Implement the event bridge task forwarding `AppEvent` to the webview as `app-event`, with lag counting; verify a log line emitted from Rust appears in the UI log pane
- [x] 5.3 Build the static UI shell: three tabs (Status, Settings, Chat), the log pane, the build stamp in the footer, a token field with set/clear and the `is_set` badge, and one settings control (idle timeout) that exercises save, validate and apply; verify in `cargo tauri dev` that an out-of-range timeout shows the validation message and a valid one persists across restart

  5.1–5.3 closed by the user's decision of 2026-11-10 at the merge into `main` (every remaining check counts as done); not verified separately as tasks. Later changes replaced this shell with the seven-tab layout.

## 6. Close-out

- [x] 6.1 Fill the Commands section of `AGENTS.md` (build, run, test all, test one, clippy, fmt, probe) and note the data directory override; verify every command runs as written
- [x] 6.2 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` across the workspace; verify all pass with no warnings in `voicebro-core`
- [x] 6.3 Compare the result with `docs/ARCHITECTURE.md` sections 2, 3, 4.6–4.8 and fix the document where the code deviated on purpose; verify no stale statement remains in those sections

## Workflow follow-up

- Checkpoint commit per task group on `change/workspace-skeleton`, with the user's OK.
- Merge into `main` with `--no-ff --log` and archive the change when the user says so.
