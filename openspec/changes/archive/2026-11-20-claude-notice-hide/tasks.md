# Tasks

The branch is `change/claude-notice-hide`, from `main` after `change/models-in-settings` is
merged (its delta of the dialog's "Both dialogs" scenario comes first).

## 1. The setting

- [x] 1.1 Add `claude.subscription_notice_hidden` (boolean, default `false`) to the settings (D2). Verify with settings tests: the default, an older file without it loads `false` and the next save writes it, the string `"yes"` is refused naming the field. Run `cargo test -p voicebro-core settings::`
- [x] 1.2 Add the field to the settings table in `docs/ARCHITECTURE.md`. Verify that the table names it with its default

## 2. The window

- [x] 2.1 Replace the Assistant and Access cards' notice boxes with one box at the top of the Claude Code card, and add Hide (D1, D2). Verify in the command mock with `?engine=claude&claude_signin=subscription`: only the Claude Code card shows the notice; Hide hides it and saves the flag; a reload and a switch to Codex and back do not show it; "Hear only me" still sets the owner-only mode
- [x] 2.2 Add a mock parameter for a hidden notice (`?notice_hidden`) and the scenarios of the `settings-ui` and `claude-assistant` deltas to `check-engine-config.mjs`. Verify that the check passes, and that one new scenario fails when broken on purpose
- [x] 2.3 Update `docs/user-guide.md` (where the notice is and Hide) and `AGENTS.md` (the mock's parameter). Verify with a search that no doc still places the notice in the Assistant or Access card

## 3. Checks

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH), the four headless checks and `openspec validate claude-notice-hide --strict`. Say in the report what each returned
- [x] 3.2 Report what was built and what was not tested: the app in a real Discord channel with two or more people and a game running

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs and this change archived; then the merge into `main`.
