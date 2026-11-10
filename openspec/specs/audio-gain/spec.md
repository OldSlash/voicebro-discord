# audio-gain Specification

## Purpose
Lets the user set how loud the assistant hears the channel and how loud it speaks in it,
and shows when a setting drives the audio into clipping.

## Requirements

### Requirement: Input gain scales what the assistant hears
Every sample of every frame the session gate sends to the assistant SHALL be multiplied by
`audio.input_gain`. The audio fed to the wake engine SHALL not be scaled.

#### Scenario: Gain of two
- **WHEN** the input gain is 2.0 and a frame with samples of 1 000 is sent to the assistant
- **THEN** the assistant receives samples of 2 000 and the wake engine receives samples of 1 000

#### Scenario: Gain of zero
- **WHEN** the input gain is 0.0
- **THEN** the assistant receives silence and the wake engine still receives the channel

### Requirement: Output gain scales how loud the assistant speaks
Every sample of the assistant's audio played into Discord SHALL be multiplied by
`audio.output_gain`. Earcons SHALL play at their own configured volume, not scaled by it.

#### Scenario: Gain of two and a half
- **WHEN** the output gain is 2.5 and the assistant's audio holds samples of 1 000
- **THEN** samples of 2 500 are played into the channel

#### Scenario: Earcon unaffected
- **WHEN** the output gain is 4.0 and a session start earcon plays
- **THEN** the earcon plays at the earcon volume setting, the same as with a gain of 1.0

### Requirement: Clipping is clamped and counted
A scaled sample beyond the 16-bit range SHALL be clamped to the range limit and counted as
clipped, separately for each direction.

#### Scenario: Loud speech with a high gain
- **WHEN** the input gain is 4.0 and a sample of 10 000 is scaled
- **THEN** the assistant receives 32 767 and the input clip counter increases by one

### Requirement: Changes apply to a running call
A saved gain SHALL take effect within one 20 ms frame in every voice channel the bot is in,
without leaving, rejoining or restarting a call.

#### Scenario: Slider moved during an answer
- **WHEN** the user saves a new output gain while the assistant is speaking
- **THEN** the next frame played uses the new gain and the call continues

### Requirement: Levels are visible
The status SHALL carry, per direction, the peak sample level of the last second as a share
of full scale and the clip counter.

#### Scenario: Reading the levels
- **WHEN** the UI reads the status during a session
- **THEN** it gets a measured peak for the audio toward the assistant and toward Discord, and both clip counters
