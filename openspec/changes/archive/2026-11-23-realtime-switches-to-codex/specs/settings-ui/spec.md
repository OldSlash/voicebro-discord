# Spec Delta

## MODIFIED Requirements

### Requirement: Engine choice in the Assistant card
The Assistant card's Engine list SHALL offer Codex, Claude Code and OpenCode, saved on
change. Its help line SHALL say that only Codex has the voice mode and what each engine
needs. While Claude Code or OpenCode is chosen, a note under the mode SHALL say that the
voice mode works only with Codex and that choosing it switches the engine to Codex.

#### Scenario: Claude Code chosen in the realtime mode
- **WHEN** the mode is Realtime voice and the user picks Claude Code
- **THEN** one save stores the engine Claude Code and the mode Whisper, the Recognition field shows "Whisper", "Realtime voice" stays selectable, and the note under the mode says that choosing it switches the engine to Codex

#### Scenario: Whisper files missing
- **WHEN** the mode is Realtime voice, the Whisper files are not downloaded and the user picks OpenCode
- **THEN** no download dialog opens, and the Assistant card shows the note that the Whisper files are not downloaded with its button to Settings

#### Scenario: Back to Codex
- **WHEN** the engine is Claude Code in the Whisper mode and the user picks Codex
- **THEN** the engine is saved as Codex, the mode stays Whisper, and the note under the mode disappears

## ADDED Requirements

### Requirement: Realtime voice switches the engine to Codex
The mode's "Realtime voice" choice SHALL never be disabled. Choosing it while Claude Code
or OpenCode is the engine SHALL save the realtime mode and the engine Codex in one save;
the window SHALL then show Codex in the Engine list and the Codex card. The settings of
Claude Code and OpenCode SHALL stay stored. A refused save SHALL leave the mode and the
engine as they were, with the reason under the mode list.

#### Scenario: From Claude Code in the Whisper mode
- **WHEN** the engine is Claude Code, the mode is Whisper and the user picks "Realtime voice"
- **THEN** one save stores the mode realtime and the engine Codex, the Engine list shows Codex, the engine card is titled "Codex", the Recognition field shows "Vosk", the wake model row shows, and the Claude subscription notice is gone

#### Scenario: From OpenCode in the text answers mode
- **WHEN** the engine is OpenCode, the mode is Whisper with text answers and the user picks "Realtime voice"
- **THEN** one save stores the mode realtime and the engine Codex, and the OpenCode executable and model stay as they were stored

#### Scenario: Claude Code again later
- **WHEN** the user went from Claude Code to "Realtime voice" and then picks Claude Code in the Engine list
- **THEN** one save stores the engine Claude Code and the mode Whisper, and the Claude Code card shows the executable and model set before

#### Scenario: Codex already the engine
- **WHEN** the engine is Codex, the mode is Whisper and the user picks "Realtime voice"
- **THEN** only the mode is saved, as before this change

#### Scenario: Save refused
- **WHEN** the engine is Claude Code, the mode is Whisper, the user picks "Realtime voice" and the save is refused
- **THEN** the mode list shows Whisper again, the engine stays Claude Code, and the reason shows under the mode list
