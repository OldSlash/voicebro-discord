# Proposal

## Why

On 2026-11-22 the user asked for voice recognition tests: recorded activation phrases, a
test that shows whether each one activates the assistant and how, and a place to add their
own WAV files later. What exists today is the old wake corpus. Its files live only in
`target\wake-corpus\`, which `cargo clean` wipes and no other PC has. The expected result of
each file is written into `scripts\run-wake-corpus.ps1` or guessed from the file name by
`probe whisper-corpus`. Vosk gets the audio around the app's Discord path, Whisper gets each
file whole instead of the app's utterances, and a new WAV means editing a script. The user
wants the new test to run on every platform from WAV files recorded once, and the old
corpus tests to stay but run only on Windows.

## What Changes

- **A recorded English voice corpus in the repository**,
  `crates/voicebro-core/testdata/voice-corpus/en/`. A new probe command,
  `probe voice-corpus-make`, writes it with six voices whose output may be published: Piper
  ljspeech and cori (public domain) and four Kokoro v0.19 voices (Apache-2.0). The Windows
  voices were dropped on 2026-11-23: their terms do not allow publishing their output. The
  set has the
  default phrases "bro" and "peace out" alone, at a sentence's start and end, after "hey",
  sentences without them, look-alike words, and an activation and a deactivation in one
  file. Only English, as the user asked; other languages are left for later.
- **An `expect.txt` next to the files** says what each file must do: `activate`,
  `deactivate`, both in order, or `none`, optionally with the words spoken and the folder's
  own phrases. A WAV without a line is still run and reported, just not judged, so the user
  can drop in a file and see what happens.
- **One test, `voice_corpus_when_enabled`, checks both kinds of recognition on the app's own
  paths.** It plays every file as Discord delivers a speaker: 48 kHz frames, one per
  receive tick, nothing sent while quiet. Each file runs as recorded and with steady noise.
  - Vosk, the wake word of the realtime mode: the worker's speaker state, its trailing
    silence and the gate's rules.
  - Whisper, the phrase spotting of the Whisper and text answers modes: the app's utterance
    cutter, the model, the noise guards and the app's decision.
- **The test reports how each file activated.** For Vosk: which phrase, at which moment of
  the file, how long after the phrase ended, and what Vosk heard. For Whisper: each
  utterance, what Whisper wrote, the decision and the question left after the phrase, and
  the time it took. A summary per engine follows.
- **The new test runs on every platform the core builds on.** It reads the files from the
  repository and runs each engine whose assets are given. It fails on a wrong labeled file
  and downloads nothing.
- **The user's own WAV files** go into the same folder tree. Files under a `local\`
  folder are run but never committed, since the repository is public on GitHub.
- **The old corpus tests stay and run only on Windows.**
  - The Rust tests `vosk_corpus_when_assets_are_provided` and
    `vosk_gaps_when_assets_are_provided` are ignored on other platforms.
  - `whisper_runtime_when_enabled` skips its two corpus transcriptions there.
  - `scripts\make-wake-corpus.ps1` and `scripts\run-wake-corpus.ps1` say they are
    Windows-only and stop.
  - `probe whisper-corpus` is part of the probe, which builds only on Windows already.
- `scripts\run-voice-corpus.ps1` runs the new test with this PC's asset folders and keeps
  each report.

## Capabilities

### New Capabilities

- `voice-corpus`: the recorded voice corpus, its expectations file, the test that runs it
  through Vosk and Whisper on the app's paths and reports how each file activated, the probe
  that records the English set, and the old corpus tests limited to Windows.

### Modified Capabilities

None. The app's recognition behavior does not change.

## Impact

- The core: a test module for the corpus (WAV reading, the Discord-like playback, the Vosk
  and Whisper runs, the report), the Windows-only marks on the old corpus tests.
- The probe: `voice-corpus-make` (`apps/probe/src/`), using the app's sherpa-onnx voices
  (`tts::sherpa`) and its resampler.
- New files: `crates/voicebro-core/testdata/voice-corpus/en/tts/` (60 WAVs at 16 kHz mono,
  about 4 MB, `expect.txt` and `SOURCES.md`), `scripts\run-voice-corpus.ps1`; `.gitignore` gets the
  `local/` folders of the corpus.
- Scripts: `make-wake-corpus.ps1`, `run-wake-corpus.ps1` (the platform check).
- Docs: `docs/ARCHITECTURE.md` (testing), `AGENTS.md` (commands, corpus), `LOCAL-TESTING.md`.
- No new dependency. The branch starts from `main` after `change/cross-platform-headless` is
  merged, since the Linux run needs its Linux Vosk and Whisper libraries.
- Linux is not tested by the agent: this PC has no Linux. The user runs the test there.
