# Spec Delta

## MODIFIED Requirements

### Requirement: Speakers are mixed without clipping losses
When several speakers are heard in one tick, each sample of the mix SHALL be the sum of the
speakers' samples, computed in 32-bit integers, divided by the number of speakers that
contributed to it; each speaker's own frame SHALL also be passed on unmixed.

#### Scenario: Two loud speakers
- **WHEN** two speakers both produce a sample of 30 000 at the same instant
- **THEN** the mixed sample is 30 000, neither clipped to 32 767 nor halved to 16 383

#### Scenario: One speaker
- **WHEN** only one speaker is heard in a tick
- **THEN** the mixed frame equals that speaker's frame

#### Scenario: Per-speaker frames preserved
- **WHEN** three members speak in one tick
- **THEN** the tick sink receives the mixed frame and three (source id, frame) pairs

### Requirement: Mute flushes and silences, earcons still play
When output is muted the queue SHALL discard what it holds and what arrives, and the source
SHALL play silence plus any pending earcon; while mute is on no frame of the channel SHALL
be sent toward the assistant, because mute ends the session.

#### Scenario: Mute during an answer
- **WHEN** mute is turned on in the middle of an answer and off again later
- **THEN** no sample of the old answer is heard after the unmute

#### Scenario: Earcon while muted
- **WHEN** an earcon is requested while output is muted
- **THEN** the earcon is audible in the channel at its configured volume

#### Scenario: Input while muted
- **WHEN** members speak while mute is on
- **THEN** no frame is sent toward the assistant
