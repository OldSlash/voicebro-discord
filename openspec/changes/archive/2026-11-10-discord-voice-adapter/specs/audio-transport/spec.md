# Spec Delta

## Purpose

Fixes the audio contracts between Discord and the rest of the app so that latency stays low,
speakers stay separate until they are deliberately mixed, and playback never runs after a
mute or outlives a voice connection.

## ADDED Requirements

### Requirement: Fixed frame format
Audio between Discord and the app SHALL be 48 kHz, 2 channels, 20 ms frames of 1920
16-bit samples. Decoding from Discord SHALL request exactly that format.

#### Scenario: Frame of another size
- **WHEN** a decoded packet does not hold 1920 samples
- **THEN** it is dropped and counted as malformed, and the tick is processed with the remaining speakers

### Requirement: Speakers are mixed without clipping losses
When several speakers are heard in one tick, their samples SHALL be summed in 32-bit
integers and clamped to 16-bit once; each speaker's own frame SHALL also be passed on
unmixed.

#### Scenario: Two loud speakers
- **WHEN** two speakers both produce a sample of 30 000 at the same instant
- **THEN** the mixed sample is 32 767, not 16 383

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
SHALL play silence plus any pending earcon; when input is muted the frames toward the
assistant SHALL be silence of the same size, so the stream is never cut.

#### Scenario: Mute during an answer
- **WHEN** mute is turned on in the middle of an answer and off again later
- **THEN** no sample of the old answer is heard after the unmute

#### Scenario: Earcon while muted
- **WHEN** an earcon is requested while output is muted
- **THEN** the earcon is audible in the channel at its configured volume

### Requirement: Output source lifetime
The output source SHALL produce silence when it has nothing to play and SHALL end the track
only when it is closed on leave; nothing else ends it.

#### Scenario: Session ends
- **WHEN** an assistant session ends and no audio is queued
- **THEN** the track keeps playing silence and is still attached

#### Scenario: Leave
- **WHEN** the bot leaves the channel
- **THEN** the source is closed, the next read reports end of stream, and exactly one track end event is logged

### Requirement: Counters in the status
The status SHALL carry the counters: ticks, frames in, frames dropped toward the assistant,
frames dropped toward Discord, underruns, malformed frames.

#### Scenario: Status after a minute in the channel
- **WHEN** the UI reads the status after the bot has been in a channel
- **THEN** every counter is a measured number, and none is a placeholder
