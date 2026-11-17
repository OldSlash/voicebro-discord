## MODIFIED Requirements

### Requirement: Each Whisper answer is measured
For each answer in Whisper mode the app SHALL log one latency line with:

- the time from the end of the speech to the text;
- the audio length, the model and the device;
- the time from the text to Codex's first words;
- the time from the first words to the voice;
- the time from the end of the speech to the voice.

The line SHALL be added to the status's recent turns.

#### Scenario: One answer
- **WHEN** a question is answered in Whisper mode
- **THEN** the log has one `[latency:<guild>] whisper turn <n>: …` line with the four times, and the Settings tab's Whisper card shows them
