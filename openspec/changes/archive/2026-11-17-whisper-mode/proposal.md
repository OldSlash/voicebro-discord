# Proposal

## Why

Today the app has one way to talk to the assistant. Vosk listens for the wake phrase, and
the channel's audio then goes to a ChatGPT realtime voice call that Codex opens. On
2026-11-16 the user asked for a second, complete mode of the app, built from local parts:

- Whisper, running on the PC, turns what people say into text and listens for the wake
  phrases;
- Codex CLI answers that text through the same app-server and ChatGPT sign-in;
- a local voice speaks the answer into the channel.

The user wants to choose among many Whisper models (fast and slower, multilingual and
Russian-tuned), to run Whisper on the GPU or the CPU, to choose the voice engine and the
voice, and to have every file downloaded on demand as the Vosk files are. Vosk is not
needed in this mode; Whisper replaces it for the wake phrase "for now". The window gets a
Whisper tab. The assistant stays Codex only. After the change is built, the user tests the
wake phrase in this mode and compares the app's resource use in both modes.

## What Changes

- **A second assistant mode.** The setting `assistant.mode` is `realtime` (default, today's
  behaviour) or `whisper`.
  - A change applies at once. Vosk unloads or loads, and every voice channel the bot is in
    switches its session to the new mode without leaving.
  - The session modes (always / wake word), idle timeout, earcons, mute, owner-only filter
    and answer posting work in both modes.
- **Whisper in the app's process.**
  - whisper.cpp v1.9.5 is loaded through `libloading`, like `libvosk.dll` today, by a new
    wrapper of the same kind.
  - Two runtimes, each pinned by SHA-256: CPU (9 MB) and NVIDIA GPU / CUDA 12.4 (685 MB).
  - GPU or CPU is a setting. A GPU that is missing or fails falls back to the CPU and says
    so.
  - Whisper and the voice load at `/join` and unload after the last leave, so nothing of
    them is in memory between joins.
- **A catalog of Whisper models**, each pinned and downloaded only after the user agrees:
  - tiny, base and small (multilingual and English-only);
  - medium;
  - large-v3-turbo in three sizes;
  - large-v3;
  - two Russian-tuned models;
  - an English distilled model;
  - the user's own ggml files in a `whisper-models\` folder.

  A 0.9 MB Silero VAD model comes with every setup, so that noise gives no text.
- **Utterances instead of a stream.** Each speaker's audio is cut into utterances at 0.7 s
  of silence, counted in Discord's receive ticks. The utterances are transcribed one at a
  time on their own thread, from a small bounded queue.
- **Phrases in Whisper's text.**
  - `whisper.phrases` holds one activation list and one deactivation list (any words, no
    vocabulary check), matched case- and punctuation-insensitively as whole words.
  - The words after an activation phrase in the same utterance are the first question.
  - In always mode every utterance is a question.
- **Codex text turns.** Each question becomes a `turn/start` on the guild's existing thread,
  with the speaker's name.
  - The answer streams in through `item/agentMessage/delta`.
  - A new question while an answer runs interrupts it (`turn/interrupt`), which is the
    realtime call's barge-in on words, not on noise.
  - The final answer is posted to Discord as today, when posting is on.
  - The app adds no instruction of its own.
- **Speaking the answer.**
  - The streamed text is cut into sentences, and Markdown, code, tables and URLs are removed
    from what is read.
  - Each sentence is synthesized on its own thread, resampled to 48 kHz, and played through
    the guild's output, so the voice starts after the first sentence.
  - Deactivation, barge-in and mute stop the voice at once.
- **Local voices**, on the CPU, downloaded after the user agrees, except Windows' own:
  - **The engine:** sherpa-onnx v2.0.1, 8 MB, Apache-2.0, loaded through `libloading`.
  - **Supertonic 3** (129 MB): one archive with ten voices in 31 languages, Russian among
    them. It needs no phonemizer. It is the default.
  - **Piper voices** (67–116 MB each): seventeen voices for English, Russian, Czech, Dutch,
    French, German, Italian, Polish, Portuguese and Spanish. Voices whose data is
    non-commercial are left out.
  - **Kokoro English** (320 MB, 11 voices): the best small English model.
  - **piper-phonemize** (0.4 MB): Piper and Kokoro need it since sherpa-onnx 2.0 removed
    espeak-ng. It is GPL-3.0, so it is a separate download with its license shown.
  - **The Windows voices** (David, Zira, Mark, Irina, Pavel on this PC): instant, robotic,
    no download.

  Each voice shows its data license, and the design (D9b) lists them with hashes.
- **The Whisper tab**, after Config, with four foldable cards:
  - Mode;
  - Speech recognition: model, device, threads, language, download;
  - Phrases;
  - Voice: voice, speed, Preview in the window, download.

  While the mode is Whisper, the Config tab's Vosk fields are disabled with a note.
- **Downloads.** The asset store moves out of `wake/`, so the wake word, Whisper and the
  voices share the hash check, the download by hand and the consent. It learns single-file
  assets and keeping only the needed files of an archive. The install dialog gets a Whisper
  subject.
- **Timings and status.**
  - `Status.whisper` and `Status.tts` report what is loaded, on which device, the queue and
    the drops.
  - One latency line per answer: speech end → text → Codex's first words → voice.
  - The GPU buffer sizes that ggml reports are logged.
- **Probes and tests.**
  - `probe whisper`, `probe tts`, and `probe whisper-corpus`, which runs the existing wake
    corpus through Whisper next to Vosk's results, with noise-only clips;
  - a gated live test from a WAV question through Codex to a WAV answer;
  - unit tests for every pure part.

Not in this change:

- Claude or any engine other than Codex;
- Vosk for the wake phrase inside the Whisper mode;
- streaming (partial) Whisper results;
- Whisper on AMD or Intel GPUs: whisper.cpp publishes no Vulkan build;
- the voice on a GPU;
- a cloud Whisper API, voice cloning, translation;
- a GPU memory meter in the window;
- any change to the realtime mode.

## Capabilities

### New Capabilities

- `whisper-recognition`:
  - the Whisper runtime and its pinned builds;
  - the model catalog and the models folder;
  - the device choice and the fallback;
  - utterance cutting, the transcriber queue and the noise guards;
  - loading at join and unloading after the last leave.
- `whisper-assistant-mode`:
  - the mode setting and its switch at once;
  - the phrases in Whisper's text;
  - the questions as Codex text turns, barge-in and the end of a session;
  - the spoken answer and the per-turn timings.
- `local-speech-synthesis`:
  - the voice engines and the voice catalog;
  - the Windows voices;
  - sentence splitting and speakable text;
  - the player, its flush and resampling;
  - the Preview.

### Modified Capabilities

- `assistant-session-gate`: in Whisper mode the gate feeds every speaker to Whisper with
  receive ticks, sends no frames to the assistant, and takes phrases and questions from
  transcripts. A running turn counts as activity for the idle timeout.
- `wake-word-detection`: Vosk is off in Whisper mode, with that reason in its state.
- `wake-model-assets`: the shared asset store, single-file assets, kept-files lists, and
  the Whisper and voice files under the same consent and download-by-hand rules.
- `codex-assistant`: a join in Whisper mode prepares the same thread; the text session with
  `turn/start`, deltas, `turn/interrupt`, `additionalContext` and the answers.
- `app-settings`: `assistant.mode` and the `whisper` and `tts` sections, with their
  validation.
- `settings-ui`: the Whisper tab's fields, the disabled Vosk fields in Whisper mode, the help
  lines, and the new collapsible cards.
- `app-window`: the Whisper tab in the tab order, and the install dialog's Whisper subject.
- `status-view`: the Whisper and voice lines in the status.
- `app-distribution`: the third-party notices for the downloaded runtimes and models; the
  uninstaller also removes `whisper-models\`; release artifacts carry none of the new
  files.

## Impact

- `crates/voicebro-core`:
  - new `assets/`, moved from `wake/assets.rs`, with single-file and kept-files support;
  - new `stt/`: the engine traits, `whisper/ffi.rs` and the loader, the catalog, the
    models folder, the cutter, the transcriber and `WhisperManager`;
  - new `tts/`: the engine trait, the engines of D9b, the catalog, the splitter,
    `speakable()`, `SpeechPlayer` and `TtsManager`;
  - `audio/dsp.rs`: the rational resampler;
  - `gate.rs`: the Whisper mode, ticks to the feed, transcripts, and the activity of a
    running turn;
  - `assistant/mod.rs`: `AssistantSession::user_text` and `working`;
  - `assistant/codex/text_session.rs`, and in `codex/mod.rs` the attach by mode;
  - `runtime.rs`: the managers, the mode switch at once, loading at join and unloading
    after the last leave;
  - `settings/mod.rs`, `status.rs`, `events.rs`, `latency.rs`, `wake/mod.rs` (off in
    Whisper mode);
  - `discord/handler.rs` and `panel.rs`: the mode in `/status` and in the panel.
- `apps/desktop`:
  - the commands for the Whisper and voice lists, downloads, requirements, Preview and the
    models folder;
  - the Whisper tab in `ui/index.html`, `ui/main.js` and `ui/styles.css`;
  - `ui-dev/mock.js` and a new `ui-dev/check-whisper-tab.mjs`;
  - `installer-hooks.nsh`.
- `apps/probe`: `whisper`, `tts`, `whisper-corpus`.
- `crates/voicebro-core/testdata/fake-codex`: text turns.
- Docs:
  - `docs/ARCHITECTURE.md`: 2, 3, 4.1, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.10, 5, 6, 7 and 9,
    plus a new 4.12;
  - `docs/user-guide.md`, `README.md`, `AGENTS.md`;
  - `scripts\third-party-notices.ps1`, section 4.
- Dependencies: no new crate for Whisper, sherpa-onnx or the resampler. The core adds the
  `windows` crate 0.62, which is already in `Cargo.lock` through Tauri, with the features
  `Media_SpeechSynthesis`, `Storage_Streams` and `Foundation`, for the Windows voices. If
  the system `tar.exe` cannot unpack `.tar.bz2`, a `bzip2` and `tar` crate pair is added
  (checked in the tasks).
- Downloads after the user's OK: from 9 MB (CPU runtime) up to about 1.15 GB (CUDA runtime)
  plus a model of 44 MB to 1.6 GB, plus the voice. Nothing is in the installer or the zip.
- A settings file from 1.0.x loads in the realtime mode with the new sections at their
  defaults. An older build keeps the new sections as unknown fields.
