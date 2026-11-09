# Tasks

## 1. Settings tab (spec `settings-ui`)

- [ ] 1.1 Add `wake_models` command (catalog entries) and reuse `codex_models` and the voices from the status; verify both return in the webview console
- [ ] 1.2 Build the Settings tab with a control per field, per-control patch save, inline validation messages, lower-cased phrase display and unknown-word list; verify in the running app: an invalid timeout shows its message, a valid one persists across restart, a phrase overlap names the phrase
- [ ] 1.3 Model, effort and voice controls as selects from the live lists with text fallback, plus the note about delegated work; verify with Codex running (lists) and stopped (text fallback)

## 2. Status tab (spec `status-view`)

- [ ] 2.1 Render every status field from one snapshot: Discord, voice channels, assistant and call state, wake state with progress and unknown words, hotkey, errors, stamp; buttons Mute, Connect/Disconnect, Sign in, Check; counters under details; refresh on events and every 5 s; verify in the running app while joining a channel and toggling mute from Discord

## 3. Chat tab

- [ ] 3.1 Finish the Chat tab per `assistant-chat-log` if the Codex change left anything out (filters, auto-scroll, copy, open folder); verify after a live call

## 4. Acceptance

- [ ] 4.1 Add `scripts\measure.ps1` (private bytes per process, receive-path CPU) and run it for idle always-on, idle wake-word, during a call, each three times; verify the numbers are written into `docs/MVP-PLAN.md`
- [ ] 4.2 Compare the receive-path CPU between `opt-level = 3` and `z` once; verify the decision and numbers are written into the plan's release profile section
- [ ] 4.3 Run the acceptance checklist (mute, wake word on noisy recordings, Codex canary and continuity, app lifecycle) and record pass/fail per item with what was not tested; verify every item has a result
- [ ] 4.4 Real-channel session with two or more people and a game running (the user's step; the agent prepares the build and reads the log afterwards); verify the log shows the session and the results are recorded in the plan
- [ ] 4.5 Bump the version to 0.2.0 in `apps/desktop/Cargo.toml` and `tauri.conf.json`; verify the stamp in the UI and the log shows 0.2.0

## 5. Close-out

- [ ] 5.1 Update `docs/ARCHITECTURE.md` 4.8 and the plan, `AGENTS.md`, `LOCAL-TESTING.md`; verify no stale statement remains
- [ ] 5.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass

## Workflow follow-up

- Checkpoint commit per task group on `change/settings-ui-and-acceptance`.
- Merge into `main` with `--no-ff --log`, tag `v0.2.0`, archive the change when the user says so.
