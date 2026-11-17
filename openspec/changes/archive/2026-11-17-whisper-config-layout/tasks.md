# Tasks

The branch is `change/whisper-config-layout`, started from the tip of `change/whisper-mode`
(this change reworks that change's window and is merged after it).

## 1. Core: phrases and downloads in both modes

- [x] 1.1 Make the Whisper phrases default to "bro" and "peace out" whatever `whisper.language` is (D2). Verify with a settings test: no `whisper.phrases` and language `ru` gives "bro" / "peace out", and a stored list keeps its value. Run `cargo test -p voicebro-core settings::`
- [x] 1.2 Add `missing` to `WhisperStatus` and `TtsStatus`, computed by `configure` in both modes without the network (D1). Verify with manager tests: in the realtime mode the state is `Off` and `missing` lists the runtime, the model and the VAD model (Whisper) or the engine and the voice (voice); the status serialization test names the new field
- [x] 1.3 Accept `download` in either mode when the selected item has missing files, and load nothing in the realtime mode (D1). Verify with manager tests: a download in the realtime mode installs the files, ends `Off` with an empty `missing`, and loads no model or voice; a download for another model or voice is refused
- [x] 1.4 Point the refused join's reply to Config → Assistant and Local voice (`JOIN_NEEDS_DOWNLOADS`), and update its runtime test. Verify with `cargo test -p voicebro-core runtime::`

## 2. Commands

- [x] 2.1 Split the download command: `whisper_download(model)` for Whisper's files and a new `tts_download(voice)` for the voice's (D5). Verify `cargo build -p voicebro-desktop`

## 3. The window

- [x] 3.1 Rework Config → Assistant (D3): the assistant mode first; the grey Recognition field; the wake model row in the realtime mode and the Whisper model row, with its notice and Download, in the Whisper mode; the phrase fields of the mode shown; Activation below. Verify in the command mock: each mode shows its rows, and each field saves its own path
- [x] 3.2 Add the Config → Local voice card (D4): the mode note, the voice, the speed, Preview and the voice's notice with Download. Verify in the mock in both modes: the note changes with the mode, and Download works in the realtime mode
- [x] 3.3 Add the Settings → Whisper card (D4): the On/Off label, device, CPU threads, language, the CUDA runtime's notice under the device when NVIDIA GPU is chosen, the in-use line, Open Whisper models folder. Verify in the mock with `?gpu_fallback` and with the GPU chosen on a PC without the CUDA runtime
- [x] 3.4 Remove the Whisper tab and its cards, and move the install dialog's triggers and texts to the new places (D5). Verify in the mock: the tab bar ends with Config; a refused join, a switch to Whisper with files missing and a model choice in the Whisper mode open the dialog; a choice in the realtime mode does not
- [x] 3.5 Update `ui-dev/mock.js` (`missing` in both modes, `tts_download`), turn `check-whisper-tab.mjs` into `check-whisper-config.mjs` for the `settings-ui` and `app-window` delta scenarios, and update `check-config-tab.mjs` and `check-wake-dialog.mjs`. Verify that all three checks pass, and that one new scenario fails when broken on purpose

## 4. Docs

- [x] 4.1 Update `docs/ARCHITECTURE.md` (4.8 the window, 4.12 the managers' `missing`), `docs/user-guide.md` section 7, `AGENTS.md` (the window's tabs, the mock, the checks) and `README.md` where it names the Whisper tab. Verify that no doc still names the Whisper tab, with `grep -rn "Whisper tab" docs README.md AGENTS.md`

## 5. Checks

- [x] 5.1 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` (CMake on PATH), and the three headless checks. Say in the report what each returned
- [x] 5.2 Start the release build with the test assets (`VOICEBRO_DATA_DIR` = `target\whisper-test`) and the WebView2 debugging port. Take screenshots of Config → Assistant and Local voice in both modes, and of Settings → Whisper, into `target\whisper-config-shots\`. Check that Download under the Whisper model works in the realtime mode against an empty scratch data folder with a small model (tiny, 44 MB)
- [x] 5.3 Report what was built, decided and measured, and what was not tested: a real Discord channel with two or more people and a game running

The user tested the layout in a Discord channel on 2026-11-17 and asked for one fix from
that test to go into this change: group 6.

## 6. The repetition filter (added on 2026-11-17)

- [x] 6.1 Make `clean` take the utterance's speech length and drop a run of one word as a hallucination only when it is at least four words long and faster than five words a second (D7); log the repeats and the length on a drop; pass the length from the transcriber, the probe (`whisper`, `whisper-corpus` and its noise clips) and the Whisper text-turn test. Verify with transcriber tests: "Bro, bro, bro, bro." in 1.5 s and in 0.8 s is kept, in 0.6 s is dropped, twenty repeats in 2 s are dropped wherever they are in the text. Run `cargo test -p voicebro-core stt::transcriber`
- [x] 6.2 Update `docs/ARCHITECTURE.md` (the transcriber's guards, the decoding table, the log line) and `docs/DECISIONS-2026-11-17.md`. Verify that no doc still says four repeats alone make a hallucination
- [x] 6.3 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` and `openspec validate whisper-config-layout --strict`, and say in the report what each returned and what was not tested: the filter in a live Discord channel

- [x] 6.4 Before the commit, check the join and the leave without Discord with a gated live test (`join_and_leave_unload_when_enabled` in `stt/manager.rs`): three cycles of load, four utterances and leave, on the GPU and on the CPU (large-v3-turbo), with the process's CPU, private working set, threads and GPU memory recorded every second. 2026-11-17: Whisper and the voice unload 0.5 s after the leave on the GPU; after the unload the CPU is at 0 and nothing grows over three cycles. Open: on the CPU the utterance being transcribed at the leave runs to its end (8 to 12 s at half the machine's CPU) before the unload; on the GPU the process keeps 251 MB of GPU memory and about 255 MB more RAM than on the CPU after the unload, until the app is closed. Both fixed by group 7

## 7. The leave frees everything (added on 2026-11-17)

- [x] 7.1 Stop the transcription in progress at a leave or an unload (D8): `AbortFlag` and `SpeechToText::transcribe_until`, whisper.cpp's `abort_callback` and `encoder_begin_callback`, the queue's guild in progress. Verify with a transcriber test: a leave of the utterance's guild or a close frees the model at once and gives no text, another guild's leave does not stop it. Run `cargo test -p voicebro-core stt::`
- [x] 7.2 Reset the CUDA context after a model on the GPU unloads, with the busy lock held (D9). Verify with a manager test: one reset after a GPU model's unload and one at a switch to the realtime mode, none after a CPU model's
- [x] 7.3 Update `docs/ARCHITECTURE.md` (the unload, the transcriber, the table, the log lines), `AGENTS.md` and `docs/DECISIONS-2026-11-17.md`
- [x] 7.4 Run the check of task 6.4 again on the GPU and the CPU, and run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` and `openspec validate whisper-config-layout --strict`. Pass: on the GPU the process holds no GPU memory after each unload and each cycle transcribes its four utterances; on the CPU the transcription in progress stops within one encoder pass and the CPU is idle after the unload. 2026-11-17: on the GPU 0.3 MB of GPU memory after each unload (251.5 MB before the fix), RAM 156 to 160 MB (275 to 278 MB before; the CUDA libraries stay loaded), four utterances in each cycle after each reset; on the CPU the model unloads 2.7 to 3.0 s after the leave (8 to 12 s before), no text after the leave, then CPU 0 and 19 to 20 MB

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs and this change archived; the merge after `change/whisper-mode`.
