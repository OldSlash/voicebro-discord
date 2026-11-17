# Design

## Context

Change `whisper-mode` (branch `change/whisper-mode`, not merged yet) built the Whisper mode
with its own Whisper tab: four cards (Mode, Speech recognition, Phrases, Voice) in
`apps/desktop/ui/index.html` and `main.js`, plus disabled Vosk fields with a note on the
Config tab. Two facts of the current code shape this change:

- **The download notices show only in the Whisper mode.** `WhisperManager::configure` and
  `TtsManager::configure` set the state `Off` in the realtime mode and compute no missing
  files, and `download` is refused unless the state is `NeedsDownload` or `Error`. The
  window draws a notice from the state, so in the realtime mode there is none.
- **The Whisper phrases' defaults follow `whisper.language`.**
  `WhisperSettings::phrases_in_effect` takes the wake catalog's defaults of that language,
  English for `auto`.

The mockups the user approved on 2026-11-17, and the answers that followed, are the layout:

- phrases: a separate Whisper set, default "bro";
- downloads: possible in both modes (variant A), always only on Download;
- the Recognition field: grey, following the mode;
- the install dialog: kept, and it also shows that the voice is not downloaded.

## Goals / Non-Goals

**Goals:**

- One place per setting: the bot's settings on Config, how Whisper runs on this PC on
  Settings. No tab that repeats another.
- A Download button wherever a missing file is chosen, in both modes.

**Non-Goals:**

- Changing what the Whisper mode does, its catalogs, its settings names or the status
  tiles on the Status tab.
- A Download in the realtime mode loading anything: files only.

## Decisions

### D1. The managers report missing files in both modes

- `WhisperStatus` and `TtsStatus` gain `missing: Vec<DownloadItem>`: the files of the
  selected model and device (Whisper), or of the selected voice (voice), that are not on
  disk. `configure` computes it in every mode, in the calling thread, without the network,
  as it does today in the Whisper mode.
- The state keeps its meaning. In the realtime mode it stays `Off`, and the window draws the
  notices from `missing`, not from the state.
- `download(model)` / `download(voice)` is accepted whenever the selected item has missing
  files, in either mode, and is still refused for anything else. It records the consent as
  today and starts the background job.
- In the realtime mode the job downloads and installs, publishes progress and ends with
  `Off` and an empty `missing`. It loads nothing: `wanted` is false outside a join in the
  Whisper mode.
- **Alternative:** a separate command that lists files for the window. Rejected: the status
  already carries the notice's items, and two sources could disagree.

### D2. The Whisper phrases default to "bro" and "peace out"

`WhisperSettings::phrases_in_effect` returns the stored list, or the activation "bro" or the
deactivation "peace out" for a list never set, whatever `whisper.language` is (the user's
decision of 2026-11-17).

- No schema change: an absent list keeps meaning "the default".
- A stored list keeps its value.
- The wake models' phrases (`wake.phrases.<model>`) stay apart and unchanged.

### D3. The Assistant card swaps rows, not data

The Assistant card keeps one field per setting path, and the mode decides which rows are
shown:

- Assistant mode (`assistant.mode`) is the first field.
- Recognition is a read-only grey field: "Vosk" or "Whisper".
- Model:
  - in the realtime mode, the existing wake model row (`wake.model`) with its notice;
  - in the Whisper mode, a Whisper model row (`whisper.model`) in the same place, with the
    grouped list and its own notice.
- Phrases:
  - in the realtime mode, the existing fields of the selected Vosk model;
  - in the Whisper mode, a second pair bound to `whisper.phrases`.
- Activation (`session.mode`) stays below the phrases.

The wake engine select disappears into the Recognition field: Vosk was its only value.

- **Alternative:** one select whose options change with the mode. Rejected: the form keeps
  its state apart from the status, and one control writing two paths would break the rule
  that a save writes only the field changed.

### D4. The Local voice card and the Settings → Whisper card

- **Local voice** follows the Assistant card. It holds:
  - the voice row, the speed and Preview, moved as they are from the Whisper tab;
  - a mode note on top: realtime voice says the local voice is not used; Whisper says it
    reads the answers;
  - the voice's notice from `tts.missing`.
- **Settings → Whisper** follows Diagnostics. It holds:
  - an On/Off label from `assistant.mode`;
  - the device, CPU threads and language rows;
  - under the device, the CUDA runtime's notice: the runtime items of `whisper.missing`,
    while NVIDIA GPU is chosen;
  - in the realtime mode instead, the full notice of every Whisper file the model chosen
    for the Whisper mode still needs, with Download. **Found while building:** the Whisper
    model row is hidden in the realtime mode, so without this the model could not be
    downloaded before the switch, which the user's variant A asks for. The Settings card
    is always shown, so it carries that notice; in the Whisper mode the notice moves under
    the model on Config;
  - the in-use line, moved from the Whisper tab's Mode card;
  - "Open Whisper models folder".
- Both cards fold, with the ids `local-voice` and `whisper` in `ui.collapsed_cards`. Ids
  stored for the removed cards are ignored.

### D5. Download buttons and the install dialog

- **One action per button.** The Assistant card's Download calls `whisper_download` for the
  model. The Local voice card's calls the voice's download. The Settings card's calls
  `whisper_download` too: the runtime is part of the model's set for that device.
- **The command splits.** `whisper_download` today downloads both parts. It becomes
  `whisper_download(model)` for Whisper's files, and a new `tts_download(voice)` fetches the
  voice's, so each notice fetches only what it lists. The install dialog calls both.
- **When the dialog opens:** on a refused join, on a switch to Whisper with files missing,
  and on a model, device or voice choice that adds missing files while the mode is Whisper.
  In the realtime mode a choice saves at once, and its card shows the notice.
- **Texts:** the refused join's reply (`JOIN_NEEDS_DOWNLOADS`) and the dialog point to
  Config → Assistant and Local voice. The window recognizes the reply's text, so both change
  together, and the headless check covers it.

### D6. The checks

- `check-whisper-tab.mjs` becomes `check-whisper-config.mjs`. It covers the new layout's
  scenarios of the `settings-ui` and `app-window` deltas.
- `check-config-tab.mjs` and `check-wake-dialog.mjs` are updated where they named the
  Whisper tab or the disabled Vosk fields.
- The mock keeps its query parameters (`?whisper`, `?whisper_missing`, `?gpu_fallback`) and
  adds `missing` to its statuses in the realtime mode.

### D7. The repetition filter follows the speech's length

The user's test of 2026-11-17 lost a real "Bro, bro, bro, bro" (the tiny model, 12:48:45):
`clean` dropped every text with one word four times in a row. A person can say a word four
times in a second or two; Whisper's decoding loop writes it dozens of times over a short
clip.

- A run of one word is a hallucination when it is at least four words long **and** longer
  than the utterance's speech could hold at five words a second
  (`MAX_REPEATS_PER_SECOND`). The longest run in the text counts.
- `clean` takes the utterance's speech length (`Utterance::speech_seconds`, before the
  padding); the probe passes the WAV's length, the text-turn test the clip's.
- The info line of a drop names the repeats and the length, so a live test shows why.
- **Why five:** an estimate, not a measurement. A person repeating a short word with
  pauses between stays below five a second, and a loop of dozens of words over a clip of a
  second or two is far above it. The next live test checks it: the drop's log line shows
  the repeats and the length.
- **What else guards the noise:** Silero VAD, the no-speech threshold and whisper.cpp's
  temperature fallback, which decodes again when a text's entropy is too low, as a loop's
  is. In all 20 corpus runs of change `whisper-mode` (ten models, with and without the
  prompt) the three noise clips gave no text at all, so the repetition filter never fired
  there.
- **Alternative:** raising the fixed count from four. Rejected: any fixed count drops a
  person who says the phrase that many times, and the user asked for the length to decide.

### D8. A leave stops the transcription in progress

The check of task 6.4 found that the utterance being transcribed at a leave ran to its end:
`unload` waits for the model's lock, which the transcription holds, 8 to 12 s for
large-v3-turbo on the CPU.

- `SpeechToText::transcribe_until(audio, options, abort)` takes an `AbortFlag` (a shared
  `AtomicBool`); the default runs `transcribe`, so the fakes need nothing new.
- `WhisperModel` hands the flag to whisper.cpp twice: `abort_callback`, checked after each
  encoder pass and decoder step, and `encoder_begin_callback`, checked before each encoder
  pass after the language detection's. Neither stops a pass in the middle: the public API
  has no way into ggml's graph, so a stop waits for the pass running.
- The queue owns the flag and knows the guild being transcribed. `pop` lowers it,
  `clear_guild` of that guild and `close` raise it, `finish` tells the thread whether its
  utterance was stopped. A stopped utterance gives no text and one info line; it is not
  counted as transcribed.
- A leave raises it at once (`clear_guild`), an unload through the pipeline's close, so the
  model's lock is free when `unload` takes it.
- **Alternative:** a smaller `audio_ctx` to shorten each pass. Rejected: the user declined
  changing what Whisper computes.

### D9. The CUDA context goes after a model on the GPU

The same check found 251 MB of GPU memory, and about 255 MB more RAM than after a model on
the CPU, held after the unload until the app was closed: the CUDA context that ggml-cuda
creates on first use and that freeing the model leaves alive.

- `WhisperRuntime::reset_cuda` calls `cudaSetDevice(0)` and `cudaDeviceReset` from the
  loaded `cudart64_12.dll`, the module ggml-cuda imports. Device 0 is the one a model is
  loaded on.
- `unload` returns whether the freed model was on the GPU. The reset runs only with the
  manager's busy lock held, so no load or download runs meanwhile: after the last leave the
  unload thread holds it; at a switch to the realtime mode the reset is skipped when the
  lock is taken and left for the next unload.
- The next join loads into a new context. The libraries stay loaded, as before.
- **Alternatives:** unloading the libraries, rejected as before (ggml's registry is global);
  Whisper in a child process, rejected as too large for this change.

## Risks / Trade-offs

- [The Assistant card grows] → the rows of the other mode are hidden, not disabled. The
  card holds as many rows as before plus the mode and Recognition.
- [A Download in the realtime mode fetches up to 1.6 GB the user may never use] → the same
  rule as everywhere: nothing without Download, and the notice states the sizes.
- [The window still recognizes the refused join by its log text] → kept from `whisper-mode`.
  Changing the text needs the check updated; the check fails otherwise.
- [ggml-cuda keeps state across a context reset that a later load trips on] → the gated
  check loads, transcribes and unloads three times on the GPU after each reset; a failure
  there blocks the commit.
- [A stop waits for the pass running: up to one encoder pass, about 6 s for
  large-v3-turbo on the CPU] → the gated check measures it; finer stops need a change in
  whisper.cpp.
- [A short noise clip that Whisper turns into four to five repeats a second now passes]
  → it must first pass the VAD and the no-speech threshold, which caught every noise clip
  of the corpus; a drop or a kept text both appear in the detail log for the next test.

## Migration Plan

- No settings migration.
- A stored `ui.collapsed_cards` with the old Whisper card ids is harmless.
- Rollback is the previous build.
