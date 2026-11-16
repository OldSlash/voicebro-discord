# Spec Delta

## ADDED Requirements

### Requirement: Assistant mode setting
`assistant.mode` SHALL be `realtime` or `whisper`, default `realtime`. Any other value
SHALL be refused, naming the field and the two values. A change SHALL apply at once, as
the requirement "The mode switches at once" of `whisper-assistant-mode` says.

#### Scenario: Older file
- **WHEN** a settings file from 1.0.1 is loaded
- **THEN** the assistant mode is realtime, and the next save writes the field

#### Scenario: Unknown value
- **WHEN** a save sets `assistant.mode` to "local"
- **THEN** it is refused with a message naming `assistant.mode`, `realtime` and `whisper`, and the stored settings stay

### Requirement: Whisper settings
The `whisper` section SHALL hold:

| Field | Values | Default |
|---|---|---|
| `model` | a catalog id, or `local:<file name>` of a plain file name in `whisper-models\` | `whisper-base-q8_0` |
| `device` | `cpu` or `gpu` | `cpu` |
| `threads` | 0 (automatic) to 64 | 0 |
| `language` | `auto` or an ISO 639-1 code that Whisper knows | `auto` |

A value outside these SHALL be refused, naming the field.

#### Scenario: A path as a local model
- **WHEN** a save sets `whisper.model` to `local:..\other.bin`
- **THEN** it is refused, saying a local model is a plain file name of the Whisper models folder

### Requirement: Whisper phrase settings
The `whisper` section SHALL also hold `phrases.activation` and `phrases.deactivation`, each
a list of phrases. Each SHALL default to empty, or to the defaults of design D4 once
`voice-recognition-rework` has landed. A value that is not a list of phrases SHALL be
refused, naming the field. The two phrase lists SHALL not share a phrase in the matching
form. A phrase with no letter or digit SHALL be refused, naming it.

#### Scenario: Shared phrase
- **WHEN** a save puts "Goodbye!" in the activation list and "goodbye" in the deactivation list
- **THEN** it is refused with a message naming `whisper.phrases.deactivation` and the phrase

### Requirement: Voice settings
The `tts` section SHALL hold `voice` (a voice catalog id, or `windows:<voice id>` for a
voice Windows has) and `speed` (0.5 to 2.0, default 1.0). The default voice SHALL be the
one the design names. A voice that is neither in the catalog nor of the form
`windows:<id>` SHALL be refused, naming the field.

#### Scenario: Speed out of range
- **WHEN** a save sets `tts.speed` to 3
- **THEN** it is refused with a message naming `tts.speed` and the range 0.5 to 2.0

### Requirement: Whisper and voice changes apply without a restart
A saved change of the `whisper` or `tts` section SHALL apply without a restart:

- the phrases, the language and the CPU threads from the next utterance;
- the model, device and voice by a background reload, while the bot is in a channel in
  Whisper mode, and otherwise at the next join;
- the speed from the next sentence.

Such a change SHALL rebuild no gate and no realtime call.

#### Scenario: Phrase added during a session
- **WHEN** the user adds a deactivation phrase while a session is active in Whisper mode
- **THEN** the next utterance holding it ends the session, and nothing was reloaded
