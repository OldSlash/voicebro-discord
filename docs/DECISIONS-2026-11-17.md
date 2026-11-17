# Decisions of 2026-11-17

The user asked an agent to carry out the two open OpenSpec changes one after the other,
`voice-recognition-rework` first and `whisper-mode` second, without asking questions, and to
write down how each open point was decided. This file is that record. Each change's own
`design.md` (archived with the change) holds the same decisions marked **[decided alone]**
next to the decision they belong to, with the measurements.

## How the work is split

- **Two branches, merged one after the other.** `change/voice-recognition-rework` starts at
  `main`. `change/whisper-mode` starts at the tip of `change/voice-recognition-rework`, not
  at `main`: the Whisper mode reuses the phrase matching, the default phrases and the
  tick-counted feed of the first change (its design says "whichever change lands first
  defines the function, and the other reuses it"), and both changes touch the gate, the
  settings and the window. Branched from `main`, the second merge would conflict in those
  files and duplicate the matching. Merged in order, `--no-ff` as `AGENTS.md` says, each
  merge is clean.
- **One commit per branch**, as the git flow asks: the code, the docs and the change
  archived. No AI attribution line, nothing pushed, nothing merged.

## voice-recognition-rework

What it does: the deactivation phrase works while the voice answers, phrases may be several
words between commas, "ё" and "е" match, and every wake model comes with default phrases.

### Decided while building

1. **Trailing silence after 300 ms of absence, counted in receive ticks (D1), kept although
   two short phrases are confirmed 100–120 ms later.** Measured with the real Russian
   model: with songbird-like holes of 6 ticks every 0.5 s, the old worker (zeros in the
   holes) missed the phrase in 1 to 2 of 5 phases on 5 of the 9 Russian samples ("отбой"
   heard as "от бобби"); the new worker heard exactly what the uninterrupted stream heard
   in every run. At the end of a sentence 7 of 9 samples are confirmed at the same moment
   as before, but "Ладно, отбой." and "Заканчивай." 100 and 120 ms later (400 ms after the
   speech instead of 280–300 ms), above the 0.1 s the task named. The task said to bring
   that to the user; since this run allows no questions, I kept the design: a lower
   threshold would put zeros back into songbird's refill gaps, which is the fault being
   fixed, and the call's transcript ends the session on its own clock.
2. **The people's transcript is read from the app-server's `thread/realtime/transcript/delta`
   with `role: "user"`.** The Codex CLI 0.167.0 schema has the `role` field, so the data
   channel was not needed (D2).
3. **The falling tone needed no fix.** A test plays an answer, deactivates mid-answer and
   runs the Codex session's two flushes: the whole earcon plays and nothing of the answer
   follows. The silent stops of the report were therefore not the app's deactivation.
4. **The default phrases after the corpus check** (D7). The table of the plan was checked
   with the Codex voice speaking every language; defaults heard in fewer than 3 of 4 runs
   were replaced with the best candidate (66 candidate recordings):

   | Model | Activation | Deactivation | Replaced |
   |---|---|---|---|
   | English (US) | bro | peace out | — |
   | Russian | братан | отбой | — |
   | Czech | brácha | čau | kámo/kámoš ("samo"), čus ("sil s") |
   | Dutch | maat | doei doei | doei ("dooi") |
   | French | mon frère | à la prochaine, a la prochaine | frérot ("raison"), tchao ("tiens") |
   | German | bruder | bis dann | tschau ("schau", "ciao") |
   | Italian | fratello | arrivederci | — |
   | Polish | ziomek | do widzenia | nara ("nora") |
   | Portuguese (Brazil) | irmão | até mais | mano ("humano"), tchau ("chão") |
   | Spanish | hermano | hasta luego | compa ("tampa"), chao |

   Final corpus: 140 of 144 runs right, no false hit, every default heard in at least 3 of
   its 4 runs. Czech "čau" is also a greeting; it is the only Czech candidate that passed,
   and it only ends a running session.
5. **French gets both spellings** of its farewell, because the small French model writes
   "a la prochaine" without the accent when the phrase is said alone. The matching form
   folds only "ё", as designed.
6. **Words with an apostrophe or a hyphen** ("don't", "по-моему") count as known to the
   model when it knows the whole word, its folded form, or each piece, so such a phrase is
   not dropped as unknown; matching splits them the same way on both sides.
7. **Version-1 settings files** now keep an empty list as empty when they are migrated,
   instead of letting it fall back to the model's default.
8. **New log lines** beyond the plan: the gate's phrase list for the call's transcript at
   start, at each join and on each change ("the call's transcript ends a session on: …"),
   so a report shows which phrases were in effect.
9. **The decision delay handed to the gate** is now the recognizer's part plus the time the
   audio waited in the queue, so the replay after an activation starts at the phrase's
   real end when the worker runs behind.
10. **Version 1.0.2** for every crate, so a build from `main` after the merge can be told
    apart.

### Not done here

- Task 7.3, the acceptance in a real Discord channel with a second person and a game: the
  user's step.
- Task 7.2 only in part: the debug app was started in a scratch folder with the Russian
  model and the user's three deactivation phrases ("wake word ready" with all of them, no
  unknown word, the gate's transcript list in the log), then switched to the English model
  without phrases ("bro" and "peace out" in effect and shown in the fields). Joining the
  test channel needs someone to type `/join`, so the ticks reaching the worker in the app
  were not watched; the unit tests cover them.

### The call's transcript as the second source, measured live

Thirty real calls (three rounds of five per phrase) said the phrase 1.5 s into the voice's
answer. Two decisions came out of them:

- **The voice is silenced as soon as the transcript holds the phrase**, also while the gate
  still waits 300 ms to see whether the last word goes on ("stop" + "watch"). If it does
  go on, the voice plays again. Without this the wait let the reply through.
- **A turn that the call cut at a pause is read with the next one.** Codex ended a turn at
  the comma of "Отбой, братан" in 2 of 5 calls, so a two-word phrase never matched.

Result with the final rule: the voice was silenced before its reply in 14 of 15 calls for
"Goodbye." and in 8 of 15 for "Отбой, братан.". The task asked for 4 of 5 per phrase; the
Russian phrase misses it because Codex transcribed the two words 0.36–0.78 s after the
speech while its voice often began to reply after 0.27–0.63 s. In those calls the reply is
cut 10–250 ms after it starts, with the falling tone, instead of the voice answering the
phrase. The local wake engine (Vosk), fixed by point 1 above, remains the faster path. Task
5.6 stays open with this note.

## whisper-mode

What it does: a second assistant mode next to today's realtime voice call. In the Whisper
mode whisper.cpp writes down on this PC what people say and spots the phrases in that text,
Codex answers in text turns on the same thread and sign-in, and a local voice reads the
answer into the channel sentence by sentence. Only text goes to OpenAI. The window gets a
Whisper tab, and every new library, model and voice is downloaded only after the user's OK,
pinned by SHA-256. Version 1.1.0.

### Decided while building

1. **The runtime's libraries stay loaded; the models do not.** whisper.cpp's ggml keeps a
   global backend registry and CUDA is not made to be unloaded, so whisper.dll, ggml and
   the voice engine (sherpa-onnx with onnxruntime, piper-phonemize) are loaded on first use
   and kept for the process. The models and voices load at a join and unload 0.5 s after
   the last leave, as planned. The libraries cost a few MB.
2. **CUDA only when the GPU is chosen.** The app loads the best CPU variant of ggml itself
   (by `ggml_backend_score`, `ggml-cpu-alderlake` here) instead of
   `ggml_backend_load_all_from_path`, which would create a CUDA context even on the CPU
   device. If the CPU runtime was loaded first and the GPU runtime is downloaded later, the
   GPU needs an app restart, and the fallback reason in the log and on the tab says so.
3. **`whisper_full` on the context, not a separate state.** whisper.cpp v1.9.5 applies its
   voice activity detection only in `whisper_full`. One context per model, on the
   transcriber thread only.
4. **The automatic thread count is half the logical processors, at most 16** (12 here). On
   this PC 12 threads beat 4, 6 and 8 for every model.
5. **Language and CPU threads apply from the next utterance**, without a reload (they are
   decoding parameters); only a model, device or voice change reloads in the background,
   the old one serving until the new one is in. The settings delta spec says so now.
6. **The prompt stays.** Whisper's `initial_prompt` is the phrases in effect. It helped every
   multilingual model, and no noise clip or speech without a phrase ever gave a phrase.
7. **The call context goes as `additionalContext`, facts only.** The live canary found that
   Codex CLI 0.167.0 refuses a `name` on `application` context; without it the answer
   named both people sent as context. The value says who is in the channel and who spoke,
   nothing more (the code ships no assistant instruction). A new thread's first turn gets
   the guild's last exchanges the same way.
8. **Every agent message is spoken, the last one is posted.** Codex's interim line ("I'll
   check the current time in London.") fills the wait while it searches.
9. **Voice threads per engine: Supertonic 2, Piper 4, Kokoro 8.** Supertonic got slower with
   more threads. Supertonic keeps 5 denoising steps: 3 or 4 are faster but not needed.
10. **Kokoro stays.** Its espeak phonemes from piper-phonemize work with sherpa-onnx 2.0:
    Whisper read back 92 % of its words. piper-phonemize's initialize returns espeak's
    sample rate (22050) on success, not 0; the app reads any positive value as success.
11. **The default voice stays Supertonic 3 F1**, GPL-free and speaking every catalog
    language; the defaults for recognition stay CPU and base q8_0, the choice that works on
    any PC. The tab recommends turbo on an NVIDIA GPU. The user's listening test has the
    last word on the voice.
12. **The mode switch builds a new gate on the same output**, rather than reattaching on the
    same gate: the gate owns the sender of the assistant's input. The speaker names and the
    people in the channel move over; the bot stays in the channel. The mode is its own
    settings part, `mode`, so answer-posting saves keep rebuilding nothing.
13. **A refused join releases Whisper and the voice; a join counts as wanting them even when
    files are missing**, so a download agreed after a switch to Whisper inside a channel
    loads them at once.
14. **A single-file download put into the assets folder by hand is named by its own file
    name**, not the URL's last part: three models of other repositories are all published
    as `ggml-model.bin`.
15. **Preview loads a voice of its own** for the one sentence and drops it, so it never
    competes with a voice speaking in a channel.
16. **The Whisper mode's voice measures its own turns.** One latency line per answer: speech
    end → text, text → Codex's first words, first words → voice, speech end → voice. The
    output's realtime tracker is not told about the local voice (`own_latency`).
17. **The window** (built by a sub-agent from design D13): the Whisper tab with four cards
    after Config; the install dialog's second subject `whisper`; the Config tab's Vosk fields
    disabled in the Whisper mode; Status tiles for the mode, Whisper, its device and the
    voice. Two of its choices to review: the window sees a refused join through the log
    line "The Whisper mode needs downloads" (if that text changes, the dialog no longer
    opens on a refused join), and in realtime mode a model, device or voice choice saves at
    once without the dialog, which opens when the mode is switched to Whisper. It also found
    a real bug: the page's CSP had no `media-src`, so Preview's audio was blocked; fixed.

### Measured on this PC

Intel Core Ultra 9 275HX (24 logical processors), RTX 5090 Laptop GPU, 2026-11-17.

- **Whisper on the CPU** (12 threads, one 2–10 s clip): tiny 0.23–0.28 s, base 0.54–0.56 s,
  small 1.7–1.8 s, medium 6.4–9.5 s, large-v3-turbo 11 s. Whisper encodes a 30 s window,
  so the clip's length barely matters.
- **Whisper on the GPU** (CUDA 12.4 build): runs on Blackwell through the driver's JIT. The
  very first transcription after installing took 25 s; afterwards turbo q5_0/q8_0 load in
  0.45–0.6 s and transcribe a clip in 0.11–0.14 s, every model within 0.7 s. turbo q5_0
  holds about 826 MB of VRAM. No `nvrtc*.dll` is needed, and no own CUDA build.
- **The wake corpus through Whisper** (72 files in ten languages, the default phrases,
  automatic language), with the prompt: base 59, small 65, medium 64, turbo 71, large-v3
  67 of 72; Vosk had 140 of 144 on its corpus after the first change. Three noise-only
  clips (silence, steady noise, a game-like mix) gave no phrase on any model.
- **Voices** (CPU): Supertonic 3 RTF 0.13, first sentence 0.47 s; Piper RTF 0.03, first
  sentence 0.09–0.16 s; Kokoro RTF 0.19, first sentence 0.6 s; Windows voices 5–20 ms per
  sentence. Whisper large-v3-turbo read back 89–92 % of every voice's words. The WAVs are
  in `target\tts-listen\` for listening.
- **End to end, live** (Whisper turbo on the GPU, real Codex, Supertonic or Piper): from the
  end of the question to the voice 4.6–5.0 s, of which Codex took 3.7–4.0 s, Whisper
  0.23–0.24 s and the voice's first sentence 0.6–0.8 s. Questions in Russian and English,
  and the channel canary.

### Not done here

- **The private working set in a channel** (task 5.6 with `scripts\measure.ps1`): Whisper
  and the voice load only at a join, which needs a person's `/join`. It is the resource
  comparison of both modes the user planned.
- **The uninstall with "Delete the application data" checked** (task 10.2): it needs the
  uninstaller's window and deletes the real Discord token. The `whisper-models` line sits
  next to the `models` one in the hook.
- **Not tested by the agent:** a real Discord channel with two or more people and a game
  running, the Whisper mode's wake phrase in the channel, the voices by ear, and the
  comparison of both modes' resource use. Those are the user's steps.

## whisper-config-layout

The user tried build 1.1.0 the same day and rejected the separate Whisper tab: it copied
the Config tab's cards, and no Download button showed until the mode was switched. The user
approved mockups of a new layout and chose how open points go:

- **No Whisper tab.** Config → Assistant gets the assistant mode first and a grey
  Recognition field that follows it; in the Whisper mode the model row becomes the Whisper
  model with its own notice and Download, and the phrase fields hold the Whisper set. A new
  Config → Local voice card holds the voice, with a note that says it is not used in the
  realtime voice mode and needed in the Whisper mode. A new Settings → Whisper card holds
  the device, CPU threads, language, the in-use line and the models folder, with an On/Off
  label.
- **The Whisper phrases stay a separate set**, defaulting to "bro" and "peace out" whatever
  the language.
- **Downloads work in both modes** (the user's "variant A"), always only on Download. A
  download in the realtime mode installs files and loads nothing.

Decided while building:

- **Where the model is downloaded before the switch.** The Whisper model row is hidden in
  the realtime mode, so the Settings → Whisper card shows the full notice of the files the
  model chosen for the Whisper mode still needs, with Download; in the Whisper mode that
  notice moves under the model in Config, and the Settings card keeps only the CUDA
  runtime's notice under the device.
- **The managers report missing files in both modes** (`missing` in the Whisper and voice
  statuses), and each Download fetches only what its notice lists: `whisper_download` for
  Whisper's files, a new `tts_download` for the voice's; the install dialog calls both.
- **The refused join's reply** now says to open Config and press Download in the Assistant
  and Local voice cards.

Checked: the core tests, the three headless window checks (23, 44 and 34 scenarios), and
the release build: screenshots of both modes in `target\whisper-config-shots\`, and a
download in the realtime mode into an empty folder (the tiny model, 53 MB, in 4 s, nothing
loaded). Not tested: a real Discord channel with two or more people and a game running.

### The repetition filter (after the user's Discord test)

The user tested the build in a Discord channel. The tiny model wrote the user's "Bro, bro,
bro, bro" correctly, and the transcriber dropped it as a hallucination: any word four times
in a row counted as one. Of the fixes discussed after that test, the user asked only for
this one, inside this change.

- **A run of one word is a hallucination only when it is faster than five words a second
  of the utterance's speech** (and at least four words long). "Bro, bro, bro, bro" in 1.5 s
  is kept; twenty repeats in 2 s are dropped. Five is an estimate; the drop's log line now
  names the repeats and the length, so the next live test shows it.
- The other fixes discussed after the test were declined by the user and are not part of
  any change.

### The leave frees everything (the check before the commit)

The user asked for a check that leaving the channel frees the CPU, the GPU and the memory
and leaves nothing running, without Discord: a gated live test now takes the join and the
leave as the runtime does (`join_and_leave_unload_when_enabled`), three cycles each on the
GPU and the CPU with large-v3-turbo, the process measured every second. It found two gaps,
and the user chose to fix both in this change:

- **The transcription in progress stops at a leave.** On the CPU it ran 8 to 12 s past the
  leave at half the machine before the unload. whisper.cpp's abort callbacks now read a
  flag that a leave of the utterance's guild or an unload raises. whisper.cpp checks them
  only between encoder passes and decoder steps, so a stop can still wait for the pass
  running.
- **The CUDA context is reset after a model on the GPU unloads** (`cudaDeviceReset`). The
  process kept 251 MB of GPU memory, and about 255 MB more RAM than on the CPU, until the
  app was closed.
