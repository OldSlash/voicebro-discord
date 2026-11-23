# Spec Delta

## Purpose

A corpus of recorded speech in the repository, and a test that runs it through the app's
Vosk wake word and Whisper phrase spotting as the app hears a Discord speaker, on every
platform, and reports whether and how each file activated the assistant.

## ADDED Requirements

### Requirement: The corpus lives in the repository
The voice corpus SHALL live in `crates/voicebro-core/testdata/voice-corpus/`, in one folder
per language named by its two-letter code, with WAV files at any depth below it, so that
every checkout on every platform has it. Files under a folder named `local` SHALL be run like
the others and SHALL never be tracked by git.

#### Scenario: A new WAV is picked up
- **WHEN** the user puts a WAV anywhere under `voice-corpus/en/` and runs the test
- **THEN** the file is run and reported, with no code, script or list edited

#### Scenario: A local file stays local
- **WHEN** a WAV is under `voice-corpus/en/local/`
- **THEN** the test runs it
- **AND** `git status` does not show it

### Requirement: Only English is covered
The test SHALL run the `en` folder. A folder of any other language SHALL be named in the
report as not covered, and none of its files SHALL be run.

#### Scenario: Another language's folder
- **WHEN** a `de` folder holds WAV files
- **THEN** the report says that `de` is not covered, runs none of its files, and the test does not fail because of it

### Requirement: Each folder says what its files must do
An `expect.txt` SHALL list files of its own folder, one per line, with the state changes the
app must make, in order: `activate`, `deactivate`, both separated by a comma, or `none`. The
line MAY end with `;` and the words spoken. Lines starting with `#` SHALL be comments.

#### Scenario: A labeled file
- **WHEN** `expect.txt` holds `bro-start.wav = activate ; Bro, what time is it in London?`
- **THEN** `bro-start.wav` passes only when exactly one activation happens, and the report shows the spoken words next to what each engine heard

#### Scenario: An activation and a deactivation in one file
- **WHEN** a file's line says `activate, deactivate`
- **THEN** it passes only when an activation and then a deactivation happen, and nothing else

### Requirement: A folder can set its own phrases
A `phrases = <activation> | <deactivation>` line in `expect.txt` SHALL set the phrases for
its folder and the folders below it that set none, each list separated by commas as in the
settings. Without such a line the language's default phrases SHALL apply.

#### Scenario: The user's own phrases
- **WHEN** `expect.txt` says `phrases = hey bro | goodbye`
- **THEN** the files of that folder are checked against "hey bro" and "goodbye"

#### Scenario: The default phrases
- **WHEN** no folder above an English file sets phrases
- **THEN** it is checked against "bro" and "peace out"

### Requirement: The starting state follows the expectation
A file whose expected changes begin with `deactivate` SHALL be played with a session already
active. Every other file SHALL be played while waiting for the activation.

#### Scenario: A deactivation file
- **WHEN** `peace-alone.wav = deactivate`
- **THEN** it is played with a session active, and a confirmed "peace out" ends the session

#### Scenario: A file that must do nothing
- **WHEN** `plain.wav = none`
- **THEN** it is played while waiting, and any activation counts as a false activation

### Requirement: Unlabeled files and broken entries
A WAV without a line SHALL be run and reported as unlabeled, and SHALL not be judged. A line
naming a file that does not exist, and a labeled file the test cannot read, SHALL be
reported as errors with the reason.

#### Scenario: An unlabeled file
- **WHEN** a WAV has no line in its folder's `expect.txt`
- **THEN** the report shows how each engine handled it, marked unlabeled, and it neither passes nor fails

#### Scenario: A missing file
- **WHEN** `expect.txt` names `bro-twice.wav` and there is no such file
- **THEN** the report names the line as an error and the test fails

### Requirement: Audio formats
The test SHALL read WAV files with 16-bit or 24-bit integer or 32-bit float samples, mono or
stereo, at any sample rate. A file in another format SHALL be reported with its format and
not run.

#### Scenario: A recording from an audio editor
- **WHEN** a WAV is 44.1 kHz stereo with 24-bit samples
- **THEN** it is run like a 16 kHz mono file of the same speech

#### Scenario: An unreadable format
- **WHEN** a WAV holds 8-bit samples
- **THEN** the report names the file and its format and does not run it

### Requirement: Files are played as Discord delivers a speaker
Each file SHALL be played as 48 kHz stereo frames of 20 ms, one per receive tick. A frame
SHALL be sent while the audio is audible (a peak of 256 or more) and for 5 frames after;
nothing SHALL be sent in between, while the receive ticks go on. After the file, 2 s of
receive ticks SHALL pass with nothing sent.

#### Scenario: A pause in the file
- **WHEN** a file has 1.5 s of silence between two sentences
- **THEN** the engines get no frames during that pause, only receive ticks

### Requirement: Each file also runs under noise
Each file SHALL be run twice: as recorded, and with steady noise of peak 300 added under it
and continued for 3 s after it, so that every frame of that run is sent.

#### Scenario: Two runs per file
- **WHEN** the test runs a file
- **THEN** the report has a run as recorded and a run with noise for it

### Requirement: Vosk is checked as the realtime mode hears
When the Vosk assets are given, every file SHALL go through the language's catalog Vosk
model, handled as the app's wake worker handles a speaker's frames, receive ticks and
trailing silence. The gate's rules SHALL decide the state: activation only while waiting,
deactivation only while active, and no activation within 1.5 s of a deactivation, counted
in the file's time.

#### Scenario: An activation while active
- **WHEN** Vosk confirms an activation phrase while the session is active
- **THEN** the report shows it as ignored because the session is already active, and it is not a state change

#### Scenario: A phrase the model does not know
- **WHEN** a folder's phrases contain a word the Vosk model does not know
- **THEN** the report names the word, and that folder's labeled files are errors for Vosk

### Requirement: Whisper is checked as the Whisper mode hears
When the Whisper assets are given, each file's frames SHALL be cut into utterances as the
app cuts them. Each utterance SHALL be transcribed by the app's default Whisper model on the
CPU, unless another model or device is named, with the folder's language and the phrases as
the prompt. The app's noise guards SHALL apply, and the app's wake word decision SHALL turn
each text into a state change, with the rules of state that Vosk gets.

#### Scenario: An activation with a question
- **WHEN** a file says "Bro, what time is it in London?" while waiting
- **THEN** the run activates with the phrase "bro" and the question "what time is it in London?"

#### Scenario: Two utterances in one file
- **WHEN** a file says an activation sentence, pauses for 1.5 s and says "Thanks, peace out."
- **THEN** the first utterance activates and the second deactivates

#### Scenario: Too little speech
- **WHEN** an utterance has fewer voiced frames than the app transcribes
- **THEN** the report shows it as dropped noise, with nothing transcribed

### Requirement: The report shows each file's result
For every file, run and engine the report SHALL show the expected state changes, the state
changes that happened, and whether they match. When the spoken words are given, it SHALL
show them next to what each engine heard.

#### Scenario: A wrong file
- **WHEN** a file labeled `activate` gives no activation in the noise run of Whisper
- **THEN** that run's line says WRONG, with what was expected and what happened

### Requirement: The report shows how Vosk activated
For every Vosk run the report SHALL show each confirmed phrase with its kind, the moment of
the file at which it was confirmed and how long after the phrase's end, every ignored phrase
with the rule that ignored it, a deactivation phrase seen but never confirmed, and every
final text Vosk heard.

#### Scenario: A phrase at the end of a sentence
- **WHEN** Vosk confirms "bro" at the end of "That was a good game, bro."
- **THEN** the report gives the second of the file of the confirmation, the delay after "bro" ended, and the sentence as Vosk heard it

### Requirement: The report shows how Whisper activated
For every Whisper run the report SHALL show each utterance with its start and end in the
file, the text Whisper wrote or why it gave none, the decision with its phrase and the
question left after it, the transcription time, and how long after the end of the speech the
decision came.

#### Scenario: An utterance that activates
- **WHEN** Whisper writes "Hey bro, what is the weather in Bangkok?"
- **THEN** the report shows that text, the activation with "bro", the question, the transcription time and the delay after the speech ended

### Requirement: The summary and the verdict
After every file the report SHALL give, per engine and run, the labeled files that were
right out of all labeled files, the false activations, the unlabeled files and the errors.
The test SHALL fail when a labeled file is wrong, missing or unreadable, and only after the
whole report is printed.

#### Scenario: One wrong file among many
- **WHEN** one labeled file is wrong
- **THEN** every other file is still run and reported, the summary counts it, and the test fails at the end naming it

### Requirement: The test runs on every platform with the assets it is given
The test SHALL build and run on every platform the core builds on. It SHALL run Vosk when
`VOICEBRO_VOSK_DIR` names the Vosk assets and Whisper when `VOICEBRO_WHISPER_DIR` names
Whisper's, and do nothing with neither. It SHALL never download anything. A named assets
folder without the needed files SHALL fail the test with the files named.
`VOICEBRO_VOICE_CORPUS` SHALL point it at another corpus folder.

#### Scenario: Linux with the Vosk assets only
- **WHEN** the test runs on Linux with `VOICEBRO_VOSK_DIR` set to the Linux Vosk assets and no Whisper assets
- **THEN** the corpus runs through Vosk, and the report says Whisper was not run because its assets were not given

#### Scenario: No assets
- **WHEN** neither variable is set
- **THEN** the test passes without running anything, as the other gated tests do

### Requirement: The English set is recorded with voices whose output may be published
`probe voice-corpus-make` SHALL record the English set only with voices whose licenses allow
publishing their output: Piper `en_US-ljspeech-high` and `en_GB-cori-high` (public domain)
and the Kokoro v0.19 voices `af_bella`, `am_adam`, `bf_emma` and `bm_george` (Apache-2.0),
into `voice-corpus/en/tts/`, which it SHALL replace whole, touching nothing else.

#### Scenario: Recording the set
- **WHEN** the probe runs
- **THEN** every case of the set is recorded once per voice as a 16 kHz mono 16-bit WAV, `expect.txt` lists every file with its expected changes and the words spoken, and `SOURCES.md` says the files are machine generated and names the six voices with their licenses and model cards

#### Scenario: Voice files not on disk
- **WHEN** a voice's files are missing from the assets folder
- **THEN** they are downloaded with their pinned hashes before anything is recorded

#### Scenario: No voice whose output may not be published
- **WHEN** the set is recorded
- **THEN** no file is made with a Windows voice or another voice outside the list above

#### Scenario: The user's files are kept
- **WHEN** the user has files in `voice-corpus/en/mine/` and the probe runs again
- **THEN** those files are unchanged

### Requirement: The English set's cases
The English set SHALL hold, per voice: "bro" alone, at a sentence's start, after "hey" and at
a sentence's end, each to activate; a sentence without a phrase, one starting with
"Brother" and one of look-alike words, each to do nothing; "peace out" alone and at a
sentence's end, each to deactivate; and an activation sentence, a 1.5 s pause and a
deactivation sentence in one file, to activate and then deactivate.

#### Scenario: The set's expectations
- **WHEN** the set is recorded
- **THEN** its `expect.txt` gives each of these files the result named above

### Requirement: The old corpus tests run only on Windows
The tests of the old wake corpus SHALL run only on Windows. Elsewhere the Vosk corpus test
and the Vosk gaps test SHALL be reported as ignored, the Whisper runtime test SHALL skip its
transcriptions of corpus files, and the scripts that make and run the old corpus SHALL say
that they are Windows-only and stop without an error.

#### Scenario: cargo test on Linux
- **WHEN** `cargo test -p voicebro-core` runs on Linux
- **THEN** the two Vosk corpus tests are listed as ignored with the reason that the old corpus is made with the Windows voices

#### Scenario: The old scripts on Linux
- **WHEN** `make-wake-corpus.ps1` or `run-wake-corpus.ps1` runs in PowerShell on Linux
- **THEN** it prints that it runs only on Windows and exits with success

#### Scenario: On Windows nothing changes
- **WHEN** the old tests and scripts run on Windows with their variables
- **THEN** they run as before

### Requirement: Running the corpus on this PC
`scripts/run-voice-corpus.ps1` SHALL run the test in a release build with the asset folders
of this PC by default, and SHALL save each report, named with its date and time, in
`target/voice-corpus/runs/`.

#### Scenario: One command
- **WHEN** the user runs the script after adding WAV files
- **THEN** the report is printed and saved, and the script's exit code is the test's
