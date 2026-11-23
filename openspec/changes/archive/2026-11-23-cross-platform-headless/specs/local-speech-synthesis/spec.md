# Spec Delta

## MODIFIED Requirements

### Requirement: A local voice engine from a pinned build
The app SHALL synthesize speech on the CPU with sherpa-onnx v2.0.1, loaded at run time from the assets folder. On Windows the CPU archive SHALL be pinned by SHA-256 (7 899 712 bytes), and only `sherpa-onnx-c-api.dll` and `onnxruntime.dll` SHALL be kept from it. On Linux x86-64 (not tested) the archive SHALL be `sherpa-onnx-v2.0.1-linux-x64-shared-lib.tar.bz2`, pinned by SHA-256 (9 710 131 bytes), keeping only `libsherpa-onnx-c-api.so` and `libonnxruntime.so` with the file names the probe of the archive found. The app SHALL load its own onnxruntime library by full path before the sherpa-onnx library, so that a copy in a system folder is never used. It SHALL never ship any of these files.

#### Scenario: A system onnxruntime exists
- **WHEN** Windows has its own `onnxruntime.dll` in `System32`
- **THEN** the voice engine uses the downloaded `onnxruntime.dll`, and the log names its path

#### Scenario: Linux voice engine (not tested)
- **WHEN** the Linux (not tested) voice engine archive has been installed and a Supertonic 3 voice is selected in the Whisper mode
- **THEN** the voice engine loads the downloaded `libonnxruntime.so` and `libsherpa-onnx-c-api.so` by full path and speaks the answers

### Requirement: Voice catalog
The app SHALL offer the voices of the design (D9b):

- Supertonic 3, ten voices in 31 languages;
- the listed Piper voices for English, Russian, Czech, Dutch, French, German, Italian,
  Polish, Portuguese and Spanish;
- Kokoro English, while its check in the design passes;
- on Windows only, the voices Windows has, listed at run time.

On Linux (not tested) the list SHALL hold no Windows voices.

#### Scenario: A Windows speech pack added
- **WHEN** the user adds a Windows speech pack and restarts the app
- **THEN** the new Windows voices are in the list

#### Scenario: Linux voice list (not tested)
- **WHEN** the headless app runs on Linux (not tested)
- **THEN** the voice list holds the Supertonic 3, Piper and Kokoro voices and no Windows voice

### Requirement: Piper voices need a separate phonemizer
Piper and Kokoro voices SHALL get their phonemes from piper-phonemize v1.5.9, which SHALL be a separate download pinned by SHA-256: on Windows the existing Windows archive, on Linux x86-64 (not tested) `piper-phonemize-v1.5.9-linux-x64-shared-lib.tar.bz2`. The download notice, the dialog and the headless install command SHALL list it as its own item, with its license GPL-3.0 and a link to its source. Supertonic 3 and the Windows voices SHALL work without it.

#### Scenario: First Piper voice
- **WHEN** the user picks the first Piper voice
- **THEN** the dialog lists the voice and piper-phonemize (GPL-3.0) as two items, with sizes

#### Scenario: Supertonic only
- **WHEN** the user picks a Supertonic 3 voice
- **THEN** piper-phonemize is neither listed nor downloaded

#### Scenario: Headless install of a Piper voice
- **WHEN** the operator runs `voicebro-server --install-assets` in the Whisper mode with a Piper voice selected and piper-phonemize missing
- **THEN** the command lists the voice and piper-phonemize (GPL-3.0, with its source link) as two items and installs both
