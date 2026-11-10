# Tasks

## 1. Setting (spec `app-settings`)

- [x] 1.1 Create the branch `change/codex-handoff-mode` from the tip of `change/memory-measurement` and add it to the chain in `AGENTS.md` (Git flow); verify `git log -1` shows `67ff97f` as the parent and the chain lists the change last
- [x] 1.2 Add `codex.handoff_mode` (`items` default, `commentary`, `bem_tags`, `thinking`) with validation naming the field and the four values, per design D2; update `docs/ARCHITECTURE.md` 4.6 (settings schema); verify unit tests: each value is valid, `verbatim` is rejected with the field and the values in the message, a `codex` section without the field loads as `items` and the next save writes it

## 2. The call (spec `codex-assistant`)

- [x] 2.1 Carry the mode in `CodexConfig` and map it in `realtime_start_params` per design D1; verify unit tests of the start parameters for all four values: `items` sends `codexResponsesAsItems: true` and no handoff mode, `commentary` and `bem_tags` send `codexResponseHandoffMode` `commentary` and `bemTags`, `thinking` sends neither field
- [x] 2.2 Retry a rejected start once without the handoff fields and log the requested mode, the mode used and Codex's error through `log_used` (design D4), before the existing retry without history; verify a unit test with the Node fake app-server taught to reject the field: the call starts, and the log line names both modes
- [x] 2.3 Name the mode in the call start line of the log and the chat (design D5); update `docs/ARCHITECTURE.md` 4.5 (realtime start parameters, the fallback, the start line); verify a unit test of the line and the documented parameters match the mapping
- [x] 2.4 Let `live_call_when_enabled` take `VOICEBRO_CODEX_HANDOFF_MODE` and wait after a delegated question for the task to finish and the voice's retelling (design D7); document the variable in `AGENTS.md`; verify one live call per mode with the rope-gun question: the chat shows the start line with the mode, the agent's answer and the retelling (four calls through the setting, each start line named its mode, e.g. "answers: conversation items"; the retellings kept the agent's place in all four, `bem_tags` added a detail of its own, and `items` and `commentary` again claimed to show the link on the screen)

## 3. Settings tab (spec `settings-ui`)

- [x] 3.1 Add the "Answer handoff" list to the Codex card with the labels and hints of design D6, the hint of the chosen value under the list, saved on change; add the field, its default and its validation to the `ui-dev` mock; update `docs/ARCHITECTURE.md` 4.8; verify with the mock: the list shows "Conversation items (recommended)" and its hint on a document without the field, picking "Commentary" changes the hint and saves `commentary`, the stored value survives a reload (the mock keeps its document in memory, so it starts with the default the app fills in and a reload resets it; the reload is checked in the running app in 3.2. In the mock the choice showed "Saved", stayed through 6 s of events, and each hint followed the choice)
- [x] 3.2 Check the list in the running app with a scratch data dir through the WebView2 debugging port; verify `settings.json` holds the chosen value and the next live call's start line names it (the real channel is the user's step) (an older settings file showed "Conversation items (recommended)"; choosing "Tagged channels" saved `bem_tags` to `settings.json` and survived a restart; a call needs a voice channel, so the start line was checked through the setting in the live test of 2.4)

## 4. Checks

- [x] 4.1 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output (fmt clean; clippy no errors, the same 6 warnings in code this change does not touch; tests: core 140, desktop 6, all pass)

## Workflow follow-up

- Checkpoint commits per task group on `change/codex-handoff-mode`, with the user's OK.
- Hearing the retelling in a real channel with people and a game is the user's step.
- `docs/HANDOFF-MODES-REPORT.md` stays uncommitted until the user decides about it.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
