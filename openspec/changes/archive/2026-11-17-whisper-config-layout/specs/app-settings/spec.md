## MODIFIED Requirements

### Requirement: Whisper phrase settings
The `whisper` section SHALL also hold `phrases.activation` and `phrases.deactivation`, each
a list of phrases, kept apart from the wake models' phrases. A list never set SHALL mean
the activation phrase "bro" or the deactivation phrase "peace out", whatever
`whisper.language` is. A value that is not a list of phrases SHALL be refused, naming the
field. The two phrase lists SHALL not share a phrase in the matching form. A phrase with no
letter or digit SHALL be refused, naming it.

#### Scenario: Shared phrase
- **WHEN** a save puts "Goodbye!" in the activation list and "goodbye" in the deactivation list
- **THEN** it is refused with a message naming `whisper.phrases.deactivation` and the phrase

#### Scenario: Defaults whatever the language
- **WHEN** the settings file has no `whisper.phrases` and `whisper.language` is `ru`
- **THEN** the Whisper phrases in effect are the activation "bro" and the deactivation "peace out"
