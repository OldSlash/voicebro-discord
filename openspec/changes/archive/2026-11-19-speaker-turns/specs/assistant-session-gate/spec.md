## ADDED Requirements

### Requirement: One speaker at a time is optional
The gate SHALL send the call one person at a time only while `assistant.one_speaker_at_a_time`
is on. While it is off, the gate SHALL send the call the mix of every passing speaker and
keep the pre-roll of the mix, as before this setting existed. A change SHALL apply from the
next session without a rejoin.

#### Scenario: Off by default
- **WHEN** Alex and Sam talk over each other during a session and the setting was never changed
- **THEN** the call receives the mix of both, as before

#### Scenario: Turned on in a channel
- **WHEN** the user turns one speaker at a time on while the bot is in a voice channel
- **THEN** the bot stays in the channel, and the next session sends one person at a time

### Requirement: One speaker at a time toward the call
In realtime mode, while one speaker at a time is on and a session passes audio to the call,
the gate SHALL send the call the frames of one person at a time, never a mix of several
people. The first person to speak SHALL take the floor and their frames SHALL go to the
call as they arrive. This SHALL hold in both session modes.

#### Scenario: One person talks
- **WHEN** only Alex speaks during a session
- **THEN** the call receives Alex's frames with no added delay, as with the setting off

#### Scenario: Two people talk at once
- **WHEN** Alex has the floor and Sam starts talking while Alex is still speaking
- **THEN** the call receives only Alex's frames until the floor passes, and none of Sam's frames are mixed into them

#### Scenario: Always mode
- **WHEN** the session mode is always and Alex and Sam talk over each other
- **THEN** the call receives one of them at a time, as in wake-word mode

### Requirement: A waiting person's speech is kept
While one speaker at a time is on, frames of a person who speaks while someone else has
the floor SHALL be kept in a buffer of that person's own, in order. A buffer SHALL hold at
most 15 s; beyond that the oldest frames SHALL be dropped and counted. The end of the
session, mute and a deactivation SHALL empty every buffer.

#### Scenario: Buffer full
- **WHEN** Sam talks for 20 s while Alex keeps the floor
- **THEN** Sam's buffer holds the last 15 s of Sam's speech and the dropped frames are counted

#### Scenario: Deactivation with someone waiting
- **WHEN** Sam is waiting with a buffer and Alex says the deactivation phrase
- **THEN** the session ends and nothing of Sam's buffer reaches the call

### Requirement: The floor passes after a pause
The floor SHALL pass when the person holding it has sent no voiced frame for 15 receive
ticks (300 ms). It SHALL pass to the person who started waiting first; with nobody waiting
the floor is free and the next person to speak takes it. Silence SHALL be counted in
receive ticks, never on a clock.

#### Scenario: Pause between two people
- **WHEN** Alex stops talking, Sam has been waiting since before Alex stopped, and 300 ms pass
- **THEN** the floor passes to Sam and Sam's buffer starts going to the call

#### Scenario: A short pause in a sentence
- **WHEN** Alex pauses for 200 ms in the middle of a sentence while Sam is waiting
- **THEN** Alex keeps the floor

#### Scenario: Three people
- **WHEN** Sam starts waiting before Kim does, and Alex then stops talking
- **THEN** the floor goes to Sam, and to Kim after Sam's pause

### Requirement: A buffer goes to the call at the normal pace
When a person who waited gets the floor, their buffer SHALL go to the call one frame per
receive tick, in order, followed by their live frames while they keep talking. Silent
frames at the front of the buffer SHALL be skipped to catch up, as the pre-roll replay
does.

#### Scenario: Still talking when the floor passes
- **WHEN** Sam gets the floor with 2 s in the buffer and is still talking
- **THEN** the call receives Sam's buffered speech and then Sam's live speech, in order and without a gap in the words

#### Scenario: Pauses are skipped
- **WHEN** Sam's buffer holds 1 s of speech, 1 s of silence and 1 s of speech
- **THEN** the call receives the speech of both parts and less than 1 s of the silence

### Requirement: A waiting person does not cut the answer
While the assistant's voice plays, the floor SHALL NOT pass to a person who waited; their
buffer SHALL go to the call after the voice has stopped. A person who starts talking while
nobody holds the floor and nobody waits SHALL go to the call live.

#### Scenario: The answer starts before the floor passes
- **WHEN** the assistant starts answering Alex while Sam's buffer is waiting
- **THEN** the answer plays to its end and Sam's buffered question reaches the call after it

#### Scenario: Someone talks over the answer
- **WHEN** nobody waits and Kim starts talking while the assistant speaks
- **THEN** Kim's frames go to the call live, as with the setting off

### Requirement: Floor counters in the status
The gate status SHALL show per guild the number of people waiting for the floor, the floor
passes so far, and the frames dropped from full buffers.

#### Scenario: Status while someone waits
- **WHEN** the UI reads the status while Sam waits for the floor
- **THEN** it shows one person waiting

## MODIFIED Requirements

### Requirement: Pre-roll replay from the end of the phrase
The gate SHALL keep about 5 s of audio: of the mix while one speaker at a time is off, of
each speaker on their own while it is on. On activation it SHALL replay that audio starting
at the end of the activation phrase (from the recognizer's timing, or 1 s before the hit
plus an 80 ms margin), hold the replay while the assistant is not ready, catch up by
skipping silent frames, and cap the replay at about 6 s. While one speaker at a time is on,
the caller SHALL take the floor, the replay SHALL hold only the caller's audio, and what
other people said after the end of the phrase SHALL go into their buffers as waiting speech.

#### Scenario: Question right after the phrase
- **WHEN** a member says the activation phrase and a question in one breath, and the assistant takes 2 s to become ready
- **THEN** the assistant receives the question and not the phrase, and the live audio continues after the replay

#### Scenario: Assistant never becomes ready
- **WHEN** the assistant does not become ready within the replay cap
- **THEN** the held audio is dropped, the drop is logged, and the session ends with an error in the status

#### Scenario: Someone else talks during the phrase
- **WHEN** one speaker at a time is on and Alex says the activation phrase and a question while Sam is talking
- **THEN** the replay holds only Alex's speech, and Sam's speech after the end of the phrase waits in Sam's buffer

### Requirement: Call context for the assistant
Before a session starts the gate SHALL give the assistant the caller's name (if a phrase
started it) and the participant names. While one speaker at a time is off, it SHALL report
the new set when the set of people talking changes; while it is on, it SHALL report the
person the floor passes to, before any of their frames reach the call, as requirement
"Speaker labels" of `assistant-conversation` says. While the access mode is `agreed` or
`owner`, the participant names SHALL be only those of the people whose voice that mode lets
through.

#### Scenario: Session start
- **WHEN** a member starts a session with the activation phrase
- **THEN** the assistant receives that member's name as the caller and the current participants

#### Scenario: A member who did not agree
- **WHEN** the access mode is `agreed`, Alex has a consent record, Sam has none, both are in the channel, and Alex starts a session
- **THEN** the assistant receives Alex as the caller and as the only participant, and the name Sam is not sent

#### Scenario: Owner mode
- **WHEN** the access mode is `owner` and the linked owner starts a session with two other members in the channel
- **THEN** the participants sent to the assistant are the owner alone

#### Scenario: Floor passed
- **WHEN** one speaker at a time is on, speaker labels are not `off`, and the floor passes from Alex to Sam, who waited
- **THEN** the assistant is told Sam is speaking before the first frame of Sam's buffer reaches the call
