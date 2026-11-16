# wake-word-detection Specification

## Purpose
Spots the activation and deactivation phrases in each speaker's speech, offline, with rules
that hold up in noisy, continuous game chat. The phrases are each model's default phrases
or the user's own, which replace them list by list.

## Requirements

### Requirement: Phrases come from the settings
The engine SHALL spot the selected model's activation and deactivation phrases: each list
as stored in `wake.phrases`, or the model's default phrases for a list never set. A phrase
is one or more words. Phrases of other models SHALL be ignored. With both lists set to
empty and the files on disk, the engine SHALL stay idle and say no phrases are configured.

#### Scenario: No phrases configured
- **WHEN** wake-word mode is on, the selected model and the Vosk library are on disk, and the user has set both phrase lists of the selected model to empty
- **THEN** the status shows "no phrases configured", nothing is loaded for recognition, and no session can start

#### Scenario: No phrases and no files
- **WHEN** wake-word mode is on, both phrase lists of the selected model are set to empty, and the model is not on disk
- **THEN** the status says a download is needed, not "no phrases configured", and no session can start

#### Scenario: Phrases of another model
- **WHEN** the user has set phrases for the Russian model only and selects the English model, which is on disk
- **THEN** the English model's default phrases are spotted and the Russian phrases are not

### Requirement: Phrases match as whole words in one form
Phrases and the recognizer's output SHALL be compared in one matching form (lower-case,
"ё" as "е"); a phrase counts when its words appear in a row as whole words.

#### Scenario: Look-alike word
- **WHEN** a speaker says a longer word that contains an activation phrase as a prefix
- **THEN** the phrase does not count

#### Scenario: English activation
- **WHEN** the English model is selected with the activation phrase "bro" and a speaker says "hey bro, what time is it"
- **THEN** the activation is reported once and the question's words are still recognized

#### Scenario: A phrase of several words
- **WHEN** the Russian model is selected with the deactivation phrases "отбой братан", "заканчивай" and "братан всё", and a speaker says "ну всё, отбой братан, спасибо"
- **THEN** the deactivation is reported once

#### Scenario: Only part of a phrase
- **WHEN** the deactivation phrase is "отбой братан" and a speaker says only "отбой"
- **THEN** that phrase does not count

#### Scenario: "ё" typed, "е" recognized
- **WHEN** the deactivation phrase is typed as "Братан всё" and the recognizer writes "братан все"
- **THEN** the phrase counts, and the status lists no word of it as unknown

### Requirement: One recognizer per speaker, one detector per phrase kind
The engine SHALL run a separate recognizer for each speaking source and separate detectors
for activation and deactivation, so a wake word earlier in a sentence cannot hide a
deactivation later in it.

#### Scenario: Wake word then deactivation in one sentence
- **WHEN** a speaker says an activation phrase followed by a deactivation phrase in one utterance during a session
- **THEN** the deactivation counts

### Requirement: Confirmation thresholds
A phrase SHALL count when it appears in a final result, or in 15 consecutive partial results
for activation, or in 5 of the last 10 partial results for deactivation. Each occurrence
SHALL be reported once per utterance, and the recognizer SHALL NOT be reset after a hit.

#### Scenario: Words after the phrase survive
- **WHEN** a speaker says an activation phrase and a question in one breath
- **THEN** the phrase is reported once and the question's words are still recognized

### Requirement: Deactivation guards
An activation phrase that follows a deactivation phrase in the same utterance SHALL be
ignored, and activations SHALL be ignored for 1.5 s after any deactivation.

#### Scenario: Deactivation followed by a wake word
- **WHEN** a speaker says the deactivation phrase and then an activation phrase in one utterance
- **THEN** the session ends and does not restart

### Requirement: Trailing silence
When a speaker's audio has been missing from 15 receive ticks in a row (300 ms of the
channel's audio), the engine SHALL feed 20 ms of silence for every tick of the absence, up
to 1.5 s, so a phrase at the end of a sentence is finalized. The absence SHALL be counted
in receive ticks, not by when the engine processes the audio. Audio that returns sooner
SHALL continue the speaker's stream with nothing inserted.

#### Scenario: Phrase at the end of a sentence
- **WHEN** a speaker ends a sentence with the deactivation phrase and stops talking
- **THEN** the phrase counts within about 1 s without the speaker saying anything else

#### Scenario: A gap in the middle of a word
- **WHEN** a speaker's audio is missing from 6 ticks in the middle of the deactivation phrase (songbird refilling its playout buffer) and the delayed audio then arrives
- **THEN** the engine receives the speaker's frames back to back with no silence between them, and the phrase counts

#### Scenario: The engine runs behind
- **WHEN** four people talk at once and the engine processes their audio 300 ms after it arrived
- **THEN** no silence is fed into any of their streams while each of them keeps talking

### Requirement: Resource behavior
A recognizer SHALL exist only while its speaker is talking and be dropped after 30 s idle.
The engine and model SHALL be loaded only while the session mode is wake word and the
assistant mode is realtime; in Whisper mode the engine SHALL be off, and its state SHALL
say that Whisper spots the phrases. The audio queue to the worker SHALL be bounded, and
drops SHALL be counted and logged.

#### Scenario: Mode switched to always
- **WHEN** the user switches the session mode from wake word to always
- **THEN** the model is unloaded and the engine's memory is released

#### Scenario: Assistant mode switched to Whisper
- **WHEN** the user switches the assistant mode to Whisper while Vosk is loaded
- **THEN** Vosk is unloaded, the wake word state reads off with the reason "the Whisper mode spots the phrases", and no Vosk download is asked for

### Requirement: Diagnostics
The engine SHALL log every final recognition result and every phrase decision with its
delay from the phrase's end. It SHALL also log dropped audio, a deactivation phrase seen
in partial results but never confirmed, bridged gaps in a speaker's audio, and processing
more than 500 ms behind the audio; the last two at most once per 10 s. The status SHALL
show the engine state, the model, the words of the configured phrases the model does not
know, and the drop count.

#### Scenario: Unknown word in a phrase
- **WHEN** a configured phrase contains a word the model does not know
- **THEN** the status lists that word as unknown

#### Scenario: A deactivation phrase seen but not confirmed
- **WHEN** a deactivation phrase appears in three partial results of an utterance and the utterance ends without confirming it
- **THEN** one log line names the speaker, the phrase and the three partial results, without the recognized text, and is written to the file with the verbose log off

#### Scenario: Gaps in a speaker's audio
- **WHEN** a speaker's audio is missing for 120 ms and for 60 ms within 10 s while they talk, and returns both times
- **THEN** one log line names the speaker, two bridged gaps and the longest of 120 ms

#### Scenario: The engine falls behind
- **WHEN** the engine processes audio 700 ms after it arrived
- **THEN** a log line gives that lag, and no further lag line follows within 10 s

#### Scenario: Delay of a phrase decision
- **WHEN** the engine confirms a deactivation phrase
- **THEN** the decision's log line gives how long after the phrase's end it reached the gate

### Requirement: Default phrases of each model
Each catalog model SHALL come with default phrases: for activation the language's slang for
"bro", for deactivation a slang farewell rare in game talk. Every default word SHALL be in
its model's vocabulary. A local model SHALL take the defaults of its folder's catalog
language.

#### Scenario: The defaults of the catalog
- **WHEN** the window lists the catalog models
- **THEN** their defaults (activation | deactivation) are: English "bro" | "peace out"; Russian "братан" | "отбой"; Czech "brácha" | "čau"; Dutch "maat" | "doei doei"; French "mon frère" | "à la prochaine", "a la prochaine"; German "bruder" | "bis dann"; Italian "fratello" | "arrivederci"; Polish "ziomek" | "do widzenia"; Portuguese (Brazil) "irmão" | "até mais"; Spanish "hermano" | "hasta luego"

#### Scenario: A new user
- **WHEN** the app starts with no settings file and wake-word mode on, and the English model is on disk
- **THEN** a speaker saying "bro" starts a session and "peace out" ends it, with nothing typed in the settings

#### Scenario: A list set by the user
- **WHEN** the user set the Russian deactivation list to "отбой братан, заканчивай" and never set its activation list
- **THEN** "братан" activates, the deactivation phrases are the user's two, and "отбой" alone does not end the session

#### Scenario: A list cleared by the user
- **WHEN** the user set the English deactivation list to empty
- **THEN** no deactivation phrase is spotted for the English model and the default "peace out" does not come back

#### Scenario: A local model of a catalog language
- **WHEN** the selected model is `local:vosk-model-small-de-0.21` and its phrases were never set
- **THEN** the German defaults are used, and any word of them the local model does not know is listed as unknown

#### Scenario: A local model of another language
- **WHEN** the selected model is a local model whose folder name gives no catalog language and its phrases were never set
- **THEN** the status shows "no phrases configured"

#### Scenario: Every default is known
- **WHEN** the catalog check loads all ten models through the real library
- **THEN** every word of every default phrase is known to its model
