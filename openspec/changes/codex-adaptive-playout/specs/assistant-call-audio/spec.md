# Spec Delta

## MODIFIED Requirements

### Requirement: The answer plays in order without gaps
Packets of the assistant's voice SHALL be played in sequence order, one 20 ms frame per
tick, after a buffer whose depth follows the measured lateness of the packets. A packet
arriving after its turn has been played SHALL be discarded and counted as late. A packet
whose turn was bridged while the buffer was dry has not been played: it SHALL be played
when it arrives.

#### Scenario: Packets arrive out of order
- **WHEN** packets 10, 12, 11, 13 arrive in that order within the buffer time
- **THEN** they are played as 10, 11, 12, 13

#### Scenario: Packet arrives too late
- **WHEN** packet 20 arrives after frame 20 has already been played
- **THEN** it is discarded, the late counter increases, and playback continues with 21

#### Scenario: Packet arrives after a bridged moment
- **WHEN** the buffer ran dry waiting for packet 30, 40 ms were bridged, and packet 30 then arrives
- **THEN** packet 30 is played next and is not counted as late

### Requirement: Call audio counters
Each call SHALL count:
- frames sent, input drops and input underruns;
- packets received, late packets, concealed frames, frames recovered by FEC, playout
  underruns, output queue drops and decode errors;
- the time stretched in pauses, the time stretched in speech and the time removed;
- the frames bridged after a dry buffer and the audible gaps in speech;
- the current and the highest buffer depth.

The counters SHALL be in the status while the call runs and SHALL be written as one log
line when the call ends. Frames bridged after a dry buffer SHALL NOT be counted as concealed
frames, which count lost packets only.

#### Scenario: End of a call
- **WHEN** a call ends
- **THEN** one log line names the guild and every counter of that call with its value

#### Scenario: Clean local network
- **WHEN** a ten-second question is sent through a call on a loopback connection
- **THEN** the input drop, late and concealed counters are zero, and no audible gap is counted

#### Scenario: A stall is not a loss
- **WHEN** the server stalls for 200 ms in the middle of an answer and every packet arrives
- **THEN** the concealed counter does not change, and the stall shows in the bridged frames, the stretched time or the audible gaps

## ADDED Requirements

### Requirement: Buffer depth follows measured lateness
The buffer depth SHALL be derived from how late the packets arrived, measured against
the RTP clock, over the last seconds of the call. It SHALL be at least 60 ms and at most
500 ms. It SHALL grow after stalls and SHALL come back down after a calm period. No assumed
rate of server lateness SHALL be built in.

#### Scenario: Steady server
- **WHEN** packets arrive every 20 ms with a jitter under 10 ms for 30 s, from the starting depth of 200 ms
- **THEN** the depth is back at 60 ms by the end

#### Scenario: Repeated stalls
- **WHEN** the server stalls for 200 ms several times within 10 s
- **THEN** the depth grows to cover a 200 ms stall, staying at or under 500 ms

#### Scenario: Calm after stalls
- **WHEN** no packet has been more than 20 ms late for a long calm period after the stalls
- **THEN** the depth decreases back toward 60 ms

### Requirement: A call starts from what was learnt
A call SHALL start from the depth the previous call of the same guild learnt during this run
of the app. The first call SHALL start from a depth of 200 ms, which SHALL fade toward the
measured lateness like any learnt value.

#### Scenario: First stall of the first call
- **WHEN** the server stalls for 200 ms in the middle of a phrase 3 s into the first call
- **THEN** no audible gap is heard

#### Scenario: Next call
- **WHEN** a call ends with a depth of 300 ms learned from stalls, and the next call of the guild starts
- **THEN** the next call starts with a depth of 300 ms, not 200 ms

### Requirement: A late stream is stretched, pauses first
When less audio is buffered than the depth, the playout SHALL lengthen the voice to let the
buffer refill:
- a pause SHALL be lengthened first, by adding 20 ms to it;
- voiced speech SHALL be lengthened only when no pause has been usable for a while, by one
  pitch period at a time, keeping the pitch, and at most once every 60 ms.

Every stretch SHALL be counted, as pause time or speech time.

#### Scenario: Server 5 % slower with pauses
- **WHEN** a 30 s answer with normal pauses arrives 5 % slower than real time
- **THEN** it plays with no audible gap, and most of the added time is counted as pause time

#### Scenario: Pitch is kept
- **WHEN** a sustained vowel is stretched in speech
- **THEN** its fundamental frequency after stretching is within 2 % of the original

### Requirement: Extra delay is removed
When more audio is buffered than the depth plus a margin, the playout SHALL shorten pauses
first, and voiced speech by one pitch period at a time only when no pause has been usable,
until the buffered audio is back at the depth. Every removal SHALL be counted.

#### Scenario: A burst after a stall
- **WHEN** 400 ms of audio arrive at once after a stall
- **THEN** the buffered audio returns to the depth within a few seconds with no audible jump, and the removed time is counted

### Requirement: A dry buffer is bridged
When the buffer runs dry in the middle of speech, the playout SHALL bridge up to 60 ms with
the codec's loss concealment and then play silence until the next packet arrives. Each
bridged frame SHALL be counted. A gap that needed silence during speech SHALL be counted
as one audible gap. Silence after the end of an answer SHALL be counted as neither.

#### Scenario: Short stall
- **WHEN** the next packet arrives 40 ms late in the middle of speech
- **THEN** two frames are bridged, no audible gap is counted, and the late packet is played next

#### Scenario: Long stall
- **WHEN** the next packet arrives 300 ms late in the middle of speech
- **THEN** three frames are bridged, silence follows until the packet arrives, and one audible gap is counted

#### Scenario: End of an answer
- **WHEN** the assistant finishes speaking and no packets follow
- **THEN** no frame is bridged and no audible gap is counted

### Requirement: Playout never stops the app
No packet SHALL stop the playout or the app, whatever its size, duration or content.
- A packet that cannot be decoded SHALL be counted as a decode error and replaced by
  concealment.
- A packet longer than 20 ms SHALL be played in 20 ms frames.

#### Scenario: Corrupt and unusual packets
- **WHEN** the call delivers empty, truncated, random-byte and 2.5, 10, 40, 60 and 120 ms packets
- **THEN** playback continues, the undecodable ones are counted as decode errors, and the decodable ones are played
