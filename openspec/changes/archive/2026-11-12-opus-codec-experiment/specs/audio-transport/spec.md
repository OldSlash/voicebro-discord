# Spec Delta

## ADDED Requirements

### Requirement: Speakers are decoded by the selected codec
With `libopus` selected at the join, songbird SHALL decode each speaker as before. With
opus-rs selected at the join, songbird SHALL only decrypt the speakers' packets, and the
app SHALL decode each speaker itself with opus-rs: one decoder per SSRC, 48 kHz
stereo, from the same payload songbird decodes, skipping the RTP header extension as
songbird does, concealing a 20 ms frame for every packet missing from the sequence and for
every tick on which a speaker's packet is missed. The frames SHALL reach the mix, the wake
engine and the assistant as songbird's decoded frames do, and the voice tick handler SHALL
still not block. A speaker's decoder SHALL be dropped when songbird drops the speaker's
state.

#### Scenario: Two people speak with opus-rs selected
- **WHEN** two people speak in the channel and opus-rs was selected at the join
- **THEN** each is decoded by their own opus-rs decoder, and their frames are mixed and passed on as with songbird's decoding

#### Scenario: A packet is missing
- **WHEN** a speaker's packet with the next sequence number never arrives and songbird reports the tick without a packet
- **THEN** the app conceals that speaker's frame with the same decoder, and the speaker's next packet decodes as usual

#### Scenario: Same audio as songbird
- **WHEN** a recorded sequence of a speaker's packets is decoded by the app's own path with libopus and by songbird's decoder
- **THEN** both give the same samples
