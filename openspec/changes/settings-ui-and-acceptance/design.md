# Design

## Context

See proposal.md. Target: `docs/ARCHITECTURE.md` 4.8 and the acceptance list in
`docs/MVP-PLAN.md` section 7. The UI is static HTML/JS over `window.__TAURI__`; the shell
has three tabs, the log pane, the stamp, the token field and the idle timeout. Commands
`get_settings`, `save_settings(patch)`, `set_secret`, `clear_secret`, `status`, `app_info`,
`set_muted` exist; the Discord and Codex changes add `discord_connect`, `discord_disconnect`,
`codex_sign_in`, `codex_check`, `codex_models`, `chat_log`, `open_log_folder`.

## Goals / Non-Goals

**Goals:** the full UI; the acceptance run; the measurements; the first tagged build.

**Non-Goals:** no framework or bundler; no new settings; no tray mode.

## Decisions

**D1. One `save_settings` patch per control.** Each control knows its JSON path
(`session.idle_timeout_seconds`); on change the UI sends `{section: {field: value}}`, shows
the error at the control on rejection, and re-reads the snapshot on success. No form-wide
save button.

**D2. Lists are filled from `codex_models` and the voices in `AssistantStatus` when present,
otherwise the control degrades to a text input** with a `datalist` of the last known values.
The wake model `select` comes from a `wake_models` command over the catalog.

**D3. The Status tab renders from one snapshot.** `renderStatus(status)` fills every field;
events only trigger a refresh; a 5 s timer backs it up. Counters live under a `<details>`.

**D4. Acceptance is a checklist in `docs/MVP-PLAN.md` filled with results**, each item with
pass/fail and what was not tested; recordings from the session become the start of the
wake-word corpus.

**D5. Measurements with one PowerShell script in `scripts\measure.ps1`** (private bytes per
process for `voicebro.exe`, `codex.exe`, WebView2; receive-path CPU) run three times per
state; results into the plan; the `opt-level` comparison decides the release profile.

**D6. Version 0.2.0 and tag.** `apps/desktop/Cargo.toml` and `tauri.conf.json` bumped
together before the merge; the tag goes on `main` after the user merges.

## Risks / Trade-offs

- [The UI cannot be exercised by the agent inside the webview] → the user clicks through the
  checklist; the agent verifies through the log and the settings file.
- [Two-person session needs scheduling] → the acceptance run is the user's step; everything
  else is done beforehand.

## Open Questions

None.
