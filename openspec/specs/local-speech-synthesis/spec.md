# local-speech-synthesis Specification

## Purpose
Speaks the assistant's answers in the Whisper mode with a voice on this PC: a catalog of
downloadable voices (Supertonic 3, Piper, Kokoro) and the voices Windows has, read
sentence by sentence as the answer streams in and stopped at once when the session ends.

## Requirements

### Requirement: A local voice engine from a pinned build
The app SHALL synthesize speech on the CPU with sherpa-onnx v2.0.1, loaded at run time from
the assets folder. The CPU archive SHALL be pinned by SHA-256 (7 899 712 bytes), and only
`sherpa-onnx-c-api.dll` and `onnxruntime.dll` SHALL be kept from it. The app SHALL load its
own `onnxruntime.dll` by full path before the sherpa-onnx library, so that a copy in the
Windows system folder is never used. It SHALL never ship any of these files.

#### Scenario: A system onnxruntime exists
- **WHEN** Windows has its own `onnxruntime.dll` in `System32`
- **THEN** the voice engine uses the downloaded `onnxruntime.dll`, and the log names its path

### Requirement: Voice catalog
The app SHALL offer the voices of the design (D9b):

- Supertonic 3, ten voices in 31 languages;
- the listed Piper voices for English, Russian, Czech, Dutch, French, German, Italian,
  Polish, Portuguese and Spanish;
- Kokoro English, while its check in the design passes;
- the voices Windows has, listed at run time.

#### Scenario: A Windows speech pack added
- **WHEN** the user adds a Windows speech pack and restarts the app
- **THEN** the new Windows voices are in the list

### Requirement: Catalog voices are pinned with their data license
Each downloaded voice SHALL be pinned by SHA-256, with its size, language, engine and the
license of its data. Voices whose data is licensed for non-commercial use only SHALL not be
offered.

#### Scenario: Voice list
- **WHEN** the window asks for the voices
- **THEN** each catalog voice comes with its engine, language, size, data license and whether it is downloaded, and each Windows voice with its language and "built into Windows"

### Requirement: The default voice
The default voice SHALL be `supertonic3:F1` until the design's listening test changes it.

#### Scenario: No voice chosen yet
- **WHEN** the app starts with no settings file
- **THEN** the voice is `supertonic3:F1`

### Requirement: Piper voices need a separate phonemizer
Piper and Kokoro voices SHALL get their phonemes from piper-phonemize v1.5.9, which SHALL
be a separate download pinned by SHA-256. The download notice and the dialog SHALL list it
as its own item, with its license GPL-3.0 and a link to its source. Supertonic 3 and the
Windows voices SHALL work without it.

#### Scenario: First Piper voice
- **WHEN** the user picks the first Piper voice
- **THEN** the dialog lists the voice and piper-phonemize (GPL-3.0) as two items, with sizes

#### Scenario: Supertonic only
- **WHEN** the user picks a Supertonic 3 voice
- **THEN** piper-phonemize is neither listed nor downloaded

### Requirement: Speakable text
Before synthesis the app SHALL remove from an answer what a voice should not read:

- code blocks;
- tables;
- link targets, keeping the link text;
- bare URLs, keeping the host name;
- heading, list, emphasis and quote markers;
- emoji.

The text SHALL be cut into sentences at `.`, `!`, `?`, `…` and line breaks, with pieces of
20 to 300 characters. The first piece of an answer SHALL end at the first comma after 20
characters.

#### Scenario: Answer with code
- **WHEN** an answer holds a sentence, a code block and a link `[the docs](https://example.org/a)`
- **THEN** the voice reads the sentence and "the docs", and nothing of the code or the URL

#### Scenario: First words sooner
- **WHEN** an answer starts "Sure, in Bangkok it is 31 degrees and sunny today."
- **THEN** the first synthesized piece ends after the first comma that follows 20 characters, and the rest is synthesized while that piece plays

### Requirement: The voice is played in time
Synthesized audio SHALL be resampled to 48 kHz stereo and played through the guild's output
at one frame per 20 ms. The output gain SHALL apply to it, and the earcons SHALL keep their
own volume. Sending toward Discord SHALL start before voiced audio plays. Synthesis SHALL run
on a thread of its own, at most 20 s ahead of what has played.

#### Scenario: Long answer
- **WHEN** an answer takes two minutes to read
- **THEN** synthesis never runs more than 20 s ahead, and memory stays bounded

### Requirement: The voice stops at once
A flush (deactivation, barge-in, mute, mode change) SHALL empty what is waiting at once,
skip the queued sentences, and stop or discard a synthesis that is running.

#### Scenario: Deactivation during synthesis
- **WHEN** the session ends while a sentence is being synthesized
- **THEN** no audio of that answer plays afterwards

### Requirement: The voice loads at a join and unloads after the last leave
In Whisper mode the voice engine and the chosen voice SHALL load while the bot joins, and
one short synthesis SHALL warm them up. They SHALL unload 0.5 s after the last leave, and
when the mode goes back to realtime. A voice change while loaded SHALL load the new voice
in the background. A speed change SHALL apply from the next sentence.

#### Scenario: Voice changed between sentences
- **WHEN** the user picks another voice while an answer is being read
- **THEN** the sentences already queued finish in the old voice, and the next answer uses the new one

### Requirement: Preview a voice in the window
Preview SHALL synthesize the app's own short, neutral sample sentence in the voice's
language and play it in the window only, never in Discord. Preview with a voice that is not
downloaded SHALL first ask for the download, as the choice does. A voice engine that was not
loaded for Preview SHALL unload one minute later unless a join needs it.

#### Scenario: Preview while not in a channel
- **WHEN** the bot is in no voice channel and the user presses Preview
- **THEN** the sample plays in the window within a few seconds, and the engine is unloaded about a minute later

### Requirement: Voice diagnostics
The status SHALL show the voice's state, the voice, the engine, its sample rate and the last
time to the first audio. A voice that fails to load while the bot is in a channel SHALL be
an error state, with the reason in the status and the log. The bot SHALL stay in the
channel, and the answers SHALL still be posted when posting is on. A failure at the join
itself refuses the join, as the requirement "Join prepares the assistant" says.

#### Scenario: Voice fails to load during a session
- **WHEN** the user picks a voice whose files turn out damaged while the bot is in a channel
- **THEN** the status shows the voice error with the file named, and the log says the answers will not be spoken until another voice is chosen
