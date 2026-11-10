# Spec Delta

## Purpose

Makes every call after a pause continue the same conversation, and tells the model who is
in the channel in a way that reaches it without distracting it.

## ADDED Requirements

### Requirement: Instructions and context reach the model
The user's `codex.instructions` and the call context (participants, who called) SHALL be
sent as the last developer item of the call's initial items, which is the channel the probe
confirmed the model follows. The session prompt field MAY be set too but SHALL NOT be relied
on.

#### Scenario: Canary
- **WHEN** the instructions say to start every reply with a given word
- **THEN** the assistant's first reply starts with that word

#### Scenario: Who called
- **WHEN** a member starts a session and asks who called
- **THEN** the answer names that member's display name

### Requirement: Continuity across calls
Within one app run, a new call in the same guild SHALL reuse the thread while the app-server
generation is unchanged, pass the same voice session id and the last realtime session id,
and include the transcript history as initial items, capped at 100 items or 24 000 bytes,
oldest dropped first. If the history is rejected the call SHALL be retried once without it.

#### Scenario: Remember a number
- **WHEN** the user says a number in one call, ends it, starts a new call and asks for the number
- **THEN** the assistant answers with that number

#### Scenario: History too large
- **WHEN** the transcript history exceeds the cap
- **THEN** the oldest entries are left out and the call still starts

### Requirement: Speaker labels
`assistant.speaker_labels` SHALL control what the model learns about speakers: `start`
puts the names only in the initial items; `changes` also sends a developer note when the
speaker changes; `off` sends nothing. Nothing else SHALL be sent mid-call.

#### Scenario: Changes mode
- **WHEN** another member starts talking during a call in `changes` mode
- **THEN** one note naming them is sent, and no note is sent while the same person keeps talking

#### Scenario: Off
- **WHEN** labels are off
- **THEN** no name reaches the model and the call still works
