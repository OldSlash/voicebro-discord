# Tasks

## 1. Traits and spotter

- [ ] 1.1 Add `wake/mod.rs` with `SpeechRecognizer`, `LoadedModel`, `WakeEngine`, `Recognition`, `Word`, `PhraseKind`, `PhraseSink`, `WakeFeed`, `WakeStatus`, `WakeError`; verify `cargo build -p voicebro-core`
- [ ] 1.2 Add `wake/spotter.rs` from v1 `Detector`/`PhraseDetectors`/`find_occurrences` over `Recognition` (15-in-a-row activation, 5-of-10 deactivation, final always counts, once per utterance, deactivation-then-activation ignored); verify unit tests with synthetic partials for each rule and for the look-alike word

## 2. Assets (spec `wake-model-assets`)

- [ ] 2.1 Extend `catalog.rs` with `Asset {url, sha256, directory, marker, size}` for the model and the Vosk library (pins from the probe); verify a test that every entry has assets with 64-hex hashes
- [ ] 2.2 Add `wake/assets.rs`: `ensure(entry, assets_dir, progress)` with `curl.exe` download to `.part`, `sha2` check, rename, `tar.exe` extraction, marker, `cleanup_partial()`; verify a unit test with a local file URL and a wrong hash (deleted, error), and a live run that downloads both assets into a scratch dir
- [ ] 2.3 Emit `AppEvent::Assets {label, percent, error}` in ≤ 20 % steps; verify the events during the live download

## 3. Vosk engine

- [ ] 3.1 Add `wake/vosk/ffi.rs` (v1 `VoskApi`, `Model`, `Recognizer` with `Drop`, `LOAD_WITH_ALTERED_SEARCH_PATH`, log level −1, no partial word timestamps) and `wake/vosk/mod.rs` implementing the traits; verify the gated corpus test `VOICEBRO_WAKE_WAV` fires on a positive sample and `VOICEBRO_WAKE_EXPECT=none` holds on a negative one
- [ ] 3.2 Port the sequence and noise tests (`VOICEBRO_WAKE_SEQUENCES`, `VOICEBRO_WAKE_NOISE`); verify they run against the scratch assets and report pass/fail per file

## 4. Worker and manager (spec `wake-word-detection`)

- [ ] 4.1 Add `wake/worker.rs`: thread with 20 ms tick, 256-chunk channel, per-speaker state with downsampler, trailing silence (60 ms → up to 1.5 s), 30 s idle drop, 1.5 s reactivation guard, drop counting and periodic log; verify unit tests with a fake recognizer for trailing silence and idle drop
- [ ] 4.2 Add `WakeManager::configure(settings)`: load only in wake-word mode, reload on model change, rebuild spotters on phrase change, vocabulary check into `WakeStatus.unknown_words`, "no phrases configured" state; verify unit tests with a fake engine for each transition
- [ ] 4.3 Wire the receive path: per-speaker frames to `WakeFeed`; phrase hits to a logging `PhraseSink`; `AppEvent::Wake` and `Status.wake`; verify a live run in the test guild where saying a configured phrase produces the decision line in the log

## 5. Close-out

- [ ] 5.1 Desktop Status tab shows the wake state, model, download progress, unknown words and drops; verify in the running app while enabling wake-word mode
- [ ] 5.2 Update `docs/ARCHITECTURE.md` 4.4, `AGENTS.md` (commands for the corpus tests), `LOCAL-TESTING.md` (where the corpus WAVs are); verify no stale statement remains
- [ ] 5.3 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass

## Workflow follow-up

- Checkpoint commit per task group on `change/wake-word-engine`.
- Merge into `main` with `--no-ff --log` and archive the change when the user says so.
