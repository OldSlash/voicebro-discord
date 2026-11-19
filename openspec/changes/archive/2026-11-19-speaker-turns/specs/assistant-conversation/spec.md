## MODIFIED Requirements

### Requirement: Speaker labels
`assistant.speaker_labels` SHALL control what the model learns about speakers. While one
speaker at a time is off: `start` puts the names only in the initial items; `changes` also
sends a developer note when the speaker changes; `off` sends nothing. While it is on, both
`start` and `changes` also send a note naming the person at every change of the floor,
except for the caller already named in the initial items; `off` sends nothing. Nothing else
SHALL be sent mid-call.

#### Scenario: Changes mode
- **WHEN** another member starts talking during a call in `changes` mode
- **THEN** one note naming them is sent, and no note is sent while the same person keeps talking

#### Scenario: Start mode, a person who waited
- **WHEN** one speaker at a time is on, labels are `start`, and the floor passes from Alex to Sam, who waited
- **THEN** one note naming Sam is sent before Sam's speech reaches the call

#### Scenario: Start mode, people take turns
- **WHEN** one speaker at a time is on, labels are `start`, Alex stops, the floor is free, and Sam starts talking later
- **THEN** one note naming Sam is sent as Sam takes the floor

#### Scenario: Off
- **WHEN** labels are off
- **THEN** no name reaches the model and the call still works
