# Tasks

The branch is `change/text-answers-mode`, from `main` after `change/models-in-settings` and
`change/claude-notice-hide` are merged.

## 1. Core

- [x] 1.1 Add `AssistantMode::Text` (`"text"`) with `hears_with_whisper()` and `speaks_locally()`, and replace every `== AssistantMode::Whisper` / `!= AssistantMode::Whisper` outside tests with one of them (D1). Verify with settings tests (the three values, the refusal naming them, `text` with Claude Code saved) and a search that finds no comparison left. Run `cargo test -p voicebro-core settings::`
- [x] 1.2 Keep the voice off in the text answers mode: the voice manager off with `missing` computed, the join prepared without the voice, the text session not speaking (D2). Verify with manager and runtime tests: a join in `text` with Whisper's files on disk and no voice succeeds and loads no voice; a join without Whisper's files is refused with the Whisper reply; a text session in `text` completes a turn with the fake engine and queues nothing on the player
- [x] 1.3 Post with posting off in the text answers mode (D3). Verify with a runtime test of `answer_destination`: `off` in `text` gives the voice channel, `off` in `whisper` and `realtime` gives none
- [x] 1.4 Write the latency line without the voice times in the text answers mode (D4). Verify with a `latency.rs` test of the line's text
- [x] 1.5 Name the mode "Whisper mode, text answers" in `/status` and the member panel (D5). Verify with the existing panel and status text tests extended to `text`
- [x] 1.6 Run the gated live text-turn test once in the text answers mode with Codex (`whisper_text_turn_when_enabled` with a question WAV). Verify that the answer is handed to posting and nothing is played. 2026-11-20 (`VOICEBRO_ASSISTANT_MODE=text`, David's "What is two plus two?"): Whisper base 660 ms, Codex answered "Two plus two equals four." in 7.2 s, the answer was handed to posting, the voice stayed off with nothing synthesized, and the latency line ended "no voice (text answers)"
- [x] 1.7 Update `docs/ARCHITECTURE.md` (the modes, the text session, posting, the latency line). Verify that it names the three modes and the two methods

## 2. The window

- [x] 2.1 Add "Whisper, text answers" to the mode list and treat `text` as the Whisper mode for the rows, the Recognition field and the Whisper phrases (D5). Verify in the command mock with a new `?text_answers` parameter: the rows of the Whisper mode show and save
- [x] 2.2 Follow the mode in the Local voice note, the posting help line, the Status tab's tiles, the Config note and the refused join's dialog (D5). Verify in the mock: with no voice downloaded and Whisper's files on disk, no note and no dialog; with Whisper's files missing, a refused `join()` opens the dialog listing only Whisper's files
- [x] 2.3 Add the delta scenarios of `settings-ui`, `app-window` and `status-view` to `check-whisper-config.mjs` and `check-engine-config.mjs`. Verify that both pass, and that one new scenario fails when broken on purpose
- [x] 2.4 Update `docs/user-guide.md` (the third mode, where its answers go) and `AGENTS.md` (the modes, the mock's parameter). Verify with a search that the docs name the third mode

## 3. Checks

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH), the four headless checks and `openspec validate text-answers-mode --strict`. Say in the report what each returned
- [x] 3.2 Report what was built and what was not tested: answers posted in a real Discord channel during a session with two or more people and a game running

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs and this change archived; then the merge into `main`.
