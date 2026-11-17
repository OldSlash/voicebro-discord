# Proposal

## Why

The user tried build 1.1.0 on 2026-11-17 and found the Whisper mode hard to use. Its
settings sat on a separate Whisper tab that copied the Config tab's cards instead of
extending them. The Download buttons showed only after the mode was switched to Whisper,
so the user saw no way to download anything. The user approved mockups of a new layout the
same day and asked for it to be built through OpenSpec.

Testing the new layout in a Discord channel the same day, the user said "bro, bro, bro,
bro" to wake the assistant. Whisper wrote it so, and the transcriber dropped it as a
hallucination, because any word four times in a row counted as one. The user asked for
that filter to be tied to the length of the speech, in this change.

Before the commit the user asked for a check that leaving the channel frees everything and
leaves nothing running. It found two gaps, and the user asked for both to be fixed here: on
the CPU the utterance being transcribed at the leave ran to its end (8 to 12 s at half the
machine) before the unload; on the GPU the process kept 251 MB of GPU memory, and about
255 MB of RAM, until the app was closed.

## What Changes

- **The Whisper tab is removed.** Its fields move to the Config and Settings tabs.
  **BREAKING** for anyone used to the tab; no setting changes its name or meaning.
- **Config → Assistant gets the assistant mode as its first field.**
  - A grey Recognition field follows the mode: Vosk or Whisper.
  - In the Whisper mode the wake model field becomes the Whisper model list, with its own
    download notice and Download button, as the Vosk model has today.
  - The phrase fields show the Vosk model's phrases in the realtime mode and the separate
    Whisper set in the Whisper mode.
  - The Vosk fields are no longer disabled with a note pointing to another tab.
- **The Whisper phrases default to "bro" and "peace out"** for every language, instead of
  the wake catalog's defaults for `whisper.language`.
- **A new Config card, Local voice**, holds the voice, the speed, Preview and the voice's
  download notice. Its note follows the mode: not used in the realtime voice mode; needed
  in the Whisper mode.
- **A new Settings card, Whisper**, holds:
  - an On/Off label that follows the assistant mode;
  - the device, the CPU threads and the language;
  - the CUDA runtime's notice and Download button under the device when NVIDIA GPU is chosen;
  - the in-use line (what is loaded, the device, the last answer's timings);
  - Open Whisper models folder.
- **Download works in both modes.** The user can download Whisper's and the voice's files
  while the realtime mode is still on, and nothing is ever fetched before the user presses
  Download.
- The install dialog stays. It lists the missing files of both Whisper and the voice, and
  the refused `/join`'s reply and its texts point to Config → Assistant instead of the
  Whisper tab.
- **The repetition filter follows the speech's length.** A run of one word is a
  hallucination only when it is faster than five words a second of the utterance's speech,
  so "bro, bro, bro, bro" said by a person is kept. The log line of a dropped one names the
  repeats and the length.
- **A leave stops the transcription in progress.** whisper.cpp's abort callbacks read a flag
  that a leave of the utterance's guild, or an unload, raises; the text of a stopped
  utterance is dropped and the model unloads without waiting for it.
- **The GPU's memory goes back after the unload.** Once a model on the GPU is freed, the app
  resets the process's CUDA context (`cudaDeviceReset`); the next join creates a new one.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings-ui`: the Whisper tab's cards are removed; the Assistant card gets the mode, the
  Recognition field, the Whisper model and the phrase fields that follow the mode; the new
  Local voice and Settings → Whisper cards.
- `app-window`: the tab list without Whisper; the Config tab's cards; when the Whisper
  install dialog opens.
- `app-settings`: the Whisper phrases' defaults.
- `wake-model-assets`: the Whisper and voice files can be downloaded in either mode.
- `codex-assistant`: the refused join's reply points to the Config tab.
- `whisper-recognition`: the GPU fallback is shown on the Settings tab's Whisper card; the
  repetition filter follows the speech's length; a leave stops the transcription in
  progress, and the CUDA context is reset after a model on the GPU unloads.
- `whisper-assistant-mode`: the answer's timings are shown on the Settings tab's Whisper
  card.

## Impact

- The window: `apps/desktop/ui/index.html`, `main.js`, `styles.css`; the command mock
  `ui-dev/mock.js`; the headless checks (`check-whisper-tab.mjs` becomes a check of the new
  layout; `check-config-tab.mjs` and `check-wake-dialog.mjs` follow).
- The core: the Whisper phrase defaults in `settings/mod.rs`; the Whisper and voice
  managers report missing files and accept Download in both modes
  (`stt/manager.rs`, `tts/manager.rs`); the refused join's reply text; the repetition
  filter in `stt/transcriber.rs` and its callers in the probe and the Whisper text-turn
  test; the abort (`stt/mod.rs`, `stt/transcriber.rs`, `stt/whisper/mod.rs`) and the CUDA
  reset (`stt/whisper/ffi.rs`, `stt/manager.rs`), with a gated live test of the join and
  the leave.
- The commands keep their names. `whisper_download` and the status's `whisper` and `tts`
  parts gain the missing files in the realtime mode.
- Docs: `docs/ARCHITECTURE.md` 4.8 and 4.12, `docs/user-guide.md` section 7, `AGENTS.md`.
- No new dependency, no settings schema change: a stored `whisper.phrases` list keeps its
  value; an absent one now means "bro" and "peace out".
