# Tasks

The branch is `change/models-in-settings`, from `main`. It is the first of the four changes
the user asked for on 2026-11-20 (`models-in-settings`, `claude-notice-hide`,
`text-answers-mode`, `interrupt-on-speech`); each next one starts from `main` after the
previous one is merged.

## 1. Core: the join refusals

- [x] 1.1 Refuse a `/join` in the realtime mode with the wake word while the selected wake model or the Vosk library is missing, with `JOIN_NEEDS_WAKE_FILES` ("The wake word needs downloads: …"), checked in `Runtime::prepare_join` before the bot enters the channel (D5). Verify with runtime tests: a scratch folder without Vosk files refuses a wake-word join and downloads nothing; always listening and the Whisper mode are not refused for Vosk. Run `cargo test -p voicebro-core runtime::`
- [x] 1.2 Point `JOIN_NEEDS_DOWNLOADS` to Settings → Whisper and Local voice (D5) and update its tests. Verify with `cargo test -p voicebro-core runtime::` and `cargo test -p voicebro-core stt::`
- [x] 1.3 Update `docs/ARCHITECTURE.md` (the join's preparation and its two refusals). Verify that the section names both reply texts as the code has them

## 2. The window: cards and notices

- [x] 2.1 Move the Local voice card to the Settings tab after the Whisper card, and the Whisper model row into the Whisper card before the device (D1); update both tabs' autosave lines. Verify in the command mock: Settings shows Appearance, Diagnostics, Whisper, Local voice; Config shows no Local voice card and no Whisper model; each moved field still saves its own path
- [x] 2.2 Replace the Whisper card's two notices and the Assistant card's Whisper notice with one notice in the Whisper card, drawn from `whisper.missing` in both modes (D2). Verify in the mock with `?whisper_missing`, with the realtime mode and nothing downloaded, and with the GPU chosen without the CUDA runtime
- [x] 2.3 Save a wake model, Whisper model, device or voice at once in both modes, with no dialog (D3). Verify in the mock: picking a missing model saves it and shows its notice; Download in the notice downloads it
- [x] 2.4 Add the Config → Assistant note of the Whisper mode with "Open Settings" (D6). Verify in the mock: the note names what is missing, the button shows the right card expanded and in view, and the note goes after `completeWhisperDownload`

## 3. The window: the dialog only at a refused join

- [x] 3.1 Open the install dialog only on a refused `/join`, for the wake word and for the Whisper mode, and remove the other triggers and the subscription dialog's `whisperNext` (D4, D7). Verify in the mock: a switch of the mode or the activation, an engine choice and a model choice open no dialog; a refused join opens it with the missing files
- [x] 3.2 Make the mock refuse `join()` in the realtime mode with the wake word while the Vosk files are missing, with the same reply text as the core and its log event. Verify that `join()` in that state leaves the bot out and logs the reply
- [x] 3.3 Update `check-wake-dialog.mjs`, `check-config-tab.mjs`, `check-whisper-config.mjs` and `check-engine-config.mjs` for the delta scenarios of `settings-ui` and `app-window`. Verify that all four pass, and that one new scenario fails when broken on purpose
- [x] 3.4 Update `docs/ARCHITECTURE.md` (the window), `docs/user-guide.md` (where each thing is downloaded) and `AGENTS.md` (the window's cards, the mock's refused join, the checks). Verify with a search that no doc places the Whisper model or the Local voice card on the Config tab

## 4. Checks

- [x] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH), the four headless checks and `openspec validate models-in-settings --strict`. Say in the report what each returned
- [x] 4.2 Start the release build with a scratch data folder and the WebView2 debugging port, and take screenshots of Settings (all four cards) and of Config → Assistant in both modes into `target\models-in-settings-shots\`. 2026-11-20: Settings shows Appearance, Diagnostics, Whisper, Local voice; the Whisper card lists the CPU runtime, base (q8_0) and the VAD model (92 MB); Config shows six cards and no Local voice; a switch to Whisper opened no dialog and showed the note naming Whisper's files and the voice; Open Settings showed the Settings tab; the log has no error
- [x] 4.3 Report what was built and what was not tested: a refused `/join` run by a person in Discord, and the app in a real Discord channel with two or more people and a game running

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs and this change archived; then the merge into `main`.
