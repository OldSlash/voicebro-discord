# Design

## Context

Today VoiceBro has one way to talk to the assistant. Vosk listens offline for the wake
phrase. The channel's mixed audio then goes to a ChatGPT realtime voice call that Codex
opens over WebRTC, and the call's voice is played back into Discord. The user asked on
2026-11-16 for a second, complete mode of the app ("the Whisper mode"):

- Whisper, running locally, turns what people say into text and also listens for the
  activation and deactivation phrases. Vosk is not loaded in this mode.
- The text goes to Codex CLI through the same `codex app-server` and the same ChatGPT
  sign-in. Codex answers in text. The assistant stays Codex only ("for now purely with
  Codex"; a Claude engine, discussed on 2026-11-16, is not part of this change).
- A local voice, also downloaded on demand, speaks the answer into the channel.
- The user chooses among several Whisper models (fast and slower, multilingual,
  English-only and Russian-tuned), runs Whisper on the GPU or on the CPU, and chooses the
  voice engine and the voice. Everything is free, runs on the PC and is downloaded after
  the user agrees, as the Vosk files are.
- The window gets a Whisper tab.
- The user tests the wake phrase in this mode, and compares the app's resource use in both
  modes, after the change is built.

The facts below were gathered on 2026-11-16 and 2026-11-17: the whisper.cpp release and
its source at v1.9.5, the Hugging Face API for the models, the sherpa-onnx release and
its C API at v2.0.1, and the Codex CLI 0.167.0 app-server schema (dumped with `probe
codex-schema`). Nothing else was run. The only downloads were the 427 KB piper-phonemize
archive and the release listings; the Windows voices were timed on this PC.

## Goals / Non-Goals

**Goals:**

- A second assistant mode, `whisper`, next to today's `realtime`. Both use Codex and the
  same thread, sign-in, instructions and answer posting.
- Whisper (whisper.cpp) runs in the app's process through `libloading`, on the CPU or on an
  NVIDIA GPU, with a catalog of models that the user downloads after agreeing.
- The wake phrases in this mode are spotted in Whisper's text. Vosk stays unloaded.
- Codex answers through text turns. The answer is spoken sentence by sentence as it
  streams in, so the voice starts before the answer is finished.
- A local voice engine with a catalog of voices, downloaded after agreeing, plus the voices
  Windows already has.
- A Whisper tab: the mode, the recognition model and device, the phrases, and the voice.
- Every new native library and model is pinned by SHA-256 and never ships with the app.
- Measured timings per turn in the log and the status, so the user can compare the modes.

**Non-Goals:**

- Any engine other than Codex (Claude, local LLMs).
- Vosk as the wake engine of the Whisper mode. The user replaced it "for now". Keeping
  Vosk for the wake phrase and Whisper for the questions is a later option.
- Streaming (partial) Whisper results; Whisper transcribes whole utterances.
- Whisper on AMD or Intel GPUs. whisper.cpp publishes no Vulkan build (D8).
- A cloud Whisper API, voice cloning, translation.
- Measuring GPU memory in the window (D15 logs what ggml reports).
- Changing the realtime mode.

## Decisions

Decisions the request left open are marked **[decided alone]**, so the user can review
them in one pass.

### D1. The mode is a setting that applies at once

`assistant.mode` is `realtime` (the default: today's behaviour) or `whisper`.

- **The wake engine** follows the mode at once. In `whisper` mode `WakeManager::configure`
  stops Vosk, and its state is `Off`, with the reason "the Whisper mode spots the phrases".
  Back in `realtime` mode Vosk loads again, as at start.
- **The voice channels the bot is in** also switch at once. **[decided alone]** For each
  gate, the runtime ends the session (falling tone), detaches the assistant session, and
  attaches a new one in the new mode to the same gate and output. The bot stays in the
  channel. The alternative, "applies at the next /join", would leave a joined channel deaf
  to the wake phrase until a rejoin, because Vosk is already gone.
- **Built (2026-11-17). [decided alone]** The session is attached again on a **new gate**
  on the same output, not on the same gate: the gate owns the sender of the assistant's
  input, and a new one carries the new mode's settings from its first tick. The speaker
  names and the people in the channel move to the new gate. `/status` says "Mode: Whisper
  mode" or "Mode: Realtime voice", and the member panel has a Mode field.
- **[decided alone]** The mode is its own part of `changed_sections`, `mode`, rather than
  `assistant` as task 1.2 put it: the `assistant` part would also reconfigure the gates
  and the engine for every answer-posting change, which must restart nothing.
- `/status` and the member panel name the mode ("Whisper mode: …" / "Realtime voice").
- The `session.mode` (always or wake word), idle timeout, earcons, mute, owner-only filter
  and answer posting work in both modes.

### D2. Where Whisper sits in the audio path

The gate already hands each speaker's 48 kHz stereo frame to the wake feed in wake-word
mode. In `whisper` mode:

- the gate hands each speaker's frames to the Whisper feed **in both session modes**,
  whenever the app is not muted, because Whisper's text is now the assistant's input;
- the gate sends nothing to `AudioPorts.input`, and the pre-roll and the replay are not
  used: a question is text, cut from the utterance by D4;
- once per receive tick the gate also sends the feed a `Tick { guild, tick }`, so silence
  is counted in Discord's ticks, not on a clock. A busy transcriber thread then cannot
  turn a delayed queue into a false gap (the problem change `voice-recognition-rework`,
  formerly `reliable-deactivation`, found for Vosk).

Two threads, so a slow transcription never blocks the audio:

- `voicebro-whisper-cut` (cheap): per (guild, SSRC) it downsamples to 16 kHz mono with the
  existing `StereoDownsampler`, appends to the speaker's utterance, and decides where it
  ends (D3).
- `voicebro-whisper` (heavy): takes finished utterances from a bounded queue and
  transcribes them one at a time with one whisper context and one `whisper_state` (D8).

Speaker names come from the gate's speaker map, as the caller's name does today.

### D3. Utterances

Constants, kept in one place and changed only with a measurement:

| Rule | Value | Why |
|---|---|---|
| An utterance ends after the speaker sent nothing for | 35 ticks (700 ms) | the gate's existing `UTTERANCE_GAP`; longer than songbird's refill gaps (100–200 ms) |
| Longest utterance | 28 s, then cut and the rest starts a new one | Whisper's window is 30 s |
| Shortest utterance transcribed | 12 frames (240 ms) with a peak ≥ 700 | clicks and breaths are not sent to Whisper |
| Silence added after the speech | 300 ms, and padding to at least 1.0 s in total | whisper.cpp returns nothing for clips under 1 s |
| Utterances waiting for the transcriber | 8; the oldest waiting is dropped, counted and logged | a slow CPU model must not build an ever-growing delay |

**[decided alone]** Each utterance is also run through whisper.cpp's built-in Silero VAD
(`params.vad = true`, model `ggml-silero-v6.2.0.bin`, 885 KB, D9). An utterance with no
speech segments, such as game sound on an open microphone, gives no text. This is the
main guard against Whisper inventing words on noise. The corpus test (D16) has
noise-only clips to check it.

### D4. Phrases and questions in Whisper's text

- **Storage.** `whisper.phrases.activation` and `whisper.phrases.deactivation` are one set
  of lists, not one per model, because Whisper is multilingual. Whisper writes any word,
  so there is no vocabulary check.
- **The fields and the matching form follow change `voice-recognition-rework`**, which was
  in planning at the same time:
  - commas and line breaks separate phrases, and a phrase may have several words;
  - lower case;
  - "ё" as "е";
  - the words in a row, matched as whole words.

  Whisper also writes punctuation, which Vosk does not, so every character that is not a
  letter or a digit counts as a space. Whichever change lands first defines the function,
  and the other reuses it.
- **Default phrases. [decided alone]** `voice-recognition-rework` gives every wake model
  default phrases, and so changes the rule that the code ships none. Once it has landed,
  the Whisper lists start with that change's defaults for the language of
  `whisper.language`, or English for `auto`. Until then they start empty, as the Vosk
  lists do today.
- **While waiting** (wake-word mode, no session):
  - an utterance that holds an activation phrase starts the session, with the rising tone;
  - the words after the phrase in the same utterance, if there are any, are the first
    question, so "hey bro, what is the weather in Bangkok" is one step;
  - an utterance without the phrase is dropped. Its text goes only to a detail line
    (`detail::whisper`).
- **While active:**
  - an utterance that holds a deactivation phrase ends the session at once, and the words
    before the phrase are dropped (D7);
  - every other utterance is a question.
- **Always mode:** every utterance is a question, and the phrases are not used, as with
  Vosk today. The help line says that this sends everything said in the channel to Codex.
- **The prompt.** **[decided alone]** Whisper's `initial_prompt` is set to the phrases,
  joined with commas, so names like "братан" are spelled the way the user typed them. A
  prompt can leak into the text Whisper writes for silence. The VAD (D3) and the
  noise-only corpus clips check that this never starts a session. If they show that it
  does, the prompt is dropped, and the design is updated with the result.
- **Measured (task 5.4, 2026-11-17)** with `probe whisper-corpus`: the 72 files of the
  wake corpus in ten languages, each padded as the cutter pads an utterance, with each
  language's default phrases and `whisper.language = auto`, plus three noise-only clips
  (3 s of silence, steady noise of peak 300, a game-like mix of hum, bursts and a sweep).

  | Model | Device | With the prompt | Without | Median per file |
  |---|---|---:|---:|---:|
  | base q8_0 | CPU | 59 of 72 | 52 of 72 | 0.52 s |
  | small q8_0 | CPU | 65 of 72 | 60 of 72 | 1.80 s |
  | medium q5_0 | GPU | 64 of 72 | 62 of 72 | 0.17 s |
  | large-v3-turbo q5_0 / q8_0 | GPU | 71 of 72 | 68 of 72 | 0.12 s |
  | large-v3-turbo F16 | GPU | 71 of 72 | 68 of 72 | 0.27 s |
  | large-v3 q5_0 | GPU | 67 of 72 | 66 of 72 | 0.25 s |
  | large-v3-ru-podlodka q5_0 | GPU | 67 of 72 | 66 of 72 | 0.26 s |
  | large-v3-turbo-ru | GPU | 55 of 72 | 57 of 72 | 0.27 s |
  | distil-large-v3.5 (English only) | GPU | 31 of 72 (en 18 of 18) | 29 of 72 | 0.26 s |

  - **No noise clip and no speech file without a phrase ever gave a phrase**, with or
    without the prompt, on any model. **The prompt stays**: it helped every multilingual
    model, most of all on one-word phrases in less common languages.
  - Next to Vosk's run of the same corpus (79 of 100 before change
    `voice-recognition-rework`, 140 of 144 after it with the new default phrases), turbo on
    the GPU is as good; base on the CPU is not. The window recommends turbo on an NVIDIA GPU.
  - The misses: short one-word phrases. Base on the CPU wrote "Bratan" in Latin letters for
    the Russian word with automatic language, and wrote nothing for some one-word goodbyes.
    Turbo's one miss wrote "отбои" for "отбой" ("й" as "и"); the matching form of change
    `voice-recognition-rework` is not changed for it. A fixed `whisper.language` helps short
    utterances.

### D5. Whisper's decoding parameters

- `language` from `whisper.language`: `auto` (Whisper detects it per utterance) or a fixed
  ISO 639-1 code. A fixed language is faster and avoids misdetection on short clips. Default
  `auto`. **[decided alone]**
- `no_context = true`: every utterance stands alone. `single_segment = false`.
- `suppress_nst = true`, `no_speech_thold = 0.6`. A segment whose
  `whisper_full_get_segment_no_speech_prob` is above the threshold is dropped.
- Greedy decoding (`best_of = 1`), temperature 0 with whisper.cpp's default fallback.
- `n_threads`: `whisper.threads`, where 0 means automatic. The automatic value is set from
  a measurement on this machine (task 5.6); whisper's own default is `min(4, cores)`.
- Text that repeats one word four or more times in a row is dropped as a hallucination.
  Counted and logged.
- `print_progress`, `print_timestamps`, `print_realtime` and `print_special` are all false.
  whisper.cpp's own log goes through `whisper_log_set` into the app log at debug level,
  except the buffer-size lines, which are kept at info (D15).
- **Measured (task 5.6, 2026-11-17).** On this PC (Intel Core Ultra 9 275HX, 24 logical
  processors) 12 threads beat 4, 6 and 8 for every model. Per clip of 2, 5 or 10 s:

  | Model (CPU) | 4 threads | 6 | 8 | 12 |
  |---|---:|---:|---:|---:|
  | tiny q8_0 | 0.49–0.54 s | 0.35–0.39 s | 0.29–0.37 s | 0.23–0.28 s |
  | base q8_0 | 1.13–1.14 s | 0.79–0.80 s | 0.74–0.75 s | 0.54–0.56 s |
  | small q8_0 | 3.7–3.8 s | 2.5–2.6 s | 2.4–2.5 s | 1.7–1.8 s |
  | medium q5_0 | 14.1–18.3 s | 10.2–13.8 s | 9.0–12.1 s | 6.4–9.5 s |
  | large-v3-turbo q5_0 | 22.7–22.8 s | 16.5–16.7 s | 13.5–14.5 s | 11.1–11.3 s |

  Whisper always encodes a 30 s window, so the clip's length hardly matters. **[decided
  alone]** The automatic count (`whisper.threads = 0`) is half the logical processors, at
  most 16: 12 here. In a game the CPU is shared; the user can lower the count on the tab.

### D6. Questions to Codex as text turns

The app-server, the sign-in check, the lists and the thread are the same as today. A
`/join` in Whisper mode prepares them as it does now, plus the Whisper model and the voice
(D12). What differs is the session that `CodexAssistant::attach` builds in `whisper` mode:
`CodexTextSession` instead of `CodexSession`. It opens no realtime call. The schema facts
below were checked against Codex CLI 0.167.0:

- **A question** is sent as `turn/start { threadId, input: [{ type: "text", text }] }`. With
  `assistant.speaker_labels` set to `start` or `changes`, the text is "<name>: <question>";
  with `off` it is the question alone.
- **The call context.** The people in the voice channel and the caller go with the first
  turn of each session as `additionalContext: { "voicebro.channel": { kind: "application",
  value: "<people and caller>" } }`. This needs a canary test before it is relied on
  (AGENTS.md): a live probe asks "who is in the channel?". If Codex ignores the field, the
  context goes as a first line of the question text, and the design is updated with the
  result.
- **The canary (task 7.3, 2026-11-17), live with Codex CLI 0.167.0.** The first attempt was
  refused: "additional context name is only supported for untrusted context". `application`
  context takes only `kind` and `value`; the app sends no `name`. With that, the question
  "Who is in the voice channel right now?" (a Windows voice, through Whisper turbo on the
  GPU) and the people "Alex, Sam" as `additionalContext` got "Alex and Sam are in the voice
  channel right now." **The context goes as `additionalContext`**, and the history of a new
  thread goes the same way (`voicebro.history`). Its value is facts only (the channel's
  people and who addressed the assistant), no instruction.
- **Measured end to end (task 7.4)** with `whisper_text_turn_when_enabled`, release build:
  speech end → text 229 ms (2.5 s of audio, turbo q8_0 on the GPU); text → Codex's first
  words 3.8 s (default model and reasoning); first words → voice 0.6 s (Supertonic 3);
  speech end → voice 4.6 s. A Russian question ("Братан, сколько сейчас времени в
  Бангкоке?", the corpus file, Supertonic reading Russian) took 0.24 + 4.0 + 0.8 = 5.0 s
  and an English one ("Hey bro, what time is it in London?", Piper libritts_r) 0.23 + 3.9
  + 0.7 = 4.9 s; in both Codex first said an interim line ("Сейчас проверю время в
  Бангкоке.", "I'll check the current time in London."), which was spoken, then searched
  and answered. Codex's own time dominates; a lower reasoning level is the quick choice,
  as the tab says.
- **Barge-in.** **[decided alone]** A new question while an answer is running or still
  being spoken works like this:
  - the running turn is interrupted (`turn/interrupt { threadId, turnId }`);
  - the voice stops at once (D7);
  - the new question starts a new turn.

  This is the realtime call's behaviour, but it reacts to words that Whisper heard, not to
  any sound, so game noise and coughs do not cut the answer. `turn/steer` exists and was
  considered: it adds text to the running turn, but the voice would keep reading the old
  answer.
- **The answer.** `item/agentMessage/delta { delta, itemId, turnId }` streams into the voice
  path (D7). `item/completed` with an `agentMessage` gives the final text of that item for
  the chat. `turn/completed` with status `completed` sends `AgentAnswer { guild, task:
  question, text: last agent message }` through `AudioPorts.answers`, so answer posting
  (change `post-assistant-answers`) works unchanged.
- **What is spoken.** **[decided alone]** Every agent message of the turn is spoken,
  including an interim line before a web search ("Let me check that"). The interim line
  fills the wait. Only the last message is posted, as today. Reasoning items are never
  spoken.
- **Ending the session** (deactivation, timeout, mute, mode change) does three things:
  - it interrupts a running turn;
  - it clears the voice;
  - it drops questions that have not been sent.
- **The chat log** gets the same entry kinds as a call: the speaker's question, the spoken
  answer, the system lines and the errors.
- **The model and reasoning** are `codex.model` and `codex.reasoning_effort`, the thread's
  settings. **[decided alone]** No separate pair for this mode. The help line on the
  Whisper tab says that in this mode they set how fast every answer comes, and that a low
  reasoning level is the quick choice.
- **History.** It lives in the thread while the app-server runs. When a new thread opens
  (a new join after the app-server stopped, or a model change), the first turn's
  `additionalContext` carries the guild's last exchanges, at most 24 KB, the same limit as
  the realtime history. **[decided alone]**
- **Instructions** are `codex.instructions`, as the thread's developer instructions, and
  nothing else. The code adds no instruction of its own, such as "be brief" (AGENTS.md: the
  code ships no default assistant instruction). Markdown is made speakable by D7 instead of
  by asking the model.
- `ready()` is true once the thread exists, which is right after the join. The idle
  timeout also counts a running turn as activity (`AssistantSession::working()`, false by
  default), so a 20 s web search does not end the session.

### D7. Speaking the answer

```text
item/agentMessage/delta ─► SentenceSplitter ─► speakable() ─► TTS queue (guild, epoch)
   ─► voicebro-tts thread: synthesize ─► resample to 48 kHz, stereo
   ─► SpeechPlayer (a FrameProvider on the guild's output) ◄─ songbird mixer, 20 ms
```

- **SentenceSplitter.** A sentence ends at `.`, `!`, `?` or `…` followed by a space or the
  end of the text, or at a line break. A piece shorter than 20 characters waits for the
  next sentence. A piece longer than 300 characters is cut at the last comma, semicolon or
  space. When the item completes, whatever remains is sent. The first piece of an answer
  ends earlier, at the first comma after 20 characters, so the first audio starts sooner
  (D9b).
- **`speakable()`** removes what a voice should not read:
  - code blocks become nothing, and the chat still has them;
  - tables are skipped;
  - `[text](url)` becomes the text, and a bare URL becomes its host;
  - heading, list, emphasis and quote markers are removed;
  - an emoji is removed.

  A unit test covers each rule.
- **SpeechPlayer.** The guild's output pulls one frame from it per read, like the call's
  jitter buffer:
  - it holds the synthesized 48 kHz audio and plays it in order;
  - `voice_ahead()` is true while it holds audio with a peak of 256 or more, so sending
    toward Discord starts before the voice plays (change `transmit-only-when-speaking`);
  - it reports the first played frame of each turn for the latency line (D15);
  - the output gain applies as to every assistant frame.
- **Back-pressure.** The TTS thread synthesizes at most 20 s ahead of what has played.
  Beyond that it waits. That bounds memory, at about 2 MB per guild of 48 kHz mono `i16`,
  and wastes nothing when an answer is cut.
- **Flush.** Deactivation, barge-in, mute and a mode change raise the guild's epoch:
  - the player is emptied at once;
  - queued sentences of an older epoch are skipped;
  - a synthesis already running for an older epoch is stopped through the engine's progress
    callback (it returns 0) or, for an engine without one, discarded.
- **Resampling.** The voices produce 16, 22.05, 24 or 44.1 kHz. A polyphase windowed-sinc
  resampler is added to `audio/dsp.rs` for any rational ratio, deterministic and without a
  new crate, as the existing fixed-ratio ones are. Tests check that a sine keeps its
  frequency and level through each ratio.

### D8. The Whisper runtime: whisper.cpp v1.9.5 through `libloading`

The user asked whether the Vosk bridge can be reused. It cannot be used as is, because it
wraps the functions of `libvosk.dll`. Whisper gets a wrapper of the same kind: a new
`stt/whisper/ffi.rs` that loads `whisper.dll` at run time, keeps raw pointers inside the
module and frees every handle on drop. The download, hash check, unpacking, consent and
install dialog of the Vosk files are shared (D10).

- **Builds.** The latest stable whisper.cpp is v1.9.5 (2026-10-06). Its binaries are on the
  nightly tag `b5454`, which has the same commit `d1be6fde11ac6e0407606b4e42fe72d34add8037`.
  Two zips are pinned:

  | Runtime | Asset | Bytes | SHA-256 |
  |---|---|---:|---|
  | CPU | `https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip` | 8 928 640 | `6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb` |
  | NVIDIA GPU | `https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-win-cuda-12.4.0-x64.zip` | 684 913 404 | `afef0b881c500958921c3f5523b50e59ee2ec9b6f5cbd25b324c51ed308a957a` |

  The GPU zip already holds the whole CPU set. After extraction only these are kept:
  `whisper.dll`, `ggml.dll`, `ggml-base.dll` and the nine `ggml-cpu-*.dll`, about 9.8 MB.
  The GPU folder also keeps `ggml-cuda.dll`, `cudart64_12.dll`, `cublas64_12.dll` and
  `cublasLt64_12.dll`, about 1.15 GB. The exes and the examples' DLLs are deleted, including
  `parakeet.dll` and `llama.dll`, which the app does not use. Whether
  `nvrtc*.dll` is needed is checked by loading without it (task 3.2).
- **Not used:** the CUDA 11.8 zip, which holds no cuBLAS. There is no Vulkan build and no
  CUDA 12.8+ build for x64. llama.cpp's ggml DLLs are a different ggml (1.0.0 against
  0.26.0) and are never mixed in.
- **Loading order** (the official builds use `GGML_BACKEND_DL=ON`, and `whisper.dll` loads
  no backend itself):
  1. load `ggml-base.dll` and, on the GPU, `cudart64_12.dll`, `cublas64_12.dll` and
     `cublasLt64_12.dll` by full path with `LOAD_WITH_ALTERED_SEARCH_PATH`. ggml loads its
     backends with a plain `LoadLibraryW`, which would look in the exe's folder and the
     system folders for their imports; Windows reuses a module already loaded under the same
     name, so this binds those imports to the runtime folder. A process-wide
     `SetDllDirectoryW` is not used: it would also change where the voice engine's
     `onnxruntime.dll` is looked for (D9b);
  2. load `ggml.dll` and call `ggml_backend_load_all_from_path(<runtime folder>)`. With a
     null path ggml would search the exe's folder instead. It picks the best `ggml-cpu-*`
     variant and loads `ggml-cuda.dll` when the folder has it. On the CPU device with the GPU
     runtime on disk, only the CPU variants are loaded (`ggml_backend_load` per file), so
     no CUDA context is made;
  3. load `whisper.dll`, check that `whisper_version()` is `1.9.5`, and refuse the load
     otherwise;
  4. `whisper_init_from_file_with_params` with `use_gpu` and `flash_attn` from the device
     (D11), then one `whisper_init_state`.
- **Struct layout.** `whisper_full_params`, `whisper_context_params` and
  `whisper_vad_params` are passed by value. They are mirrored in Rust exactly as the v1.9.5
  header declares them. The pinned hash and the version check together make the mirror
  valid. A test loads the real DLL (gated by `VOICEBRO_WHISPER_DIR`) and compares
  `whisper_full_default_params(GREEDY)` with the header's defaults, field by field. A wrong
  layout fails there and not in a call.
- **Threads.** One context and one state on the transcriber thread. The header says a
  context is not thread-safe; nothing else touches it.
- **Blackwell (this PC's RTX 5090 Laptop).** The CUDA 12.4 zip has native code up to sm_89
  and PTX for compute_90, and no sm_120 code. NVIDIA says PTX binaries run on Blackwell
  through the driver's JIT. The first start compiles for a while (the driver caches the
  result), and cuBLAS 12.4 is not tuned for the chip. This is not verified. Task 3.3
  measures it on this machine (the user allowed every test download on 2026-11-17). If it fails or
  is slow, the user decides on the fallback: an own build with CUDA 12.8+ and `120a-real`,
  hosted on the project's release and pinned the same way. That needs the CUDA toolkit on
  the build machine, a system-wide install that needs the user's OK.
- **Measured (tasks 3.2 and 3.3, 2026-11-17).**
  - The CPU set needs `whisper.dll`, `ggml.dll`, `ggml-base.dll` and one `ggml-cpu-*`
    variant; all nine variants are kept, and the app loads the best one for the CPU by
    `ggml_backend_score` (`ggml-cpu-alderlake` here). The GPU set adds `ggml-cuda.dll`,
    `cudart64_12.dll`, `cublas64_12.dll` and `cublasLt64_12.dll`; **no `nvrtc*.dll` is
    needed**.
  - The defaults test (`whisper_full_default_params(GREEDY)` and the context defaults
    against the v1.9.5 header, field by field) passes, so the struct mirrors are right.
  - **The CUDA 12.4 build runs on this PC's RTX 5090 Laptop (Blackwell).** The very first
    transcription after installing took 25.2 s: the driver compiled the PTX. The driver
    caches the result: the next process loaded large-v3-turbo q5_0 in 0.44–0.55 s and
    transcribed 2, 5 and 10 s clips in 115, 140 and 138 ms, against 11 s on the CPU. No
    fallback build is needed; the first-use JIT is noted on the tab.
  - VRAM from ggml's lines (kept at info): turbo q5_0 573 MB of weights, KV 10.5 + 31.5 +
    7.9 MB, compute buffers 37.7 + 55.4 + 9.3 + 100.0 MB: about 826 MB in all.
- **What changed while building. [decided alone]**
  - whisper.cpp applies its VAD only in `whisper_full` (on the context's own state), not in
    `whisper_full_with_state`. The app therefore uses the context's state through
    `whisper_full`; one context per model, on the transcriber thread only, as planned.
  - `ggml_backend_load_all_from_path` would also load `ggml-cuda.dll` from the GPU folder
    and create a CUDA context on the CPU device. The app instead loads the best CPU variant
    itself, and loads the CUDA libraries and `ggml-cuda.dll` by full path only when the GPU
    device is chosen.
  - The runtime's libraries are loaded once per process and stay loaded: ggml's backend
    registry is global and CUDA is not made to be unloaded. The runtime folder is the GPU
    one when it is on disk (it holds the CPU set too), else the CPU one. If the CPU runtime
    was loaded first and the GPU runtime is downloaded later, the GPU needs a restart of
    the app; the fallback reason says so.

### D9. Whisper models

From `ggerganov/whisper.cpp` on Hugging Face (MIT), pinned to the repo commit
`5359861c739e955e79d9a303bcbc70fb988958b1`
(`https://huggingface.co/ggerganov/whisper.cpp/resolve/<commit>/<file>`). Each is a single
`.bin` file, not an archive (D10). The catalog, in the order the window lists it:

| Id | File | Bytes | SHA-256 | Note in the window |
|---|---|---:|---|---|
| `whisper-tiny-q8_0` | ggml-tiny-q8_0.bin | 43 537 433 | `c2085835d3f50733e2ff6e4b41ae8a2b8d8110461e18821b09a15c40c42d1cca` | fastest, rough |
| `whisper-base-q8_0` | ggml-base-q8_0.bin | 81 768 585 | `c577b9a86e7e048a0b7eada054f4dd79a56bbfa911fbdacf900ac5b567cbb7d9` | fast; weak for Russian |
| `whisper-base.en-q8_0` | ggml-base.en-q8_0.bin | 81 781 811 | `a4d4a0768075e13cfd7e19df3ae2dbc4a68d37d36a7dad45e8410c9a34f8c87e` | fast, English only |
| `whisper-small-q8_0` | ggml-small-q8_0.bin | 264 464 607 | `49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f` | balanced; on a CPU about 1–2 s |
| `whisper-small.en-q8_0` | ggml-small.en-q8_0.bin | 264 477 561 | `67a179f608ea6114bd3fdb9060e762b588a3fb3bd00c4387971be4d177958067` | balanced, English only |
| `whisper-medium-q5_0` | ggml-medium-q5_0.bin | 539 212 467 | `19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f` | accurate; GPU |
| `whisper-large-v3-turbo-q5_0` | ggml-large-v3-turbo-q5_0.bin | 574 041 195 | `394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2` | accurate and fast on a GPU (turbo) |
| `whisper-large-v3-turbo-q8_0` | ggml-large-v3-turbo-q8_0.bin | 874 188 075 | `317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1` | the same, closer to full quality |
| `whisper-large-v3-turbo` | ggml-large-v3-turbo.bin | 1 624 555 275 | `1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69` | turbo at full precision |
| `whisper-large-v3-q5_0` | ggml-large-v3-q5_0.bin | 1 081 140 203 | `d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1` | most accurate, slower decoder; GPU |

Russian-tuned and English-distilled entries, from their own repositories, pinned the same
way:

| Id | Source and file | Bytes | SHA-256 | License, note |
|---|---|---:|---|---|
| `whisper-large-v3-turbo-ru` | `dvislobokov/whisper-large-v3-turbo-russian` / ggml-model.bin | 1 624 555 275 | `39f24d81480828d2b25e75c7c02b5c72b3db3b98e1c33d39990bc186cb3fe804` | MIT; turbo fine-tuned on Common Voice 17 Russian; no published WER |
| `whisper-large-v3-ru-podlodka-q5_0` | `kdes70/whisper-large-v3-ru-podlodka-ggml` / ggml-podlodka-large-v3-q5_0.bin | 1 081 140 203 | `3f870a58279fd98cf60ca1ac6cd97333ef4ac1a34c5991ba75d0fa7d6d428ce5` | Apache-2.0 base (bond005, 10.99 % WER on its own test); converted by an unknown uploader |
| `whisper-distil-large-v3.5` | `distil-whisper/distil-large-v3.5-ggml` / ggml-model.bin | 1 519 521 155 | `ec2498919b498c5f6b00041adb45650124b3cd9f26f545fffa8f5d11c28dcf26` | MIT; English only |

Plus the VAD model, which every Whisper setup downloads:
`ggml-org/whisper-vad` → `ggml-silero-v6.2.0.bin`, 885 098 bytes,
`2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987` (MIT).

- Left out: `large-v1`/`v2` and full `large-v3`, which are 3.1 GB with the same encoder as
  turbo and a slower decoder. Also left out are the conversions of `antony66`'s Russian
  fine-tune, because they declare no license.
- What is known about accuracy, from OpenAI's paper (FLEURS WER, Russian / English): tiny
  31.1 / 12.4 %, base 20.5 / 8.9 %, small 11.4 / 6.1 %, medium 7.2 / 4.4 %, large-v2 5.6 /
  4.2 %. Turbo performs "similarly to large-v2" and cannot translate, which the app does not
  need. The Russian fine-tunes' numbers are self-reported.
- What is known about speed: Whisper always encodes a 30 s window, so a 2 s clip costs the
  full encoder.
  - On the maintainer's desktop RTX 5090, encodes take 21 ms for turbo, 5.5 ms for small and
    2.2 ms for base, plus under 1 ms per token.
  - On CPUs like this one, base takes about 0.4–0.7 s, small about 1.2–2 s, and
    medium or turbo several seconds.

  Nothing here was measured on this PC. Task 5.6 measures every entry, on the CPU and on
  the GPU, and the window's notes are written from that.
- **Measured (tasks 5.5 and 5.6, 2026-11-17)** on the RTX 5090 Laptop (CUDA 12.4 build,
  flash attention) and on the CPU (12 threads for the first five, 8 for the rest, one
  4.2 s clip). Load is the model's own load with the runtime already loaded; VRAM is the
  weights only, from ggml's "CUDA0 total size" line (KV and compute buffers add about
  250 MB for the large models).

  | Model | Load (GPU) | GPU 2 / 5 / 10 s | Weights in VRAM | CPU per clip |
  |---|---:|---:|---:|---:|
  | tiny q8_0 | 0.15 s | 27 / 43 / 79 ms | 43 MB | 0.23–0.28 s |
  | base q8_0 | 0.16 s | 80 / 68 / 66 ms | 81 MB | 0.54–0.56 s |
  | base.en q8_0 | 0.15 s | 82 / 116 / 161 ms | 81 MB | 0.68–0.74 s (8 thr.) |
  | small q8_0 | 0.27 s | 73 / 127 / 239 ms | 264 MB | 1.7–1.8 s |
  | small.en q8_0 | 0.27 s | 110 / 206 / 220 ms | 264 MB | 2.2–4.7 s (8 thr.) |
  | medium q5_0 | 0.44 s | 152 / 245 / 477 ms | 539 MB | 6.4–9.5 s |
  | large-v3-turbo q5_0 | 0.45 s | 116 / 139 / 139 ms | 573 MB | 11.1–11.3 s |
  | large-v3-turbo q8_0 | 0.59 s | 114 / 141 / 139 ms | 874 MB | 11.9 s (8 thr.) |
  | large-v3-turbo (F16) | 1.04 s | 251 / 282 / 296 ms | 1 624 MB | 15.5 s (8 thr.) |
  | large-v3 q5_0 | 0.75 s | 233 / 364 / 586 ms | 1 081 MB | 15.4 s (8 thr.) |
  | distil-large-v3.5 | 0.98 s | 251 / 268 / 291 ms | 1 519 MB | 15.9 s (8 thr.) |
  | large-v3-turbo-ru | 0.99 s | 262 / 287 / 346 ms | 1 624 MB | 15.9 s (8 thr.) |
  | large-v3-ru-podlodka q5_0 | 0.74 s | 202 / 302 / 683 ms | 1 081 MB | 15.8 s (8 thr.) |

  On the GPU every model answers within 0.7 s, and turbo q5_0/q8_0 within 0.15 s; on the
  CPU only tiny, base and small are usable in a conversation. The window's notes say so.
  **[decided alone]** The defaults stay `cpu` and `whisper-base-q8_0`, the choice that
  works on any PC; the tab recommends turbo q8_0 on an NVIDIA GPU, which costs the same
  time as q5_0 here.
- **Defaults.** **[decided alone]** `whisper.device = cpu` and `whisper.model =
  whisper-base-q8_0`, the choice that works on any PC. The tab recommends turbo for an
  NVIDIA GPU. Task 5.6 may change the default after the measurement; if it does, this
  table and the settings spec change with it.
- **The user's own models.** A `whisper-models\` folder next to the app takes any ggml
  `.bin` (another fine-tune, for example). Each file there is listed as
  `local:<file name>`, the way Vosk's models folder works: it is listed once at start, and
  the app never changes it.

### D9b. Voices: sherpa-onnx with Supertonic 3, Piper and Kokoro, and the Windows voices

The user asked for a choice among the free local voices that are used now. Most of the
well-known ones were left out for one of three reasons:

- they are licensed for non-commercial use only: XTTS-v2, F5-TTS, Fish-Speech, Silero,
  Spark-TTS, MMS;
- they need Python and a large GPU and do not speak Russian: Dia, CSM, Orpheus,
  VibeVoice, Higgs, Zonos, Kyutai;
- there is no native runtime for them yet that fits the app.

A larger GPU tier (Qwen3-TTS, CosyVoice 3, VoxCPM2, all Apache-2.0 with Russian, through
the young `audio.cpp` library) is a possible later change. What fits now, all on the CPU
(D11):

1. **The engine: sherpa-onnx v2.0.1 (2026-10-26, Apache-2.0).**
   - **The archive** is
     `https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-win-x64-shared-MT-Release-lib.tar.bz2`,
     7 899 712 bytes, `afa4de3ca4ab0afd53bb4aab40d28ee15608364b549ffc0fe0caf6489bfc39a5`.
   - **Kept** are `sherpa-onnx-c-api.dll` and its `onnxruntime.dll` (onnxruntime 1.28.2).
   - **Loading.** `onnxruntime.dll` is loaded first, by full path, because Windows 11 has its
     own `System32\onnxruntime.dll` (1.17 on this PC), which must not be the one
     `sherpa-onnx-c-api.dll` binds to.
   - **Structs.** The `#[repr(C)]` structs are mirrored by hand from the v2.0.1 header. The
     official `sherpa-onnx-sys` crate links at build time and does not fit loading at run
     time. They changed between 1.13.8 and 2.0.0, so the pinned hash is what makes them
     valid. A gated test synthesizes a sentence through the real DLL and checks the sample
     rate and that the audio is not silent.
   - **Calls.** Only `SherpaOnnxOfflineTtsGenerateWithConfig` is used; the other Generate
     calls are deprecated. Its callback returns 0 to stop a synthesis that a flush made
     useless (D7). One engine runs on the TTS thread.
   - **No CUDA build.** The CUDA build needs about 3.2 GB of NVIDIA libraries for models that
     already run faster than real time on this CPU.
2. **Supertonic 3 (Supertone, May 2026): the default family. [decided alone]**
   - **The archive** is `https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2`,
     128 774 318 bytes, `82fa96f91c4ef8abaae3a14a3f4153facf88bed821d1f7331cec2700f432c427`.
   - **Languages and voices.** 31 languages, among them all ten of the wake catalog
     (Russian included); ten voices, F1–F5 and M1–M5; 44.1 kHz.
   - **No phonemizer.** It reads raw text, so it needs no phonemizer and no GPL component.
   - **The license** is BigScience OpenRAIL-M. It allows commercial use, but its use
     restrictions must be passed on, so they are shown before the download and in the
     notices.
   - **The language of each sentence** (`extra = {"lang": …}`) is the language Whisper
     detected for the question, or `whisper.language` when it is fixed, else `na`, which
     lets the model infer it.
   - **Speed.** `num_steps` is 5 by default. Task 6.7 measures whether 3 or 4 is needed for
     speed.
   - **Not yet heard.** No Russian listening test has been done. Task 6.7 does one, and the
     default may change with it.
   - **Numbers.** The model card says the app must write numbers and currency as words. This
     change adds no number-to-words. **[decided alone]** The listening test checks how
     digits sound. If they sound wrong, a per-language number expansion becomes a follow-up;
     Russian numerals with their cases are a job of their own.
3. **Piper voices.** These go through the same sherpa-onnx DLL, plus **piper-phonemize
   v1.5.9**.
   - **Why the extra part.** sherpa-onnx 2.0 removed espeak-ng (GPL-3.0), and with it the
     ability to read text for Piper voices. Its built-in lexicon path drops every word it
     does not know, and it has no Russian lexicon at all.
   - **What piper-phonemize is.** The same maintainer's library, archive
     `piper-phonemize-v1.5.9-windows-x64-shared-lib.tar.bz2`, 427 015 bytes,
     `e0a66e68a96c7d65a408255599816408f7cf6a0b84866b4cfd5a80f3f71abfa7`. It turns each
     sentence into the espeak-ng phoneme codepoints, using the voice's `espeak.voice` from
     its `.onnx.json`. Those go into `GenerationConfig.phoneme_codepoints`.
   - **espeak-ng's data** comes with every Piper archive. espeak-ng keeps global state, so
     only the TTS thread calls it.
   - **The GPL question. [decided alone]** piper-phonemize contains espeak-ng and is GPL-3.0.
     - The app (MIT) never ships it.
     - It is a separate download, listed as its own item in the dialog, with "GPL-3.0" and
       a link to its source, and only Piper and Kokoro voices need it.
     - Supertonic and the Windows voices work without it.

     Pinning sherpa-onnx 1.13.8, which still has espeak-ng inside, was considered: the same
     GPL question, frozen on the last 1.x. Whether Piper stays in the catalog under these
     terms is the user's call.
   - **The Piper catalog.** It has the medium quality unless noted. Voices whose data is
     non-commercial (ryan, hfc_*, miro, ruslan) or AGPL (fr tom) are left out. Each archive
     is `https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-<voice>.tar.bz2`
     at 22.05 kHz:

     | Id | Language | Bytes | SHA-256 | Data license |
     |---|---|---:|---|---|
     | `piper:en_US-libritts_r-medium` | English (US), speaker 0 of 904 | 82 038 311 | `10dc268f3e371696d721486123e2705a9fc1faa113491979fde4d88dba1f1b1c` | CC BY 4.0 |
     | `piper:en_GB-cori-high` | English (UK), female | 115 574 061 | `42922f07738fcde2e49eed4e959635692f73b933de35a6b7c1010162ff566292` | public domain |
     | `piper:en_GB-alba-medium` | English (UK), female | 67 212 349 | `fcd45962906933eec4431d3688f7d74aaac8713c87c6717f91fd3b23463aa1a1` | CC BY 4.0 |
     | `piper:ru_RU-denis-medium` | Russian, male | 67 190 991 | `efa4c18e0b5e32b81d1b6df36b9d312831e5d545200e27848ef926a4cd930300` | CC0 |
     | `piper:ru_RU-dmitri-medium` | Russian, male | 67 188 551 | `c86d0803737de13d441923ff3b3f309482fab8d7af3ec85949942809eb9a3660` | CC0 |
     | `piper:ru_RU-irina-medium` | Russian, female | 67 153 308 | `1fc0f54e5e084fe287c07909f2f6e0ba6d857864cf800e3ab80286a4e8233008` | not stated (RHVoice data); marked so in the window |
     | `piper:cs_CZ-jirka-medium` | Czech, male | 67 212 775 | `321aafd83dd89b3d5b2c36c5898e0710d4f472f3c191cb7f73901a89b7f8539f` | CC0 |
     | `piper:nl_NL-pim-medium` | Dutch, male | 67 240 538 | `e78ed301bf6fc66561a04c110754cd6928d772630a3b80b09a83f89410130171` | CC0 |
     | `piper:nl_BE-nathalie-medium` | Dutch (Belgium), female | 67 226 472 | `c7484313abea395243e872196bc75181c07aad8120b8a61e0f6f4b3863b6a7c8` | CC0 |
     | `piper:fr_FR-siwis-medium` | French, female | 67 207 459 | `375909aa30842b3a4efa10b1beb1d761af792960ae6873b4d53889f96c66195b` | CC BY 4.0 |
     | `piper:de_DE-thorsten-medium` | German, male | 67 214 254 | `50487d9c95fdf2191f31d2588569381063ba1591dcd4c7d4bdd30f12b2191714` | CC0 |
     | `piper:it_IT-paola-medium` | Italian, female | 67 221 173 | `7541f75778afa164e44e34baaef63befad7698595df26a95ca944b63ef1a16b4` | CC0 |
     | `piper:pl_PL-gosia-medium` | Polish, female | 67 211 182 | `75bd34dcbdc4dd98d763954756b4b34b4208100497c836381542e4d73dcefa9c` | CC0 |
     | `piper:pl_PL-darkman-medium` | Polish, male | 67 215 635 | `444727aa46eb6db645a2bc88fe73868e4bd7596b7f56ca39fad5ef53c41210d4` | CC0 |
     | `piper:pt_BR-faber-medium` | Portuguese (Brazil), male | 67 183 065 | `7add3f923ad6bc25ca8a192805fd1a64d1b3893e4611c4a9719545a825039a83` | CC0 |
     | `piper:es_ES-davefx-medium` | Spanish, male | 67 184 952 | `a3f6beb54a9cb893279f72978a22f807a4d9fc9c7848157b524d5cc7b7f58b22` | CC0 |
     | `piper:es_MX-ald-medium` | Spanish (Mexico) | 67 196 497 | `c60cf7bc853eb01a875fba0756523fecf9d9f46c763b19785d223fc981fb4fb6` | Unlicense |

     The ~21 MB `-int8` variants are not offered until a listening test shows that they
     sound the same. Speed is no reason to use them: Piper is fast either way.
4. **Kokoro English** (`kokoro-en-v0_19.tar.bz2` from the same `tts-models` release).
   - **The file:** 319 625 534 bytes,
     `912804855a04745fa77a30be545b3f9a5d15c4d66db00b88cbcd4921df605ac7`, Apache-2.0, 11
     voices, 24 kHz. It is the best-sounding small English model.
   - **Phonemes.** It is phonemized with piper-phonemize (`en-us`).
   - **The int8 file** is not used: it is slower than fp32 on a CPU.
   - **Not verified.** Nobody has checked that espeak phonemes fit Kokoro v0_19's tokens under
     sherpa-onnx 2.0. `probe tts` checks it in task 6.7. If they do not fit, Kokoro leaves the
     catalog, and this design records why.
   - **No Russian.** No official Kokoro speaks Russian.
5. **The Windows voices (OneCore, WinRT), with no download.**
   - **The API** is `Windows.Media.SpeechSynthesis.SpeechSynthesizer` through the `windows`
     crate. Version 0.62 is already in `Cargo.lock` through Tauri; the core adds the features
     `Media_SpeechSynthesis`, `Storage_Streams` and `Foundation`.
   - **The list** comes from `AllVoices` at run time, with ids `windows:<voice id>`. On this
     PC it is David, Zira and Mark (English, US), and Irina and Pavel (Russian). Other
     languages appear once the user adds that Windows speech pack.
   - **The output** is a 16 kHz mono 16-bit WAV, read by walking its RIFF chunks.
   - **Speed and quality.** Measured on this PC at 8–78 ms per sentence. Quality is clearly
     below the neural voices; these are the "nothing to download" choice.
   - **Narrator voices.** Narrator's natural voices are not available to other apps. The one
     adapter that reaches them uses keys extracted from system files, so it is not used.

- **Measured (task 6.7, 2026-11-17)** with `probe tts <voice> --bench`: four English and
  four Russian sentences about Bangkok and London (Supertonic reads both sets), 2, 4 and 8
  threads, then Whisper large-v3-turbo q8_0 on the GPU reads each output back. The WAVs
  are in `target\tts-listen\` for the user to listen to.

  | Voice | Load | First sentence | RTF | Whisper read back |
  |---|---:|---:|---:|---:|
  | Supertonic 3 F1, 5 steps, 2 threads | 0.47 s | 0.47 s | 0.13 | 90 % |
  | Supertonic 3 F1, 3 steps, 2 threads | 0.52 s | 0.31 s | 0.075 | 89 % |
  | Supertonic 3 M1, 5 steps, 2 threads | 0.46 s | 0.52 s | 0.13 | 89 % |
  | Piper ru denis, 4 threads | 0.44 s | 0.12 s | 0.031 | 91 % |
  | Piper ru irina, 4 threads | 0.52 s | 0.16 s | 0.036 | 91 % |
  | Piper en libritts_r, 4 threads | 0.51 s | 0.09 s | 0.035 | 90 % |
  | Kokoro en Bella, 8 threads | 0.70 s | 0.63 s | 0.19 | 92 % |
  | Kokoro en George, 8 threads | 0.76 s | 0.62 s | 0.18 | 92 % |
  | Windows David / Irina | 0.02 s | 5–10 ms | 0.002 | 92 % / 91 % |

  The read-back score counts the sentence's words Whisper wrote; numbers written as digits
  ("33") count as misses, so no voice reaches 100 %.
  - Supertonic was fastest with **2 threads**: more threads made it slower (RTF 0.17 with
    8). Piper is fast with any count; Kokoro gained up to 8. **[decided alone]** The threads
    are per engine: Supertonic 2, Piper 4, Kokoro 8.
  - **[decided alone]** Supertonic keeps `num_steps = 5`: 3 or 4 steps are faster but not
    needed for speed, and the read-back score is the same. The listening test decides.
  - **Kokoro stays.** Its espeak phonemes from piper-phonemize fit sherpa-onnx 2.0: Whisper
    read back 92 %.
  - piper-phonemize's `PiperPhonemizeInitialize` returns espeak-ng's sample rate (22050) on
    success, not 0; the app reads any positive value as success.
  - `onnxruntime.dll` is loaded from the engine's folder by full path, as planned; the
    probe's log names that path, not `System32`.
  - The default voice stays `supertonic3:F1`. The user's listening has the last word.
- **The default voice. [decided alone]** `tts.voice = supertonic3:F1`. It is GPL-free and
  speaks every catalog language. Task 6.7's listening test may change it; the settings spec
  changes with it.
- **The first audio.** Supertonic and Kokoro are estimated at a real-time factor of 0.1–0.3
  on this CPU, and Piper at 60–150 ms per sentence; none of this is measured yet. So the
  first piece of each answer is cut short: the first clause, up to the first comma, at
  least 20 characters (D7). The rest is synthesized while it plays. The engine is created
  once per join and warmed up with one short synthesis. Creating it loads 70–330 MB, and
  the first Generate is slower.
- **Voice ids:** `supertonic3:F1` … `supertonic3:M5`, `piper:<voice>`, `kokoro-en:<speaker>`
  and `windows:<voice id>`.

### D10. Downloads and consent, shared with the wake word

- `wake/assets.rs` and the `Asset` type move to a shared `assets/` module that the wake
  word, Whisper and the voices all use. The behaviour stays:
  - `curl.exe` into a `.part` file, then the hash check;
  - an archive the user put into `assets\` by hand is used instead of a download;
  - a mismatch deletes the file;
  - `.part` files are removed at start.
- `Asset` gains `kind: Archive | File`. A `File` (a Whisper model) is not unpacked. After the
  hash check it is moved to `assets\<directory>\<file>`, and that file is its own marker.
- An archive can now list which files to keep (`keep: &[&str]`, glob patterns). Everything
  else extracted is deleted, which is how the Whisper zips shrink (D8).
- The sherpa-onnx and voice archives are `.tar.bz2`. The system `tar.exe` (bsdtar) is
  expected to unpack them. Task 2.4 checks this on this PC; if it fails, a pure-Rust
  `bzip2` + `tar` pair is used instead, and that is a new dependency to note.
  **Checked (task 2.4, 2026-11-17):** the system `tar.exe` (bsdtar) unpacks `.tar.bz2` on
  this PC, in a test with a `tar -cjf` archive and with the real sherpa-onnx, Supertonic,
  Piper, Kokoro and piper-phonemize archives. No new dependency.
- **[decided alone]** A `File` asset's name in the assets folder is its marker (the model's
  own file name), not the last part of its URL: three models of other repositories are all
  called `ggml-model.bin` there, and a file put into the folder by hand must not be taken
  for another model.
- **Consent works as for Vosk, per set of files.** `WhisperManager::configure` checks in the
  calling thread which files of the runtime (CPU or GPU, by `whisper.device`), the model,
  the VAD model, the voice engine and the voice are missing. Missing files without consent
  publish `NeedsDownload { items }` (label, host, bytes, file, page). Nothing is requested
  from the network until the user presses Download (`whisper_download`). The consent is
  held in memory only, and another model or voice asks again.
- **When the dialog opens.** The install dialog takes a second subject, `whisper`, which
  lists all those files and their total. In Whisper mode it opens:
  - at a refused `/join` (D12);
  - when the mode is switched to Whisper with files missing;
  - from a model or voice choice on the Whisper tab.

  Never at the window's start, which matches the user's decision of 2026-11-12 for the
  wake word.
- The download by hand is offered as for Vosk: the file name, the official page, Open
  assets folder.

### D11. CPU or GPU

- `whisper.device`: `cpu` or `gpu`. `gpu` downloads the CUDA runtime (D8) and loads with
  `use_gpu = true` and `flash_attn = true`. `cpu` downloads only the CPU runtime. If the GPU
  runtime is already on disk, `cpu` loads from it with `use_gpu = false`, so no CUDA
  context and no VRAM is used.
- **When the GPU fails, the app falls back to the CPU.** **[decided alone]** If there is no
  NVIDIA driver, `ggml-cuda.dll` is skipped by ggml without a word and only the CPU backend
  is left. The app sees this through the backend registry (`ggml_backend_dev_count`, by
  type). It then loads on the CPU and says so in the log ("Whisper device: gpu requested,
  cpu used: no CUDA device"), in the status, and on the tab. Not working at all would be
  worse in a game.
- `whisper.threads` (0 automatic) applies only on the CPU.
- **The voice's device.** **[decided alone]** It is CPU only. A voice model is small and
  runs faster than real time on the CPU. sherpa-onnx's CUDA path would need about 3.2 GB of
  NVIDIA downloads (CUDA, cuBLAS, cuDNN) for little gain. So there is no `tts.device`
  setting.

### D12. Loading and unloading

- **Load at `/join`.** **[decided alone]** In Whisper mode, `prepare_join` loads the
  Whisper model and the voice, after Codex's preparation. When the files are missing, the
  join is refused with "The Whisper mode needs downloads: open VoiceBro → Whisper tab and
  press Download", and the window opens the dialog. Load time and device are logged.
- **Unload after the last leave.** Both unload 0.5 s after the bot leaves its last voice
  channel, as the Codex app-server stops. They also unload when the mode goes back to
  realtime.
- Between joins nothing of Whisper or the voice is in memory. The idle cost stays what it
  is today, and the comparison the user plans is between the time in a channel in each
  mode.
- **Built. [decided alone]** "Nothing in memory" holds for the models and the voices.
  The engines' libraries (whisper.cpp and ggml, sherpa-onnx with onnxruntime,
  piper-phonemize) are loaded on first use and stay loaded for the process: ggml's backend
  registry and espeak-ng are global, and onnxruntime is not made to be unloaded. They cost
  a few MB of the private working set; a failed first load of the voice engine stays an
  error until the app restarts.
- **Changes while loaded.** A change of model, device, language or voice while loaded
  reloads in the background. The old model keeps serving until the new one is ready.
  Phrase changes apply to the next utterance without a reload.
- **[decided alone]** A language or thread change also applies from the next utterance
  without a reload: both are decoding parameters, not loading ones. Only a model or device
  change reloads.
- **[decided alone]** A join counts as wanting the model even when files are missing: a
  download agreed after a refused join, or after a switch to Whisper while in a channel,
  loads the model as soon as the files are in. Leaving releases it.

### D13. The window: the Whisper tab

A new tab, **Whisper**, after Config. Its cards fold like the Config cards
(`ui.collapsed_cards` ids `whisper-mode`, `whisper-recognition`, `whisper-phrases`,
`whisper-voice`). Every field saves on change, with its help line and validation message
like the Config cards, and has its own feedback line at the bottom.

1. **Mode.** "Assistant mode" (`assistant.mode`) with two choices, "Realtime voice (Codex
   voice call, Vosk wake word)" and "Whisper (local recognition, Codex text answers, local
   voice)". The help line also says what leaves the PC: in Whisper mode only text goes to
   OpenAI. Under it is a status line from `Status.whisper` and `Status.tts`: what is loaded,
   on which device, and the last turn's timings (D15).
2. **Speech recognition.** The fields are:
   - Model, grouped as Multilingual, English only, Russian-tuned and Your models. Each entry
     shows its size, a one-line note and "downloaded" or "to download";
   - Device: CPU or NVIDIA GPU (CUDA);
   - CPU threads;
   - Language: Automatic, then the ten languages of the wake catalog, then the rest of
     Whisper's list;
   - the download notice, as under the wake model;
   - "Open Whisper models folder".
3. **Phrases.** Activation and deactivation phrases (`whisper.phrases.*`, separated by
   commas or line breaks, as on the Config tab after `voice-recognition-rework`).
   There is a second control for "Activation: wake word / always" that shares
   `session.mode` with the Config tab. The help line says Whisper writes any word, that
   matching ignores case and punctuation, and what always mode sends.
4. **Voice.** The fields are:
   - Voice, grouped by engine and language, with size and "downloaded", "to download" or
     "built into Windows";
   - Speed;
   - Preview: synthesizes a short sentence in the voice's language and plays it in the
     window only, never in Discord. The sentence is the app's own sample, neutral, for
     example about the weather in London;
   - the download notice.

On the Config tab, while the mode is Whisper, the Assistant card's Vosk fields (wake
phrases, wake engine, wake model) are disabled. A note says "Whisper mode: the phrases are
on the Whisper tab". The Codex card's Voice and Answer handoff fields carry "realtime mode
only" in their help lines.

The command mock (`ui-dev/mock.js`) gets the new commands and states. A headless check,
`check-whisper-tab.mjs`, runs the tab's scenarios as the other checks do.

### D14. Settings

```text
assistant.mode        realtime (default) | whisper
whisper.model         catalog id or local:<file>          default whisper-base-q8_0 (D9)
whisper.device        cpu (default) | gpu
whisper.threads       0 (automatic) .. 64
whisper.language      auto (default) | an ISO 639-1 code Whisper knows
whisper.phrases       { activation: [], deactivation: [] }   defaults as D4 says
tts.voice             catalog id or windows:<voice id>     default: see D9b
tts.speed             0.5 .. 2.0, default 1.0
```

- **Validation:**
  - the model, the voice and the language must be known;
  - the two phrase lists must not share a phrase in the matching form;
  - the ranges above are enforced.
- `changed_sections` reports `whisper` and `tts`. The runtime applies them as D12 says. A
  change of `assistant.mode` triggers D1.
- **Compatibility.** No schema version change: a 1.0.x file loads with the defaults, and an
  older build keeps the new sections as unknown fields.

### D15. Status, log and timings

- `Status.whisper`: the fields are
  - `state` (off, needs_download, downloading, installing, idle, loading, ready, error);
  - the model;
  - the device requested and the device in use, with the reason for any fallback;
  - the backend description (for example "CUDA0: NVIDIA GeForce RTX 5090 Laptop GPU" or
    "CPU alderlake, 8 threads");
  - the load time;
  - the utterances queued, transcribed, dropped as late, dropped as noise and dropped as
    hallucination;
  - the last transcription time and audio length.

  `Status.tts`: the state, the voice, the engine, the sample rate and the last time to first
  audio.
- **One latency line per answer** (detail `latency` as today, also in `Status.latency`):
  `[latency:<guild>] whisper turn <n>: speech end -> text <ms> ms (<s> s of audio, <model>,
  <device>); text -> Codex first words <ms> ms; first words -> voice <ms> ms; speech end ->
  voice <ms> ms`.
- **GPU memory** is not measured by the app. ggml's own load log names its buffers
  ("CUDA0 model buffer size = … MiB", the KV and compute buffers). Those lines are kept at
  info level, so the log shows what Whisper holds on the GPU. The private working set
  (`scripts\measure.ps1`, the System tab) already includes the in-process engines.
- What Whisper heard is a detail line (`detail::whisper`), written to the file only with the
  verbose log, as the Vosk text is.

### D16. Tests and probes

- Unit tests, without models:
  - the cutter's rules (ticks, gap, cap, minimum);
  - phrase matching and the question after the phrase;
  - the gate in Whisper mode;
  - the sentence splitter and `speakable()`;
  - the resampler;
  - the player's flush and epochs.
- `CodexTextSession` is tested against `testdata/fake-codex`. The fake gains `turn/start`,
  streamed deltas, `turn/interrupt` and the completion.
- **Probes:**
  - `probe whisper <wav> [--model id] [--device cpu|gpu] [--language xx]` prints the text,
    load time and transcription time;
  - `probe tts <voice id> "<text>" <out.wav>`;
  - `probe whisper-corpus` runs the existing wake corpus (`target\wake-corpus\<lang>\`, the
    same files as the Vosk run of 2026-11-11) through Whisper with the corpus phrases. It
    adds noise-only clips and prints one line per file, so the result sits next to Vosk's
    79 of 100.
- **Live, gated by variables:** a WAV question goes through Whisper, then a real Codex text
  turn, then the voice, and comes out as a WAV. The test prints the four timings.
- **Not tested by the agent:** a real Discord channel with two or more people and a game
  running (AGENTS.md). That is the user's step, including the wake phrase test the user
  announced.

## Risks / Trade-offs

- [The CUDA 12.4 build on Blackwell is unverified] → measured on this PC before anything is
  promised (D8); the CPU path works regardless.
- [`whisper_full_params` layout] → pinned zip hash, version check, defaults test (D8).
- [Whisper invents text on noise, and the prompt could leak the wake phrase] → VAD, the
  no-speech threshold, the minimum voiced length, the repetition filter, and noise-only
  corpus clips (D3, D4, D5).
- [A slow CPU model makes every answer late] → a bounded queue, the per-turn timings, and
  the tab's notes from the measurement; the GPU is recommended.
- [Whisper waits for the end of an utterance, so the wake phrase counts about 0.7 s after
  the speaker stops, plus the transcription] → accepted for this mode; Vosk's realtime
  mode stays for the fastest wake.
- [Barge-in on any transcribed words: chatter in a busy channel cuts answers] → the same as
  the realtime call's barge-in; the user's channel test decides whether it needs a switch.
- [Large downloads: up to about 1.15 GB of CUDA runtime plus a model up to 1.6 GB] → listed
  with sizes in the dialog; nothing is fetched without Download.
- [Russian fine-tunes from individual uploaders] → hash-pinned, license declared, and
  checked on the corpus; the window marks them as community models.
