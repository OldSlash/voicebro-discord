# Tasks

Nothing here starts before the user approves the proposal. Decisions the request left open
are in design.md, marked **[decided alone]**. Questions that come up while building are
decided the same way and added to design.md under the decision they touch, so the user can
review them all at the end.

**Downloads and tests are allowed without asking** (the user's decision of 2026-11-17).
The agent may download and test everything this change names, without asking first: the
CUDA runtime (685 MB), every Whisper model including the large and the Russian-tuned
ones, Supertonic 3, piper-phonemize, the Piper voices and Kokoro. This covers only the
agent's own tests. In the app, the user still agrees in the window before anything is
downloaded.

- Test downloads go into `target\` (for example `target\whisper-test\assets\`), never into
  `%TEMP%`.
- Every test download goes through the asset store or is checked against its pinned
  SHA-256.
- The permission does not cover installs on the system (a CUDA toolkit for an own
  whisper.cpp build, for example). Those still need the user's OK, as `AGENTS.md` says.

## 1. Branch and settings

- [x] 1.1 Create `change/whisper-mode` from `main`. Verify that `git status` shows that branch with this change's folder untracked; the folder of `voice-recognition-rework` and any other changed file are not this change's and stay out of its commit
  Done 2026-11-17: `change/whisper-mode` starts from the tip of `change/voice-recognition-rework`, not from `main`, because this change reuses its matching form and default phrases (D4); the user merges the two in order. **[decided alone]**
- [x] 1.2 Add `assistant.mode`, the `whisper` section and the `tts` section to `settings/mod.rs` (D14), with defaults, validation and `changed_sections` (`whisper`, `tts`, and `assistant` for the mode). Verify with tests for the `app-settings` delta scenarios:
  - a 1.0.1 file loads in realtime mode with the defaults;
  - an unknown mode, a speed of 3 and a `local:..\x.bin` model are refused, each naming its field;
  - "Goodbye!" in both phrase lists is refused;
  - a phrase change rebuilds no gate and no call.

  Run `cargo test -p voicebro-core settings::`
  Done: the mode's part of `changed_sections` is `mode`, not `assistant` (design D1). `cargo test -p voicebro-core settings::` passes.
- [x] 1.3 Add the matching form (D4) as one function: lower case, "ё" as "е", non-letters and non-digits as spaces, words in a row. If `voice-recognition-rework` has landed, reuse its function and its phrase field format (commas and line breaks), and start the Whisper lists with its defaults for the language (D4). Verify unit tests: "Отбой братан!" equals "отбой братан"; "подъём" equals "подъем"; a phrase with no letter or digit is refused

## 2. Shared asset store

- [x] 2.1 Move `wake/assets.rs` and `Asset` to a new `assets/` module, with the wake word unchanged. Verify that the existing asset tests and `cargo test -p voicebro-core wake::` pass, and that `node apps/desktop/ui-dev/check-wake-dialog.mjs` still passes 23 of 23
- [x] 2.2 Add `kind: Archive | File` (D10): a file is hash-checked and moved into `assets\<directory>\`, and it is its own marker. Verify tests:
  - a file downloaded from a `file:///` URL;
  - a file put into the folder by hand;
  - a wrong hash deletes the file
- [x] 2.3 Add the kept-files list for archives. Verify a test zip with five files and a list of two leaves exactly those two
- [x] 2.4 Check on this PC that the system `tar.exe` unpacks a `.tar.bz2` (make one with `tar -cjf` and unpack it in a test). If it cannot, stop and tell the user, because the fallback is a new `bzip2` + `tar` dependency. Write the result into design D10

## 3. Whisper runtime (whisper.cpp v1.9.5)

- [x] 3.1 Write `stt/whisper/ffi.rs`:
  - `#[repr(C)]` mirrors of `whisper_context_params`, `whisper_full_params`, `whisper_vad_params` and `whisper_vad_context_params`, field by field from the v1.9.5 header (fetched again from the tag);
  - the symbols;
  - the pre-load of `ggml-base.dll` (and the CUDA DLLs) by full path, then `ggml_backend_load_all_from_path`, or only the CPU variants for the CPU device (D8);
  - the `whisper_version()` check;
  - `Context` and `State` with `Drop`;
  - `whisper_log_set` into the app log, with the buffer-size lines at info.

  Raw pointers stay inside the module. Verify that `cargo build -p voicebro-core` passes and that clippy has no warning on the FFI
  Done with two changes recorded in design D8: the context's own state through `whisper_full` (the VAD runs only there), and the best CPU variant loaded by score instead of `ggml_backend_load_all_from_path`; CUDA only on the GPU device.
- [x] 3.2 Download the CPU runtime (9 MB) and the VAD model (0.9 MB) through the asset store into `target\whisper-test\assets\`, plus `ggml-base-q8_0.bin` (82 MB). Verify a gated test (`VOICEBRO_WHISPER_DIR`) that:
  - loads the library;
  - compares `whisper_full_default_params(GREEDY)` with the header's defaults field by field;
  - transcribes an Irina and a David WAV of the wake corpus.

  Record which DLLs the CPU set needs, and, when 3.3 runs, whether the GPU set needs `nvrtc*.dll`
  Done: the defaults match the header; Irina and David WAVs transcribed. The CPU set needs `whisper.dll`, `ggml.dll`, `ggml-base.dll` and one CPU variant; the GPU set needs no `nvrtc*.dll`.
- [x] 3.3 Download the CUDA 12.4 runtime (685 MB) and `whisper-large-v3-turbo-q5_0` (574 MB), then load the model on this PC's RTX 5090 Laptop. Measure:
  - the first load, including the driver's JIT;
  - a second load;
  - transcription times for 2, 5 and 10 s utterances;
  - the VRAM from ggml's buffer lines.

  Write the numbers into design D8. If it fails or is slower than the CPU, stop and ask the user about the fallback in D8
  Done: the CUDA 12.4 build runs on the RTX 5090 Laptop; numbers in design D8 (first transcription 25 s of JIT, then 0.12–0.14 s for turbo, about 826 MB of VRAM).
- [x] 3.4 Add `probe whisper <wav> [--model id] [--device cpu|gpu] [--language xx] [--threads n]`, which prints the text, the language, the load time and the transcription time. Verify it on the corpus WAVs on the CPU (and the GPU after 3.3)

## 4. Whisper catalog and manager

- [x] 4.1 Write `stt/catalog.rs` with the entries of design D9 and the VAD model, pinned at the repository commits named there. Verify tests:
  - ids are unique;
  - every hash has 64 hex digits;
  - sizes are positive;
  - groups and licenses are set;
  - the default is `whisper-base-q8_0`
- [x] 4.2 Scan `whisper-models\` once at start into `local:<file>` entries, as Vosk's models folder is scanned. Verify tests: a `.bin` is listed; another file is skipped with a log line; a name with `..` cannot be an id
- [x] 4.3 Write `WhisperManager`:
  - `configure` checks which files are missing (runtime by device, model, VAD) and publishes `NeedsDownload` without touching the network;
  - consent and `download` work as for the wake word;
  - the model loads at a join and unloads 0.5 s after the last leave;
  - a change reloads in the background and keeps the old model until the new one is ready;
  - the fallback from GPU to CPU is detected through the backend registry, with the log line of the spec.

  Verify unit tests with a fake engine and a fake fetch: missing files, consent, load at join, unload after leave, the background reload, a newer configure during a load
- [x] 4.4 Add `Status.whisper` (D15) and its event. Verify a test that serializes it with every field, and that the event bus test (one `event` tag per variant) still passes

## 5. Cutting, transcription and phrases

- [x] 5.1 Write the cutter thread (D2, D3), counting ticks. Verify unit tests:
  - a 300 ms pause stays one utterance;
  - 35 silent ticks end one;
  - 28 s cuts and continues;
  - a 100 ms click is dropped as noise;
  - frames that arrive late in a burst, with the ticks in order, do not end an utterance early
- [x] 5.2 Write the transcriber thread:
  - a queue of 8 that drops the oldest, with a log line and a counter;
  - padding to 1 s;
  - the VAD;
  - the no-speech threshold;
  - the repetition filter;
  - the language and the prompt (D5).

  Verify unit tests with a fake engine for the queue, the padding and each filter
- [x] 5.3 Write the phrase spotting on transcripts (D4): activation while waiting with the question after it, deactivation while active, always mode. Verify the spec scenarios as unit tests: the Bangkok question in one breath, the phrase alone, talk while waiting, "ok thanks, goodbye", and a Russian phrase typed with "ё" and punctuation
- [x] 5.4 Add `probe whisper-corpus`:
  - it runs `target\wake-corpus\<lang>\` with the phrases of `scripts\run-wake-corpus.ps1`;
  - it adds noise-only clips (silence, steady noise 300, a game-like noise made by the probe);
  - it prints one line per file and a total.

  Verify on the CPU with `whisper-base-q8_0` and `whisper-small-q8_0`, with and without the prompt. Write the results next to Vosk's 79 of 100 in design D4, and drop the prompt if a noise clip produces a phrase
  Done: with the prompt base 59, small 65 of 72; no noise clip gave a phrase, so the prompt stays (design D4).
- [x] 5.5 Download the remaining catalog models (turbo q8_0 and F16, large-v3 q5_0, medium q5_0, the two Russian-tuned models and distil-large-v3.5). Run the same corpus with them on the GPU, and on the CPU where the time allows. Write the results into design D9
  Done: every catalog model on the GPU, turbo 71 of 72; table in design D4 and D9.
- [ ] 5.6 Measure on this PC:
  - every catalog model downloaded so far: load time, and transcription time for 2, 5 and 10 s, on the CPU with 4, 6, 8 and 12 threads, and on the GPU after 3.3;
  - the private working set with `scripts\measure.ps1` (release build).

  From the numbers, set the automatic thread count, the window's note for each model and, if the numbers say so, a different default. Update design D5, D9 and the settings spec
  Partly done: load and transcription times on the CPU (4, 6, 8, 12 threads) and the GPU are in design D5 and D9, and the automatic thread count is set (half the logical processors, at most 16). Not done: the private working set with `scripts\measure.ps1` while the app is in a voice channel in the Whisper mode, which needs a join; that is the resource comparison the user planned.

## 6. Voices

- [x] 6.1 Write `tts/sherpa/ffi.rs`:
  - the v2.0.1 structs (D9b);
  - the pre-load of `onnxruntime.dll` by full path;
  - `SherpaOnnxOfflineTtsGenerateWithConfig` with a callback that can stop it;
  - the engine and its audio with `Drop`.

  Verify that a gated test (`VOICEBRO_TTS_DIR`) synthesizes one sentence with Supertonic 3 and checks the sample rate (44 100 Hz) and that the audio is not silent, and that the log names the loaded `onnxruntime.dll` path, not `System32`
- [x] 6.2 Write `tts/phonemize.rs` over piper-phonemize v1.5.9 (initialize with the voice's `espeak-ng-data`, phonemize per sentence, codepoints into the generation config), called only from the TTS thread. Verify that a gated test reads a Russian and an English sentence with a Piper voice
- [x] 6.3 Write `tts/windows.rs` with OneCore through the `windows` crate 0.62 (features of D9b): list `AllVoices`, synthesize to a stream, and parse the RIFF chunks. Verify a test (Windows only, no download) that lists David, Zira, Mark, Irina and Pavel on this PC and synthesizes 16 kHz audio with each
- [x] 6.4 Write `tts/catalog.rs` with the voices of design D9b (Supertonic 3, the Piper table, Kokoro English) and piper-phonemize as an asset that Piper and Kokoro need. Verify tests:
  - ids are unique;
  - hashes and sizes are set;
  - no voice has a non-commercial data license;
  - only Piper and Kokoro need the phonemizer
- [x] 6.5 Add the rational resampler to `audio/dsp.rs` (D7). Verify tests: 16, 22.05, 24 and 44.1 kHz sines come out at 48 kHz with the same frequency (within 1 Hz) and level (within 0.5 dB), and there is no click at chunk borders
- [x] 6.6 Write `SentenceSplitter`, `speakable()`, `SpeechPlayer` (a `FrameProvider`) and `TtsManager` (thread, queue with epochs, 20 s back-pressure, flush, load at join, warm-up, unload after the leave, Preview's own unload). Verify unit tests for each rule of D7 and the `local-speech-synthesis` scenarios:
  - code and links;
  - the first piece at the first comma;
  - a flush during synthesis plays nothing more;
  - a two-minute answer stays within 20 s ahead;
  - `voice_ahead` before voiced audio
- [x] 6.7 Add `probe tts <voice id> "<text>" <out.wav> [--threads n]`. Download Supertonic 3 (129 MB), piper-phonemize, `piper:ru_RU-denis-medium`, `piper:ru_RU-irina-medium`, `piper:en_US-libritts_r-medium` and Kokoro English (320 MB) into `target\`. For each voice, and for the Windows voices:
  - measure the load time, the first-audio time and the RTF over a fixed list of English and Russian sentences about Bangkok and London, with 2, 4 and 8 threads;
  - check Kokoro's phonemes;
  - run every output through Whisper (the most accurate model downloaded) and compare the text, which gives an intelligibility score;
  - write the WAVs into `target\tts-listen\` for the user to listen to.

  Record the numbers in design D9b, set Supertonic's `num_steps`, drop Kokoro if its check fails, and set the default voice from the results. The user's listening has the last word
  Done: numbers in design D9b; WAVs in `target\tts-listen\`; Supertonic keeps 5 steps; Kokoro stays; threads per engine (Supertonic 2, Piper 4, Kokoro 8). The listening test is the user's.

## 7. Codex text turns

- [x] 7.1 Extend `testdata/fake-codex` with:
  - `turn/start` that streams `item/agentMessage/delta` and completes;
  - an interim message;
  - `turn/interrupt`;
  - a method to delay a turn.

  Verify the existing fake-codex tests still pass
- [x] 7.2 Write `assistant/codex/text_session.rs` (D6) and the attach by mode in `codex/mod.rs`:
  - questions with names;
  - deltas to the voice;
  - barge-in with `turn/interrupt`;
  - answers to `AudioPorts.answers`;
  - the chat entries;
  - `working()`;
  - the end of a session.

  Verify tests against the fake: one question and answer; a second question that interrupts the first (interrupt sent before the new start); deactivation during a turn; an answer posted with the question as the task; nothing posted for an interrupted turn
- [x] 7.3 Run the live canary for `additionalContext`: the first question "who is in the voice channel?" with two names sent as context. Record in design D6 which way the context went, and make the history of a new thread go the same way. Verify that the answer names both people
  Done: `application` context takes no `name`; with that the answer named both people (design D6).
- [x] 7.4 Add the live test `whisper_text_turn_when_enabled` (variables: the Whisper dir, the voice dir and a question WAV). It runs the WAV through Whisper, a real Codex text turn and the voice, writes the answer WAV, and prints the four timings of D15. Verify a run with a Russian and an English question
  Done: a Russian and an English question, release build: 5.0 s and 4.9 s from the end of speech to the voice (design D6).

## 8. Gate, runtime and Discord

- [x] 8.1 Add `user_text` and `working` to `AssistantSession` (with defaults) and record them in the stub. In `gate.rs`, add the Whisper mode (D2):
  - the feed and the ticks whenever not muted;
  - no frames to the input;
  - transcripts to phrases and questions;
  - the tones;
  - `working()` for the idle timeout.

  Verify gate tests for the `assistant-session-gate` delta scenarios, and that every existing gate test still passes
- [x] 8.2 In `runtime.rs`:
  - own `WhisperManager` and `TtsManager`;
  - `prepare_join` loads both in Whisper mode and refuses with the spec's reply when files are missing;
  - unload after the last leave;
  - Vosk off in Whisper mode;
  - the mode switch at once: end the sessions, detach and attach in the new mode, keep the bot in the channel.

  Verify runtime tests with stub engines for each scenario of "The mode switches at once" and "Join prepares the assistant"
- [x] 8.3 Write the Whisper latency line and its entry in `Status.latency` (D15). Verify a test that feeds the four timestamps and checks the line
- [x] 8.4 Add the mode to `/status` and the member panel. Verify the handler and panel tests

## 9. Window

- [x] 9.1 Add the commands `whisper_models`, `tts_voices`, `whisper_requirements`, `whisper_download`, `tts_preview` (returns WAV bytes) and `open_whisper_models_folder`, thin over the runtime. Add `Status.whisper` and `Status.tts` to the status snapshot. Verify `cargo build -p voicebro-desktop`
- [x] 9.2 Build the Whisper tab (D13) in `ui/index.html`, `ui/main.js` and `ui/styles.css`, inside the existing design (the `frontend-design` skill for spacing and states):
  - the four collapsible cards;
  - fields by `data-path`;
  - help lines;
  - notices;
  - grouped lists;
  - Preview through an `<audio>` element.

  Verify in the command mock in the browser pane: every field saves, help lines are present, collapsing persists
- [x] 9.3 Give the install dialog the Whisper subject and its triggers (refused join, mode switch, model, device or voice choice). Verify that `check-wake-dialog.mjs` still passes, and a new scenario set for the Whisper subject
- [x] 9.4 On the Config tab: disable the Vosk fields in Whisper mode with the note, and add "realtime mode only" to the Codex Voice and Answer handoff help lines. Add the Status tab tiles (status-view delta). Verify that `check-config-tab.mjs` passes
- [x] 9.5 Extend `ui-dev/mock.js` with the new commands, states and query parameters (`?whisper`, `?whisper_missing`, `?gpu_fallback`), and add `ui-dev/check-whisper-tab.mjs` covering the `settings-ui` and `app-window` delta scenarios. Verify that all three headless checks pass, and that one scenario fails when broken on purpose
- [x] 9.6 Start the real app (debug build, scratch data folder, WebView2 debugging port). Take screenshots of the Whisper tab in Dark and Light into `target\whisper-tab-shots\`. Check that a join refused for missing files opens the dialog (the join is simulated through the probe's Discord path or left to the user, with the report saying which)
  Done: screenshots in `target\whisper-tab-shots\` (dark, light, the voice card) from the real app, build 1.1.0. The refused join opening the dialog is checked in the mock only (`check-whisper-tab.mjs`); in the real app it needs a `/join` from a person and is left to the user.

## 10. Distribution and docs

- [x] 10.1 Extend `scripts\third-party-notices.ps1` section 4:
  - whisper.cpp (MIT text) and NVIDIA's CUDA runtime terms;
  - each Whisper model and the VAD model;
  - sherpa-onnx and onnxruntime;
  - piper-phonemize (GPL-3.0, source link);
  - each voice with its data license and attribution;
  - Supertonic 3's OpenRAIL-M restrictions.

  Add the catalog checks. Verify that the script writes the file, and stops when a catalog entry is missing from it
- [ ] 10.2 Add `whisper-models` to the uninstall hook in `installer-hooks.nsh`. Verify with a trial install and uninstall into `target\install-test\`
  Not fully done: the line is in the hook next to the `models` one. The trial installer 1.1.0 builds and installs silently into `target\install-test\`; the installed app starts with the Whisper tab and creates `whisper-models`; the silent uninstall leaves the data, as it should with the box unchecked. The uninstall with "Delete the application data" checked needs the uninstaller's window and deletes the real Discord token, so it was not run.
- [x] 10.3 Update the docs:
  - `docs/ARCHITECTURE.md`: 2, 3, 4.1, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.10, 5, 6, 7 and 9, plus a new 4.12 for `stt/` and `tts/`;
  - `docs/user-guide.md` and `README.md`;
  - `AGENTS.md`: commands, the new probes, the audio facts of the Whisper mode;
  - `LOCAL-TESTING.md`.

  Verify that every new command in AGENTS.md runs as written
- [x] 10.4 Bump the version for the build handed over

## 11. Checks and report

- [x] 11.1 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` (CMake on PATH), and say in the report what each returned
- [x] 11.2 Report:
  - what was measured;
  - what was decided alone;
  - what changed in the design while building;
  - what was not tested.

  Not tested by the agent: a real Discord channel with two or more people and a game running, the wake phrase in the channel, and the resource comparison of both modes in a channel. These are the user's steps
- [x] 11.3 When the user says so: one commit on the branch with the code, the docs and this change archived (`openspec/changes/archive/<date>-whisper-mode`, the delta specs merged into `openspec/specs/`), then the merge as in AGENTS.md
  Done: one commit on `change/whisper-mode` with the change archived; the merge into `main` is the user's, after `change/voice-recognition-rework`.
