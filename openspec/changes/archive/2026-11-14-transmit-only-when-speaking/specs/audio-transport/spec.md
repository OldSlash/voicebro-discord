# Spec Delta

## ADDED Requirements

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

## MODIFIED Requirements

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
