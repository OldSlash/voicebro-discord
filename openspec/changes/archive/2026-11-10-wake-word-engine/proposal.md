# Proposal

## Why

The assistant must be hands-free: a phrase opens a session, another closes it, with no cloud
and no transcription of the whole channel. The first version solved this with Vosk and
learned the rules the hard way (open vocabulary, separate detectors per phrase kind,
trailing silence, no reset after a hit). This change brings that engine over behind traits,
so the engine and the model are settings and a different recognizer can be added later
without touching the phrase logic.

## What Changes

- `voicebro-core::wake`: the traits `WakeEngine`, `LoadedModel`, `SpeechRecognizer`; the
  engine-independent `PhraseSpotter` (v1 detector rules); the worker thread with one
  recognizer per speaker, trailing silence and the reactivation guard; `WakeManager` that
  loads the engine only in wake-word mode and reconfigures on settings changes.
- `wake/vosk/`: the v1 FFI wrappers over `libvosk.dll` loaded at runtime.
- `wake/assets.rs`: first-use download of the library and the model with pinned SHA-256,
  temp-and-rename, extraction with the system `tar.exe`, marker files, cleanup of partial
  downloads, progress events.
- The catalog gains the download URLs and hashes for the one model.
- `AppEvent::Wake` and `WakeStatus` (state, model, download progress, missing vocabulary,
  "no phrases configured", drops).
- The receive path feeds each speaker's frames to the wake engine (`WakeFeed`); phrase hits
  go to a `PhraseSink` (the gate, in the next change; a logging sink here).

## Capabilities

### New Capabilities

- `wake-word-detection`: when a phrase counts, per speaker, with the rules that survived
  real use, and what the status and the log say about it.
- `wake-model-assets`: how the recognizer library and model get onto the machine and are
  loaded, and how the app behaves without them.

### Modified Capabilities

None. The phrase and model settings already exist in `app-settings`.

## Impact

- New module `crates/voicebro-core/src/wake/` with `vosk/`; `catalog.rs` extended with
  assets; `runtime`, `status`, `events` extended.
- New dependencies: `libloading` 0.9, `sha2` 0.11. Downloads use the system `curl.exe` and
  `tar.exe` (no HTTP crate).
- About 150–200 MB of RAM while the model is loaded; nothing while wake-word mode is off.
- Test corpus (WAVs) lives outside the repository; tests are gated by environment variables.
