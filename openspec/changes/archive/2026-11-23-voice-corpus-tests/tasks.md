# Tasks

The branch is `change/voice-corpus-tests`, from `main` after `change/cross-platform-headless`
is merged (the Linux Vosk and Whisper libraries come from it).

## 1. The old corpus tests only on Windows

- [x] 1.1 Mark `vosk_corpus_when_assets_are_provided` and `vosk_gaps_when_assets_are_provided` ignored off Windows with the reason, and run the corpus part of `whisper_runtime_when_enabled` only on Windows (D10). Verify on Windows that `cargo test -p voicebro-core vosk_` and `cargo test -p voicebro-core whisper_runtime` behave as before: skipped without their variables, and one old corpus file and the Whisper runtime check passing with them
  (2026-11-23, release: without variables `vosk_` 5 passed and `whisper_runtime` passed at once; `david-en-bro-alone.wav` with "bro | peace out" heard "bro" and passed; `whisper_runtime_when_enabled` with `target\whisper-test\assets` passed with its two corpus files)
- [x] 1.2 Add the platform check to the top of `scripts\make-wake-corpus.ps1` and `scripts\run-wake-corpus.ps1` (D10). Verify on Windows that `run-wake-corpus.ps1 -Languages en` prints the same per-file results as before the change, and that the check's expression is true in Windows PowerShell 5.1
  (2026-11-23: the script of `HEAD` and the changed one printed the same 40 result lines, 34 of 36 right as before; in Windows PowerShell 5.1 the platform is `Win32NT`, so the check lets the script run, exit 0)
- [x] 1.3 Say in `docs/ARCHITECTURE.md` (testing) and `AGENTS.md` (the corpus commands) that the old corpus tests and scripts run only on Windows. Verify that both name the three tests and the two scripts

## 2. The recorded English set

- [x] 2.1 Add `probe voice-corpus-make` (D2) and its line in `probe list`. Verify: it writes 30 files (David, Mark and Zira on this PC) as 16 kHz mono 16-bit WAVs into `crates/voicebro-core/testdata/voice-corpus/en/windows/`, and `expect.txt` lists every file with the expectations and words of the design's table. A file put into `voice-corpus/en/mine/` before a second run is unchanged after it. The probe stops with a message when no English voice is listed (checked by filtering for a language with no voice)
- [x] 2.2 Add `crates/voicebro-core/testdata/voice-corpus/**/local/` to `.gitignore`. Verify with `git check-ignore -v` that a WAV in `en/local/` and in `en/mine/local/` is ignored and one in `en/windows/` is not
- [x] 2.3 Check Microsoft's terms for sharing Windows speech output in a public repository, read from the license text itself. Record the source and the finding in design.md under Risks. Verify that the entry is there. If the terms do not clearly allow it, ask the user before any commit of the WAVs
- [x] 2.4 Document the probe and the corpus folder in `AGENTS.md` (commands) and `docs/ARCHITECTURE.md` (testing): the cases, the `local/` folders, and that the files are public once committed. Verify that the documented probe command runs as written
- [x] 2.5 Record the set with voices whose output may be published (D2 as revised on 2026-11-23, after task 2.3 found that the Windows terms do not allow publishing): Piper ljspeech and cori, Kokoro v0.19 Bella, Adam, Emma and George, resampled to 16 kHz, into `voice-corpus/en/tts/` with `expect.txt` and `SOURCES.md`; move the Windows recordings out of the repository. Verify: 60 files as 16 kHz mono 16-bit WAVs, `SOURCES.md` names the six voices with the licenses read from their model cards, no file of a Windows voice remains under `voice-corpus/`, a file in `en/mine/` is unchanged by a second run, and Whisper reads the words of a sample of the files back
  (2026-11-23: 60 WAVs, 3.9 MB, and `SOURCES.md` with the six voices; the Windows recordings moved to `target\voice-corpus-windows-old\`; a file in `en/mine/` kept its hash through a second run; Whisper wrote 58 of the 60 clean files' sentences as spoken, and could not make out "Bro." alone from ljspeech and Emma)
- [x] 2.6 Run the corpus test on the new set through both engines and write the counts into the report; update `AGENTS.md` and `docs/ARCHITECTURE.md` (the voices, the folder `en/tts/`, `SOURCES.md`). Verify that the documented probe command runs as written
  (2026-11-23, Vosk `vosk-model-small-en-us-0.15`, Whisper `whisper-base-q8_0` on the CPU: Vosk clean 43 of 60 right with 5 false activations, noise 50 of 60 with 4; Whisper clean 58 of 60, noise 49 of 60, no false activation, median 256 and 264 ms. Findings, expectations unchanged: Vosk writes "bros" as "bro's", which the matching form splits into "bro", so the look-alike sentence activates for five voices; Vosk misses "bro" at a sentence's start for cori, ljspeech, Bella and Adam; under noise Whisper keeps "bro … peace out" as one utterance for every voice, as with the Windows set)

## 3. The corpus test

- [x] 3.1 Add the test module `voice_corpus` with its WAV reader (D4). Verify with unit tests that run everywhere without assets: 16-bit mono at 16 kHz, 24-bit stereo at 44.1 kHz, 32-bit float, `WAVE_FORMAT_EXTENSIBLE` with PCM and float, an 8-bit file refused with its format, a file without a data chunk refused. Run `cargo test -p voicebro-core voice_corpus::`
- [x] 3.2 Read the corpus tree and `expect.txt` (D1, D3). Verify with unit tests on a corpus written to a temporary folder:
  - file lines with one change, two changes and the words;
  - `phrases` set in a folder and taken by a subfolder that sets none;
  - the defaults without a `phrases` line;
  - comments, and a bad line reported with its number;
  - an unlabeled WAV, and a line naming a missing file;
  - a `de` folder reported as not covered;
  - a `local/` folder's files included
- [x] 3.3 Play a file as Discord delivers a speaker (D5). Verify with unit tests:
  - audible frames are sent, with the 5 frames after;
  - a 1.5 s pause sends nothing while its ticks pass;
  - 100 ticks follow the file;
  - the noise run sends every frame and adds 3 s;
  - a partial last frame is padded
- [x] 3.4 Read in `gate.rs` how the gate applies Vosk's phrases and Whisper's decisions, the reactivation guard included, and apply the same rules in the test (D8). Verify with unit tests:
  - an activation while active is ignored;
  - a deactivation while waiting is ignored;
  - an activation 1.0 s after a deactivation is ignored, and one 1.6 s after it counts;
  - a list that begins with `deactivate` starts active.

  Note in `docs/ARCHITECTURE.md` that a change of the gate's rules must change the test
- [x] 3.5 Run each file through Vosk on the worker's path with its report lines (D6, D11). Verify by running `voice_corpus_when_enabled` with only `VOICEBRO_VOSK_DIR` (`target\wake-corpus\assets`) on the English set. Each file has a clean and a noise line with the phrase, its moment and delay, and what Vosk heard. Phrases with a word the model does not know (`phrases = bro | zzqxvkw` in a scratch copy) give an error naming the word. Write the right-of-labeled counts into the final report; a wrong file is a finding to report, never a reason to change its expectation
  (2026-11-23, `vosk-model-small-en-us-0.15`: clean 23 of 30 right, noise 28 of 30, no false activation. Wrong: `david-peace-alone`, `mark-peace-alone`, `zira-peace-alone` clean (heard "he's out" and the like), `mark-bro-alone` and `zira-bro-alone` clean (heard "though"), `zira-bro-start` and `zira-bro-then-peace` clean and noise. The scratch copy's `zzqxvkw` gave "the model does not know «zzqxvkw»". Expectations unchanged)
- [x] 3.6 Run each file through Whisper on the cutter's path with its report lines (D7, D11). Verify with only `VOICEBRO_WHISPER_DIR` (`target\whisper-test\assets`, `whisper-base-q8_0` on the CPU):
  - each utterance's span, text, decision, question, time and delay are shown;
  - `bro-then-peace` gives two utterances that activate and then deactivate;
  - the assets folder's listing is the same before and after the run;
  - `VOICEBRO_WHISPER_MODEL` naming a catalog model that is not installed fails the test with its file named.

  Write the counts and the median transcription time into the final report
  (2026-11-23, `whisper-base-q8_0` on the CPU: clean 29 of 30 right, median 253–262 ms per utterance; noise 27 of 30, median 261–272 ms; no false activation. Wrong: `david-bro-alone` clean (no speech found), and `bro-then-peace` of all three voices under noise: the steady noise keeps the cutter's utterance open, so the activation and the farewell are one utterance and only the activation is decided. The assets listing was the same before and after; a model that is not installed failed the test naming `ggml-base-q8_0.bin`)
- [x] 3.7 Add the summary and the single verdict at the end (spec "The summary and the verdict"). Verify on a scratch copy of the corpus in `target\` through `VOICEBRO_VOICE_CORPUS`. One file is labeled wrong on purpose, one has no label and one is 8-bit. Every file is still reported, the summary counts each kind, and the test fails at the end naming the wrong file and the error. With neither assets variable set, the test passes at once
- [x] 3.8 Document the test in `docs/ARCHITECTURE.md` (the module, the playback rule, the two runs, the report) and in `AGENTS.md` (the command, the variables, `expect.txt` in short, the `local/` folders). Verify that the documented `cargo test` line runs as written

## 4. The script

- [x] 4.1 Add `scripts\run-voice-corpus.ps1` (D9) and its line in `AGENTS.md`. Verify:
  - it runs both engines with this PC's default folders;
  - the report is printed and saved as `target\voice-corpus\runs\<date-time>.txt`;
  - its exit code is the test's: 0 on the English set if every file is right, and not 0 on the scratch copy of 3.7

  (2026-11-23: on the English set both engines ran and the report was saved; the set has 13 wrong runs (3.5, 3.6), so the exit code was 101. A copy of its 20 files that every run gets right (`-Corpus target\voice-corpus-allright`) exited with 0, the scratch copy of 3.7 with 101)

## 5. Checks

- [x] 5.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH) and `openspec validate voice-corpus-tests --strict` through `.tools\openspec`. Say in the report what each returned
  (2026-11-23: fmt clean after `cargo fmt`; clippy no errors, six warnings of Rust 1.99's lints in files this change does not touch; `cargo test` 549 + 17 + 2 passed, 0 failed; validate: valid)
- [x] 5.2 The user's check on glibc Linux x86-64, since this PC has no Linux:
  - `cargo test -p voicebro-core` lists the two old Vosk corpus tests as ignored with the reason;
  - `voice_corpus_when_enabled` runs the English set with the Linux Vosk assets, and with the Linux Whisper CPU runtime when given;
  - the old corpus scripts in PowerShell 7 print that they are Windows-only and exit with 0

  Open when the change was archived on 2026-11-23: the user asked to merge and archive the
  changes and release 1.3.1, and checks Linux later.

  Closed by the user later on 2026-11-23 without being run, as not tested: for now the Linux
  support only prepares for Linux. The corpus has not run on Linux.
- [x] 5.3 Report what was built, the corpus results per engine and run, and what was not tested: Linux, real people's voices until the user adds their WAVs, and Discord's own voice activity detection

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs, the recorded set
  (after task 2.3) and this change archived; then `git merge --no-ff --log
  change/voice-corpus-tests` into `main`.
