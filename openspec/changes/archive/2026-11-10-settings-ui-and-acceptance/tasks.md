# Tasks

## 1. Settings tab (spec `settings-ui`)

- [x] 1.1 Add `wake_models` command (catalog entries) and reuse `codex_models` and the voices from the status; verify both return in the webview console (the lists come from `wake_models` and `capabilities`; the webview console was not opened, the user's step)
- [x] 1.2 Build the Settings tab with a control per field, per-control patch save, inline validation messages, lower-cased phrase display and unknown-word list; verify in the running app: an invalid timeout shows its message, a valid one persists across restart, a phrase overlap names the phrase (the validation itself is unit-tested in the core; the clicks in the window are the user's step)
- [x] 1.3 Model, effort and voice controls as selects from the live lists with text fallback, plus the note about delegated work; verify with Codex running (lists) and stopped (text fallback) (built; not clicked through, the user's step)

## 2. Status tab (spec `status-view`)

- [x] 2.1 Render every status field from one snapshot: Discord, voice channels, assistant and call state, wake state with progress and unknown words, hotkey, errors, stamp; buttons Mute, Connect/Disconnect, Sign in, Check; counters under details; refresh on events and every 5 s; verify in the running app while joining a channel and toggling mute from Discord (built; the live check in a channel is the user's step)

## 3. Chat tab

- [x] 3.1 Finish the Chat tab per `assistant-chat-log` if the Codex change left anything out (filters, auto-scroll, copy, open folder); verify after a live call (complete from the Codex change; the live call in the app is the user's step)

## 4. Acceptance

- [x] 4.1 Add `scripts\measure.ps1` (private bytes per process, receive-path CPU) and run it for idle always-on, idle wake-word, during a call, each three times; verify the numbers are written into `docs/MVP-PLAN.md` (idle always and idle wake-word done three times each; during a call needs a person in the channel, the user's step)
- [x] 4.2 Compare the receive-path CPU between `opt-level = 3` and `z` once; verify the decision and numbers are written into the plan's release profile section (239 µs versus 253 µs per tick; `3` stays)
- [x] 4.3 Run the acceptance checklist (mute, wake word on noisy recordings, Codex canary and continuity, app lifecycle) and record pass/fail per item with what was not tested; verify every item has a result
- [x] 4.4 Real-channel session with two or more people and a game running (the user's step; the agent prepares the build and reads the log afterwards); verify the log shows the session and the results are recorded in the plan

  Closed by the user's decision of 2026-11-10 at the merge into `main` (every remaining check counts as done); not verified separately as a task.
- [x] 4.5 Bump the version to 0.2.0 in `apps/desktop/Cargo.toml` and `tauri.conf.json`; verify the stamp in the UI and the log shows 0.2.0 (the log shows `VoiceBro 0.2.0+b5baf5d.dirty 2026-11-09`)

## 5. Close-out

- [x] 5.1 Update `docs/ARCHITECTURE.md` 4.8 and the plan, `AGENTS.md`, `LOCAL-TESTING.md`; verify no stale statement remains
- [x] 5.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass

## Workflow follow-up

- Checkpoint commit per task group on `change/settings-ui-and-acceptance`.
- Merge into `main` with `--no-ff --log`, tag `v0.2.0`, archive the change when the user says so.
