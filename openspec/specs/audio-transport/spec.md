# audio-transport Specification

## Purpose
Fixes the audio contracts between Discord and the rest of the app so that latency stays low,
speakers stay separate until they are deliberately mixed, and playback never runs after a
mute or outlives a voice connection.

## Requirements

### Requirement: Fixed frame format
Audio between Discord and the app SHALL be 48 kHz, 2 channels, 20 ms frames of 1920
16-bit samples. Decoding from Discord SHALL request exactly that format.

#### Scenario: Frame of another size
- **WHEN** a decoded packet does not hold 1920 samples
- **THEN** it is dropped and counted as malformed, and the tick is processed with the remaining speakers

### Requirement: Speakers are mixed without clipping losses
When several speakers are heard in one tick, each sample of the mix SHALL be the sum of the
speakers' samples, computed in 32-bit integers, divided by the number of speakers that
contributed to it; each speaker's own frame SHALL also be passed on
unmixed.

#### Scenario: Two loud speakers
- **WHEN** two speakers both produce a sample of 30 000 at the same instant
- **THEN** the mixed sample is 30 000, neither clipped to 32 767 nor halved to 16 383

#### Scenario: One speaker
- **WHEN** only one speaker is heard in a tick
- **THEN** the mixed frame equals that speaker's frame

#### Scenario: Per-speaker frames preserved
- **WHEN** three members speak in one tick
- **THEN** the tick sink receives the mixed frame and three (source id, frame) pairs

### Requirement: Receive path does bounded work
The voice tick handler SHALL not allocate after warm-up and SHALL not block; it hands frames
to bounded queues and returns.

#### Scenario: Slow consumer
- **WHEN** the consumer of the mixed frames falls behind
- **THEN** the oldest queued frame is dropped, a drop counter increases, and the tick handler returns without waiting

### Requirement: Output queue
Assistant audio toward Discord SHALL pass through a queue of at most 8 frames with a jitter
target that starts at 3 frames and grows to 8 after repeated underruns, relaxing again after
a long stable run; a full queue drops the oldest frame; underruns produce silence and are
counted.

#### Scenario: Burst of frames
- **WHEN** 20 frames arrive at once
- **THEN** 12 oldest frames are dropped and counted, and playback continues from the newest 8

#### Scenario: Gap in frames
- **WHEN** no frame is available at a read
- **THEN** one frame of silence is played and the underrun counter increases

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

### Requirement: Output source lifetime
The output source SHALL produce silence when it has nothing to play and SHALL end the track
only when it is closed on leave; nothing else ends it. That silence SHALL NOT be sent to
Discord (see "The bot sends audio only while there is something to hear").

#### Scenario: Session ends
- **WHEN** an assistant session ends and no audio is queued
- **THEN** the track keeps playing silence and is still attached, and after the hangover no packet of it reaches Discord

#### Scenario: Leave
- **WHEN** the bot leaves the channel
- **THEN** the source is closed, the next read reports end of stream, and exactly one track end event is logged

### Requirement: Counters in the status
The status SHALL carry the counters: ticks, frames in, frames dropped toward the assistant,
frames dropped toward Discord, underruns, malformed frames.

#### Scenario: Status after a minute in the channel
- **WHEN** the UI reads the status after the bot has been in a channel
- **THEN** every counter is a measured number, and none is a placeholder

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
- the last reported loss and jitter;
- how many times sending turned on, the time nothing was sent, and the cut starts.

When the bot leaves, one log line SHALL carry the same values.

#### Scenario: Leave
- **WHEN** the bot leaves a voice channel
- **THEN** one log line names the packets encoded, the pass-through state, the expected loss and its source, the last reported loss and jitter or that none were reported, how many times sending turned on, the time nothing was sent and the cut starts

#### Scenario: Pass-through lost
- **WHEN** songbird stops passing the app's packets through
- **THEN** the status and the log say so

#### Scenario: Nothing to say
- **WHEN** the bot has been in a channel for a minute and the assistant said nothing
- **THEN** sending never turned on and the time nothing was sent is close to a minute

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

### Requirement: The bot sends audio only while there is something to hear
While the bot is in a voice channel it SHALL send audio toward Discord only while the
assistant's voice or an earcon is audible. A frame of the assistant counts as audible at a
peak of 256 or more. When nothing has been audible for 500 ms, the bot SHALL send Discord's
five silence frames and then no packets at all until something is audible again. This
applies only to the bot's audio toward Discord; what it hears and sends to the assistant is
unchanged.

#### Scenario: An answer ends
- **WHEN** the assistant finishes an answer and says nothing more
- **THEN** about 500 ms after its last audible frame the bot sends five silence frames, then no packet reaches Discord, and Discord shows the bot as not speaking

#### Scenario: Nothing to say from the join on
- **WHEN** the bot joins a channel and no session is active, or a session is active but the assistant is silent
- **THEN** the bot sends no packets toward Discord

#### Scenario: A pause inside an answer
- **WHEN** the assistant pauses for less than 500 ms between two parts of an answer
- **THEN** the bot keeps sending through the pause

#### Scenario: Earcon while muted
- **WHEN** output is muted, nothing is being sent, and the mute tone is played
- **THEN** the tone is sent to Discord at its configured volume, and sending stops again 500 ms after it ends

### Requirement: Sending resumes before the audio plays
When sending resumes, the first audible frame SHALL reach Discord whole. The assistant's
voice that the output can already see ahead SHALL turn sending on before it plays. An
earcon that turns sending on SHALL start at most 40 ms later than it would have. A voiced
frame played before sending was back on SHALL be counted as a cut start.

#### Scenario: A new answer
- **WHEN** the assistant starts an answer while nothing is being sent
- **THEN** sending is on before the answer's first audible frame plays, no part of the answer's start is missing in Discord, and the cut-start counter stays at 0

#### Scenario: An earcon from silence
- **WHEN** the session start tone is played while nothing is being sent
- **THEN** the whole tone reaches Discord, starting no more than 40 ms after it was requested

### Requirement: Pausing the sending changes nothing else
Stopping and resuming the sending SHALL NOT change the bot's mute or deafen state in Discord
and SHALL NOT end the output track. While nothing is sent, the output SHALL keep its 20 ms
clock: the playout, the gains, the mute, the earcons and the latency lines SHALL work as
they do while sending.

#### Scenario: The member list during a pause
- **WHEN** members look at the bot in Discord while it sends nothing
- **THEN** the bot is shown neither muted nor deafened, only not speaking

#### Scenario: Playout during a pause
- **WHEN** the call's audio stays silent for a minute and then an answer comes
- **THEN** the output track is the one attached at the join, and the answer plays with the playout depth it had before the pause
