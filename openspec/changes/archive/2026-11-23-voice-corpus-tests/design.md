# Design

## Context

See proposal.md for the request. The pieces the test builds on already exist in the core:

- The wake worker's speaker state (`wake::worker::SpeakerState`) takes a speaker's 48 kHz
  stereo frames with their receive tick (`accept_discord`) and feeds the trailing silence
  counted in ticks (`on_tick`), returning the confirmed phrases with their delay after the
  phrase's end. The old gaps test (`vosk_gaps_when_assets_are_provided`) already drives it
  this way; the old corpus test feeds 16 kHz mono directly (`accept_mono`).
- Whisper's utterance cutter (`stt::cutter::Cutter`) takes the same frames and ticks and
  returns utterances or dropped noise. `stt::transcriber::clean` applies the noise guards
  and `stt::phrases::decide` turns a text into an activation, a deactivation, a question or
  nothing. `probe whisper-corpus` loads the runtime and the model through the asset store
  (`apps/probe/src/whisper.rs`).
- The gate applies the rules of state (`gate.rs`): activation only while waiting,
  deactivation only while active, no activation within `wake::REACTIVATION_GUARD` (1.5 s)
  of a deactivation.
- The app's voices run on sherpa-onnx (`tts::sherpa::SherpaVoice`): Piper voices at
  22.05 kHz and Kokoro v0.19 at 24 kHz, Piper's and Kokoro's text phonemized by
  piper-phonemize. Their files come through the asset store with pinned hashes. The Windows
  voices (`tts::windows`) were the first choice and were dropped: their terms do not allow
  publishing their output (Risks).
- `audio::dsp::Resampler` converts any rate to any rate. The core's two WAV readers
  (`stt::read_wav_16k`, `tts::windows::parse_wav`) take only 16-bit PCM.
- `change/cross-platform-headless` (not merged yet) adds the Linux Vosk and Whisper
  libraries and the shared native-library loader. The Linux run of this test needs it.
- `.gitattributes` already marks `*.wav` binary. This PC has no Linux (no WSL).

## Goals / Non-Goals

**Goals:**
- A corpus any checkout has, run through both recognizers on the code paths the app uses,
  with a report that says how each file activated.
- Adding a WAV, labeled or not, takes no code change.

**Non-Goals:**
- Other languages, the Windows voices, the Codex voice. English only, as asked.
- Driving the whole gate, the assistant session or Discord. The test checks recognition and
  the rules of state, not the session's effects.
- Any change to how the app recognizes speech.
- Any change to the old corpus tools on Windows.

## Decisions

### D1. The corpus is test data of the core

The files live in `crates/voicebro-core/testdata/voice-corpus/`, next to the core's other
test data (`fake-codex`, `fake-opencode`). The test finds them through `CARGO_MANIFEST_DIR`,
so it needs no path on any platform; `VOICEBRO_VOICE_CORPUS` overrides it. One line in
`.gitignore`, `crates/voicebro-core/testdata/voice-corpus/**/local/`, keeps the user's
private files out of git.

Alternatives: `target\` (wiped by `cargo clean`, absent on other PCs) and an ignored folder at
the root (absent on other PCs), both rejected because the user wants the test on every
platform. Git LFS rejected: an extra tool for about 4 MB.

### D2. The English set is recorded by a probe command with voices whose output may be published

The repository is public, so every voice of the set must allow publishing what it says
(user's decision of 2026-11-23, after the Windows voices failed that test; Risks). Each
license was read from the voice's model card and, for a dataset, from its own page:

| Name | Voice | License, as read | Why it qualifies |
|---|---|---|---|
| `ljspeech` | Piper `en_US-ljspeech-high` (American, female) | public domain; LJ Speech has "no restrictions on its use" | trained from scratch on LJ Speech |
| `cori` | Piper `en_GB-cori-high` (British, female) | public domain | trained from scratch on LibriVox recordings |
| `bella`, `adam`, `emma`, `george` | Kokoro v0.19 `af_bella`, `am_adam`, `bf_emma`, `bm_george` (American and British, female and male) | Apache-2.0 | no clause on the output; the card says it was trained on permissive or non-copyrighted audio |

`probe voice-corpus-make` installs each voice's files through the asset store (a missing
one is downloaded with its pinned hash): cori and Kokoro from the app's voice list, and
ljspeech, which the app does not list, from an `Asset` the probe pins itself. It
synthesizes each case at speed 1.0, resamples it to 16 kHz with `audio::dsp::Resampler`,
and writes 16-bit mono WAVs named `<voice>-<case>.wav`. It deletes `voice-corpus/en/tts/` and
writes it again with its `expect.txt`, which starts with a comment that explains the format
for the user's own folders, and a `SOURCES.md` that says every file is machine generated and
names each voice, its license and its model card. The pause of the last case is 1.5 s of
zeros between two syntheses.

Rejected voices: the Windows voices (their terms, Risks); Piper `lessac` (a research-only
licence that forbids distribution) and `libritts_r` (CC BY 4.0 data, but fine-tuned from
`lessac`); Piper `alba` (fine-tuned from another base, its dataset licence file not read);
Supertonic 3 (OpenRAIL-M allows it only with a machine-generated notice and use
restrictions: kept out to keep the set simple).

| Case | Spoken | Expected |
|---|---|---|
| `bro-alone` | Bro. | activate |
| `bro-start` | Bro, what time is it in London? | activate |
| `hey-bro` | Hey bro, what is the weather in Bangkok? | activate |
| `bro-end` | That was a good game, bro. | activate |
| `plain` | Let's go to the castle and wait for the others. | none |
| `brother` | Brother, tell me a joke. | none |
| `lookalike` | I broke my bow, he brought the brown boat, nice bros. | none |
| `peace-alone` | Peace out. | deactivate |
| `goodbye` | Thanks for the help. Peace out. | deactivate |
| `bro-then-peace` | Bro, what time is it in London? (1.5 s) Thanks, peace out. | activate, deactivate |

Ten cases and six voices make 60 files. The sentences of the first nine cases are the old
English set's, so results can be compared.

Alternative: extend `scripts\make-wake-corpus.ps1`. Rejected: it uses the Windows voices
through `System.Speech`, and the user wants the old tools left as they are.

### D3. `expect.txt`

One file per folder, UTF-8, read line by line:

```
# comment
phrases = bro, hey bro | peace out
bro-start.wav = activate ; Bro, what time is it in London?
bro-then-peace.wav = activate, deactivate
plain.wav = none
```

- A file line names a file in the same folder, exactly as written (Linux is case-sensitive).
- The changes are `activate`, `deactivate` or `none`, comma-separated, in order. A list
  that begins with `deactivate` starts the file with a session active.
- The text after `;` is only shown in the report, never judged.
- A `phrases` line applies to its folder and every folder below that sets none, up to the
  language folder. It is read the way the settings fields are (`a, b | c`), so the user can
  paste their own lists.
- A line that is none of these is an error with its line number.

### D4. Reading any common WAV

The test has its own RIFF reader: integer PCM of 16 or 24 bits and float of 32 bits,
including `WAVE_FORMAT_EXTENSIBLE` with the PCM or float subformat, which audio editors write
for 24-bit and float files. Channels are averaged to mono, then the file is converted to
48 kHz with `Resampler::convert` and doubled to stereo frames of 960 samples per channel. A
last partial frame is padded with zeros. Anything else is reported with its format tag, bit
depth and channel count.

Alternatives: the `hound` crate (a new dependency for test code), and widening
`stt::read_wav_16k` (app code the probes use). Both rejected; the reader is test code and
small.

### D5. Playing a file as Discord delivers a speaker

The test numbers receive ticks from 1, one per 20 ms frame of the file. A frame with a peak
of 256 or more is audible and is sent. The 5 frames after the last audible one are sent too,
then nothing until the next audible frame; the ticks go on. After the file, 100 more ticks
(2 s) pass with nothing sent, enough for Vosk's 1.5 s of trailing silence and the cutter's
700 ms.

256 is the peak the app itself treats as audible when it decides to send toward Discord. The
5 frames are the end of speech the old gaps test models. Discord's own voice activity
detection is not known in detail. For the recorded set it does not matter, since its pauses
are digital silence.

The noise run adds the old corpus test's steady noise (xorshift, seed `0x2545_f491`, peak
300) to both channels of every frame and 3 s more of it after the file. Every frame is then
audible and sent, as from an open microphone, and Vosk must end the utterance without
silence.

Both engines get the same frames and ticks.

### D6. The Vosk run

The model is the `en` catalog entry's folder in `VOICEBRO_VOSK_DIR`, loaded once. Each file
run gets a fresh `SpeakerState`. Each tick calls `accept_discord` when a frame is sent, then
`on_tick`, as the worker does. The test takes the final texts with `take_heard` after each
call and a deactivation seen but never confirmed with `take_near_miss`. A hit's moment in the
file is its tick × 20 ms; the phrase ended its delay before that. Before the files, the
folder's phrases go through `worker::usable_phrases`. Unknown words make that folder's
labeled files Vosk errors, as the app would not spot them.

### D7. The Whisper run

The runtime, the VAD model and the model load as in `probe whisper-corpus`. The test checks
`assets::is_installed` instead of calling `assets::ensure`, so it never downloads. A missing
file fails the test with its name. The model is `VOICEBRO_WHISPER_MODEL` (default
`stt::catalog::DEFAULT_MODEL`), the device `VOICEBRO_WHISPER_DEVICE` (default `cpu`), with
the fallback the app uses. The options are the folder's language (`en`), automatic threads
and `phrases::prompt` of the folder's phrases.

Each run gets a fresh `Cutter`. Each tick calls `on_frame` (guild 1, one speaker) when a
frame is sent, then `on_tick`. Each `Cut::Utterance` is transcribed and timed, passed
through `clean` with `NO_SPEECH_THRESHOLD` and its speech length, then through `decide` in
the wake word session mode with the current state. Each `Cut::Noise` is reported as
dropped. The test tracks each utterance's first and last frame tick for its span. "How long
after the speech" is the ticks from the last frame to the cut plus the transcription time:
the app's delay without its queue.

### D8. The rules of state are the gate's, applied by the test

A small state per run: waiting or active, and the file time of the last deactivation. A
Vosk hit or a Whisper decision changes it only under the gate's rules, with the gate's
constant `REACTIVATION_GUARD`. Otherwise it is reported as ignored, with the rule. A Whisper
text that is neither phrase is reported as a question or as nothing, as `decide` says. The
implementation checks `gate.rs` for how Whisper's decisions are applied, the guard included,
and follows it exactly.

Alternative: drive the real `Gate`. Rejected: it needs outputs, a session, settings and an
effect sink, none of which the corpus is about.

### D9. One gated test in the core

A test-only module `voice_corpus` in `voicebro-core` (`#[cfg(test)]`, so it can use the
crate's own items) holds `voice_corpus_when_enabled`. It runs Vosk when `VOICEBRO_VOSK_DIR`
is set and Whisper when `VOICEBRO_WHISPER_DIR` is set. With neither it returns at once, like
the other gated tests. It prints the report with `eprintln!` (seen with `--nocapture`),
collects the failures, and asserts once at the end.

`scripts\run-voice-corpus.ps1` puts CMake on `PATH` as the other scripts do. By default it
sets `VOICEBRO_VOSK_DIR` to `target\wake-corpus\assets` and `VOICEBRO_WHISPER_DIR` to
`target\whisper-test\assets`, overridable by parameters. It runs
`cargo test --release -p voicebro-core voice_corpus_when_enabled -- --nocapture`, writes
the output to `target\voice-corpus\runs\<yyyy-MM-dd-HHmmss>.txt` and exits with the test's
code. On Linux the same `cargo test` line with the two variables is the way to run it.

### D10. The old corpus tests only on Windows

- `vosk_corpus_when_assets_are_provided` and `vosk_gaps_when_assets_are_provided` get
  `#[cfg_attr(not(windows), ignore = "the old wake corpus is made with the Windows voices")]`,
  so other platforms list them as ignored with the reason.
- `whisper_runtime_when_enabled` keeps its check of the runtime's defaults everywhere and
  transcribes the two corpus files only `if cfg!(windows)`. Elsewhere it prints that it
  skipped them.
- `make-wake-corpus.ps1` and `run-wake-corpus.ps1` start with
  `if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { ...; exit 0 }`, which
  works in Windows PowerShell 5.1 (no `$IsWindows` there) and in PowerShell 7 on Linux.
- `probe whisper-corpus` needs nothing: the probe crate uses Windows APIs unconditionally
  and builds only on Windows.
- `catalog_models_load_when_enabled` reads no corpus file and stays as it is.

Alternative: `#[cfg(windows)]` on the tests. Rejected: they would vanish elsewhere instead of
being skipped with a reason.

### D11. The report

One block per file. An example of its shape (the numbers are made up):

```
en/tts/emma-bro-end.wav  want: activate  said: "That was a good game, bro."
  vosk     clean  ok     activate «bro» confirmed at 1.86 s, 0.31 s after the phrase ended; heard "that was a good game bro"
  vosk     noise  ok     activate «bro» confirmed at 2.40 s, 0.85 s after the phrase ended; heard "that was a good game bro"
  whisper  clean  ok     0.04–1.62 s: "That was a good game, bro." (190 ms) -> activate «bro», no question; 0.89 s after the speech
  whisper  noise  WRONG  0.00–4.62 s: "That was a good game." (205 ms) -> nothing
```

Then, per engine and run: right of labeled, false activations, unlabeled, errors, and the
median transcription time. Finally the list of wrong files and errors. Files run in path
order.

## Risks / Trade-offs

- [The license terms for sharing Windows voice output in a public repository are not
  checked] → A task checks them before the files are committed. If they do not allow it, the
  agent asks the user before committing anything.
  - Checked on 2026-11-23 (task 2.3). Sources: the Windows 11 Microsoft Software License
    Terms (`https://microsoft.com/content/dam/microsoft/usetm/documents/windows/11/oem-(pre-installed)/UseTerms_OEM_Windows_11_English.pdf`;
    the HTML pages of the same terms answered 404, and the PDF's text is stored as font
    glyphs that could not be extracted without installing a PDF tool), a web search of the
    terms, and a Microsoft Q&A answer
    (`https://learn.microsoft.com/en-us/answers/questions/326449/using-microsoft-tts-for-commercial-use`).
  - Finding: nothing found in the Windows terms speaks of the speech voices' output or
    allows publishing it; the Q&A answer, which is not part of the terms, says the voices are
    licensed with Windows and that distributing the audio files needs a commercial license.
    So the terms do not clearly allow it: the 30 recorded WAVs and their `expect.txt` stay
    on disk, uncommitted, until the user decides (record with another voice, keep them
    local, or commit after reading the terms).
  - The user's decision of 2026-11-23: do not publish them; record the set with voices whose
    output may be published (D2). The Windows recordings were moved out of the repository
    (to `target\voice-corpus-windows-old\` on the test PC) and the probe no longer uses the
    Windows voices.
- [A voice's license is misread or its training data is not what its card says] → Each
  license is quoted from the model card and the dataset page in D2 and `SOURCES.md`; the
  two Piper voices were chosen because they were trained from scratch on public-domain
  data. Kokoro's card says its data was permissive or non-copyrighted and lists synthetic
  audio from closed commercial voices among it, which its Apache-2.0 license does not
  restrict.
- [The test copies the gate's rules of state] → It uses the gate's constant, and D8 makes
  the implementation follow `gate.rs`. `docs/ARCHITECTURE.md` notes that a change of the
  rules must change the test.
- [The peak rule is not Discord's own voice activity detection] → It matters only for
  recordings with a noise floor between pauses. The report shows each utterance's span, so a
  pause the rule did not see is visible.
- [Linux is not tested by the agent] → This PC has no Linux, and the Linux libraries come
  from `change/cross-platform-headless`, which is not tested on Linux either. The Linux task
  stays open for the user.
- [A user's own recording committed by mistake becomes public] → `local/` folders are
  ignored, and `AGENTS.md` and the `expect.txt` comment say so.
- [Clean synthetic voices say little about real people] → The noise run, and the user's own
  WAVs, which are the point of the folder layout.
- [`VOICEBRO_VOSK_DIR` is shared with the old Vosk corpus test, which fails without
  `VOICEBRO_WAKE_PHRASES`] → Run the new test by name, as the script does. The old test stays
  untouched on Windows as the user asked.
- [The repository grows by about 4 MB] → Record again only when the cases change.
- [Recording needs about 550 MB of voice files (cori, ljspeech, Kokoro) on the PC] → They go
  into the assets folder the other probes use, never into the repository.

## Migration Plan

None. The old tools run on Windows as before. Rollback is reverting the change's commit.
