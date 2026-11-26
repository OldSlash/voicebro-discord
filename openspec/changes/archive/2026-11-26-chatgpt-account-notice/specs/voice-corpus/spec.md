## ADDED Requirements

### Requirement: The old corpus's other languages are spoken by catalog Piper voices
`scripts/make-wake-corpus.ps1 -Piper` SHALL make the old wake corpus's sets of Czech, Dutch, French, German, Italian, Polish, Portuguese and Spanish with one Piper voice of the app's catalog per language, each one whose model card and dataset allow publishing its output, as 48 kHz mono 16-bit WAVs in `target/wake-corpus/<language>/`, without any sign-in.

#### Scenario: Making the sets
- **WHEN** the user runs `make-wake-corpus.ps1 -PiperOnly` with the probe built
- **THEN** each of the eight languages gets its six WAVs (the activation alone, at a question's start and at a sentence's end, a plain sentence, the deactivation alone and at a sentence's end) spoken by that language's voice, and the script prints the voice and its license per language

#### Scenario: Voice files not on disk
- **WHEN** a language's voice files are missing from the assets folder
- **THEN** they are downloaded with their pinned hashes before that language's WAVs are made

#### Scenario: Some languages
- **WHEN** the user runs `make-wake-corpus.ps1 -PiperOnly -Languages de,fr`
- **THEN** only the German and French sets are made and the other folders stay as they are

#### Scenario: No account
- **WHEN** the sets are made on a PC where no engine is signed in
- **THEN** they are made the same way
