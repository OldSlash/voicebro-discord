# Spec Delta

## ADDED Requirements

### Requirement: The people's transcripts reach the gate
While its call is live, the Codex session SHALL hand the call's transcript of the people's
speech to the gate piece by piece as Codex sends it, joined per turn, so the gate can find
the deactivation phrase before the voice replies. It SHALL hand over nothing of the voice's
own speech, and nothing that arrives after the call stopped.

#### Scenario: A member speaks during an answer
- **WHEN** a member speaks while the voice is answering in a live call
- **THEN** the gate receives the turn's text so far after each piece Codex sends, without waiting for the end of the turn or for the voice's reply

#### Scenario: The voice says the phrase
- **WHEN** the voice's own answer contains the deactivation phrase
- **THEN** nothing of the voice's transcript reaches the gate and the session continues

#### Scenario: Transcript after the stop
- **WHEN** the call has been stopped and a transcript piece of the people's speech from that call arrives afterwards
- **THEN** nothing reaches the gate
