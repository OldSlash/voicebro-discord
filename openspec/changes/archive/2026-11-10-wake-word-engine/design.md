# Design

## Context

See proposal.md. Target: `docs/ARCHITECTURE.md` 4.4. The parts: FFI wrappers, the phrase
detectors, the worker, the corpus tests. The Vosk assets and their hashes were verified on
2026-11-09 (`probe vosk-assets`); the library loads in about 140 ms and the model in about
370 ms. Phrases are the user's and may be empty.

## Goals / Non-Goals

**Goals:** the detection rules behind engine-independent traits; assets handled safely;
the worker fed from the receive path; the wake state in the status and the log.

**Non-Goals:** no second engine, no language catalog beyond the one Russian model, no
grammar-restricted recognition, no partial word timestamps.

## Decisions

**D1. Three layers.** `SpeechRecognizer` (per speaker, `accept`/`flush` → `Recognition`),
`LoadedModel` (shared, `new_recognizer`, `knows_word`), `WakeEngine` (assets, `load`).
`PhraseSpotter` consumes `Recognition` and knows nothing about Vosk. Alternative: a single
module. Rejected: the engine must be replaceable (architecture 4.4).

**D2. Worker thread with a 20 ms tick.** Bounded `sync_channel` of 256 chunks
from the receive path (`WakeFeed::feed(guild, ssrc, frame)`), per-speaker state (recognizer,
spotter, downsampler 48 k stereo → 16 k mono, last audio, silence fed), trailing silence,
30 s idle drop, the 1.5 s reactivation guard. Hits go to a `PhraseSink` trait
(`on_phrase(guild, kind, phrase, ssrc, since_phrase_end)`); this change ships a logging sink,
the gate implements it next.

**D3. Assets through `curl.exe` and `tar.exe`.** No HTTP crate: both tools ship with
Windows 10+ and the probe used them. Download to `<name>.part`, hash with `sha2`, rename,
extract, write `.complete` marker. Progress is read from the growing `.part` size against the
catalog's expected size.

**D4. The catalog carries the assets.** `ModelEntry` gains `assets: &[Asset]` (url, sha256,
directory, marker, size) for the model and the engine's library; the settings still refer to
the model id only.

**D5. `WakeManager` owns the engine lifecycle.** `configure(settings)` compares mode, model
and phrases with the current state: mode off → unload; model changed → unload, ensure
assets, load; phrases changed → rebuild spotters and re-check vocabulary. All of it off the
UI thread; results as `AppEvent::Wake` and `WakeStatus`.

**D6. Vocabulary check once per configure.** `knows_word` for each word of each phrase; the
unknown ones go into `WakeStatus.unknown_words`, nothing is silently dropped.

**D7. Corpus tests stay environment-gated** (`VOICEBRO_VOSK_DIR`, `VOICEBRO_WAKE_WAV`,
`VOICEBRO_WAKE_EXPECT`, `VOICEBRO_WAKE_SEQUENCES`, `VOICEBRO_WAKE_NOISE`), plus
engine-independent unit tests for `PhraseSpotter` with synthetic `Recognition`s.

## Risks / Trade-offs

- [Vosk small-ru fails in heavy noise (amplitude ≥ 1500 for activation)] → known,
  reported in the status as drops and misses are logged; a better engine is a later change.
- [Partial results lag ~150 ms] → the spotter's timing uses the lag constant when the engine
  gives no word timing.
- [Model memory 150–200 MB] → loaded only in wake-word mode; unloaded on mode change.

## Open Questions

None.
