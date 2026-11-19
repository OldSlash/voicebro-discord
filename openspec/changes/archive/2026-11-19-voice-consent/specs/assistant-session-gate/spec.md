## MODIFIED Requirements

### Requirement: Call context for the assistant
Before a session starts the gate SHALL give the assistant the caller's name (if a phrase
started it) and the participant names; when the set of people talking changes it SHALL
report the new set. While the access mode is `agreed` or `owner`, the participant names
SHALL be only those of the people whose voice that mode lets through.

#### Scenario: Session start
- **WHEN** a member starts a session with the activation phrase
- **THEN** the assistant receives that member's name as the caller and the current participants

#### Scenario: A member who did not agree
- **WHEN** the access mode is `agreed`, Alex has a consent record, Sam has none, both are in the channel, and Alex starts a session
- **THEN** the assistant receives Alex as the caller and as the only participant, and the name Sam is not sent

#### Scenario: Owner mode
- **WHEN** the access mode is `owner` and the linked owner starts a session with two other members in the channel
- **THEN** the participants sent to the assistant are the owner alone
