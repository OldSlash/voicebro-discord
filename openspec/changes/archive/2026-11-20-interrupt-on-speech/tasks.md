# Tasks

The branch is `change/interrupt-on-speech`, from `main` after `change/text-answers-mode` is
merged.

## 1. Core

- [x] 1.1 Add `assistant.interrupt_on_speech` (boolean, default `false`) and pass it to the gate's settings without a rebuild. Verify with settings tests (default, older file, the string `"yes"` refused naming the field) and a runtime test that a save of only this field rebuilds no gate. Run `cargo test -p voicebro-core settings::` and `runtime::`
- [x] 1.2 Count each speaker's voiced frames per utterance in the gate and raise `stop_answer` under the conditions of D2, once per utterance, with its log line (D1, D2). Verify with gate tests: 30 voiced frames of one speaker during an active session while working raise it once; 15 frames do not; outside a session, while muted, while not working, in the text answers mode, with one speaker at a time on, or with the setting off nothing is raised; a new utterance after a 700 ms gap can raise it again. Run `cargo test -p voicebro-core gate::`
- [x] 1.3 Add `AssistantSession::stop_answer` and implement it in the text session (D3). Verify with a text session test on the fake engine: a running turn and queued speech are stopped, the engine gets the interrupt, nothing is posted for that turn, and a question that follows starts a new turn
- [x] 1.4 Feed one speech WAV of the wake corpus and the noise clips of `whisper-corpus` through the gate as a speaker while a fake answer plays, in a gated test, and print when the stop came. Verify that speech stops the answer between 600 and 700 ms after its first voiced frame and that no noise clip stops it; write the numbers into the report. 2026-11-20 (`interrupt_timing_when_enabled`): David's and Zira's sentences stopped it 620, 680 and 620 ms after their first voiced frame; the single words "bro", "братан" and "отбой" (1.2 to 1.4 s files) stopped nothing; silence and steady noise of peak 300 stopped nothing. Not met: the game-like mix stopped it after 600 ms, because its 70 Hz hum stays above the speech threshold the whole time, and loudness and duration alone cannot tell a steady loud sound from speech (design, Risks)
- [x] 1.5 Update `docs/ARCHITECTURE.md` (the gate's count, the effect, the text session) and the Whisper mode's facts in `AGENTS.md` ("Audio facts that are easy to get wrong"). Verify that both name the 30 frames and the conditions

## 2. The window

- [x] 2.1 Add the switch "Stop the answer when someone starts talking" after "One speaker at a time" in the Assistant card, shown only in the Whisper mode, disabled with its note while one speaker at a time is on. Verify in the command mock: it shows and saves in the Whisper mode, is hidden in the realtime and text answers modes, and is disabled with the note when one speaker at a time is turned on
- [x] 2.2 Add the scenarios of the `settings-ui` delta to `check-config-tab.mjs`. Verify that the check passes, and that one new scenario fails when broken on purpose
- [x] 2.3 Update `docs/user-guide.md` (the switch, what stops the answer, what does not). Verify that the guide names the 600 ms

## 3. Checks

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH), the four headless checks and `openspec validate interrupt-on-speech --strict`. Say in the report what each returned
- [x] 3.2 Report what was built and measured, and what was not tested: the stop in a real Discord channel with two or more people and a game running, including someone without headphones

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs and this change archived; then the merge into `main`.
