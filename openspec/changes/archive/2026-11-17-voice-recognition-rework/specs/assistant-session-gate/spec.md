# Spec Delta

## ADDED Requirements

### Requirement: Deactivation from the call's transcript
In wake-word mode, while a session is active and mute is off, the gate SHALL check the
call's transcript of the people's speech, joined per turn, for the selected wake model's
deactivation phrases. Words match whole, ignoring case and punctuation. A turn that ended
less than 2 s before SHALL be read with the next one, and a match SHALL end in the newer
turn. A match SHALL end the session as the wake engine's deactivation does, and the log
SHALL name the transcript as the source.

#### Scenario: The phrase cut into two turns
- **WHEN** the deactivation phrase is "отбой братан" and the call's transcript ends one turn with " Отбой" and starts the next with ", братан" 0.2 s later
- **THEN** the session ends

### Requirement: A phrase at the end of the transcript waits for its word to end
A deactivation phrase whose last word is the last word of the transcript so far, with
nothing after it, SHALL count once the text shows the word ended: the next piece starts
with a space or punctuation, the turn ends, or 300 ms pass without a newer piece. While it
waits, the assistant's voice SHALL be silenced, and it SHALL come back if the word goes on.

#### Scenario: Held phrase confirmed
- **WHEN** the deactivation phrase is "goodbye" and a turn's transcript so far is " Goodbye", with nothing after it
- **THEN** the voice is silenced at once and the session ends 300 ms later

#### Scenario: The wake engine misses the phrase during an answer
- **WHEN** a member says the deactivation phrase over the assistant's answer, the wake engine does not confirm it, and the call's transcript of that speech holds it
- **THEN** the assistant's voice stops within one frame of the match, the falling earcon plays, the call stops, and the log line names the call's transcript

#### Scenario: The phrase arrives in pieces
- **WHEN** the deactivation phrase is "goodbye" and the transcript of one turn arrives as " Good" and then "bye", and nothing follows within 300 ms
- **THEN** the session ends without waiting for the end of the turn

#### Scenario: A longer word is not the phrase
- **WHEN** the deactivation phrase is "stop" and the transcript of one turn arrives as " stop" and then "watch"
- **THEN** the session continues, and the voice, silenced after " stop", plays again after "watch"

#### Scenario: The voice replies while the phrase is held
- **WHEN** the transcript of one turn ends with " Отбой, братан" and the voice starts its reply 100 ms later
- **THEN** nothing of the reply is heard, and the session ends with the falling earcon 300 ms after that piece

#### Scenario: Punctuation and case
- **WHEN** the deactivation phrase is "goodbye" and a turn's transcript reads "Okay, GOODBYE!"
- **THEN** the session ends

#### Scenario: Both sources hear the phrase
- **WHEN** the wake engine confirms the deactivation phrase and the call's transcript holding it arrives afterwards
- **THEN** the session ends once and the falling earcon plays once

#### Scenario: The wake word right after the phrase
- **WHEN** a member says the deactivation phrase followed by an activation phrase, the call's transcript ends the session, and the wake engine confirms the activation phrase 0.8 s later
- **THEN** no new session starts

#### Scenario: Always mode
- **WHEN** the session mode is always and the transcript holds a deactivation phrase
- **THEN** the session continues

#### Scenario: Phrases of another model
- **WHEN** only a model other than the selected one has deactivation phrases stored and the transcript holds one of them
- **THEN** the session continues

## MODIFIED Requirements

### Requirement: Deactivation is immediate
On a deactivation phrase, from the wake engine or from the call's transcript, the gate
SHALL mute the assistant's output and flush what is queued at once, play the falling
earcon when earcons are on, then tell the assistant the session is not wanted.

#### Scenario: Deactivation mid-answer
- **WHEN** a member says the deactivation phrase while the assistant is speaking
- **THEN** the assistant's voice stops within one frame, the rest of the answer is never heard, and the falling earcon is heard

#### Scenario: Earcons off
- **WHEN** earcons are off and a member says the deactivation phrase while the assistant is speaking
- **THEN** the assistant's voice stops within one frame and no tone plays
