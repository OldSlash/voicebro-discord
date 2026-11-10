# wake-word-detection Specification

## Purpose
Spots the user's activation and deactivation phrases in each speaker's speech, offline, with
rules that hold up in noisy, continuous game chat.

## Requirements

### Requirement: Phrases come from the settings
The engine SHALL spot the phrases in `wake.activation_phrases` and
`wake.deactivation_phrases`, lower-cased and trimmed, as whole words in the recognizer's
open-vocabulary output. With both lists empty the engine SHALL stay idle and the status
SHALL say that no phrases are configured.

#### Scenario: No phrases configured
- **WHEN** wake-word mode is on and both phrase lists are empty
- **THEN** the status shows "no phrases configured", nothing is loaded for recognition, and no session can start

#### Scenario: Look-alike word
- **WHEN** a speaker says a longer word that contains an activation phrase as a prefix
- **THEN** the phrase does not count

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
When a speaker's audio stops for 60 ms, the engine SHALL feed up to 1.5 s of silence on a
20 ms tick so a phrase at the end of a sentence is finalized.

#### Scenario: Phrase at the end of a sentence
- **WHEN** a speaker ends a sentence with the deactivation phrase and stops talking
- **THEN** the phrase counts within about 0.5 s without the speaker saying anything else

### Requirement: Resource behavior
A recognizer SHALL exist only while its speaker is talking and be dropped after 30 s idle;
the engine and model SHALL be loaded only while the session mode is wake word; the audio
queue to the worker SHALL be bounded and drops SHALL be counted and logged.

#### Scenario: Mode switched to always
- **WHEN** the user switches the session mode from wake word to always
- **THEN** the model is unloaded and the engine's memory is released

### Requirement: Diagnostics
The engine SHALL log every final recognition result, every phrase decision and dropped audio,
and the status SHALL show the engine state, the model, the words of the configured phrases
the model does not know, and the drop count.

#### Scenario: Unknown word in a phrase
- **WHEN** a configured phrase contains a word the model does not know
- **THEN** the status lists that word as unknown
