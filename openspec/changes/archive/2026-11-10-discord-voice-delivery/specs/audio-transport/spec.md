# Spec Delta

## ADDED Requirements

### Requirement: The voice toward Discord is encoded with loss protection
The app SHALL encode the assistant's voice toward Discord itself, as 20 ms Opus packets
carrying in-band forward error correction, and Discord SHALL receive those packets as
encoded. Everything applied to the voice before Discord SHALL happen before this encoder:
playout, output gain, earcons and mute.

#### Scenario: One packet lost toward a listener
- **WHEN** one packet of the assistant's voice is lost and the next one arrives
- **THEN** the next packet carries enough redundancy for the lost 20 ms to be rebuilt from it

#### Scenario: Earcons and mute still apply
- **WHEN** an earcon plays during an answer, or output is muted
- **THEN** the packets sent to Discord carry the earcon mixed in, or silence with the earcon, as before

#### Scenario: Packets pass through unchanged
- **WHEN** the output is the only source playing in the call
- **THEN** each packet the app encodes reaches Discord byte for byte, without being decoded and encoded again

### Requirement: Protection follows the measured loss
The redundancy setting of the encoder SHALL follow the measured loss of the assistant's voice
on the Discord leg: more loss gives more redundancy, and no loss gives none beyond what Opus
needs. When no measurement is available, a cautious default SHALL be used, and the status
SHALL say that the default is in use.

#### Scenario: Loss is reported
- **WHEN** Discord reports a loss on our stream that rises from none to several percent
- **THEN** the encoder's expected-loss setting rises with it, and falls again when the reports show no loss

#### Scenario: Nothing is reported
- **WHEN** Discord sends no reports on our stream
- **THEN** the default setting is used, and the status names it as the default

### Requirement: The voice server is named
At each join the app SHALL log the voice server's host and region, without any token or
session secret, and SHALL show the region in the channel status.

#### Scenario: Join
- **WHEN** the bot joins a voice channel
- **THEN** one log line names the voice server host and region, and the channel status shows the region

### Requirement: Delivery counters
While the bot is in a channel the status SHALL carry, for the assistant's voice toward
Discord:
- the packets encoded;
- whether songbird passed them through unchanged;
- the expected loss in use and its source;
- the last reported loss and jitter.

When the bot leaves, one log line SHALL carry the same values.

#### Scenario: Leave
- **WHEN** the bot leaves a voice channel
- **THEN** one log line names the packets encoded, the pass-through state, the expected loss and its source, and the last reported loss and jitter or that none were reported

#### Scenario: Pass-through lost
- **WHEN** songbird stops passing the app's packets through
- **THEN** the status and the log say so
