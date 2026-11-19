# speaker-consent Specification

## Purpose
Lets each person in the bot's voice channel agree with `/optin` that their speech goes to
the assistant, withdraw with `/optout`, and read the operator's privacy policy with
`/privacy`, and limits the voices and names that reach the assistant to those who agreed
while the access mode is `agreed`.

## Requirements

### Requirement: The agreed mode lets through only people who agreed
While `discord.access` is `agreed`, the app SHALL pass to the wake word, the speech
detection and the assistant only the audio of sources that belong to the linked owner or to
a person with a consent record. Every other source, bots included, and every source with no
known account SHALL be dropped before the mix, counted, and logged once per source.

#### Scenario: A person without a record says the phrase
- **WHEN** the mode is `agreed` and a member who never ran `/optin` says the activation phrase
- **THEN** no session starts and the counter of ignored frames grows

#### Scenario: Two people at once
- **WHEN** a member who agreed asks a question during a session while a member who did not agree talks over them
- **THEN** the assistant receives only the audio of the member who agreed

#### Scenario: The owner without a record
- **WHEN** the mode is `agreed`, an owner is linked and has no consent record, and the owner says the activation phrase
- **THEN** a session starts

#### Scenario: Nobody agreed and nobody linked
- **WHEN** the mode is `agreed`, no owner is linked, no record exists, and a member runs `/join` and says the activation phrase
- **THEN** the bot joins the channel and no session starts

### Requirement: An unknown source with one person in the channel
In the `agreed` mode a source with no known account SHALL be taken as the only non-bot
member in the bot's voice channel, and SHALL pass only when that member is the owner or has
a record.

#### Scenario: The only person agreed
- **WHEN** audio arrives from a source Discord never announced and the only person in the channel has a record
- **THEN** the audio passes

#### Scenario: Two people in the channel
- **WHEN** audio arrives from a source Discord never announced and two members are in the channel
- **THEN** the audio is dropped

### Requirement: `/optin` shows the consent text
`/optin` SHALL reply only to the sender with the consent text, a link to the privacy
policy URL from the settings, and the buttons "I agree" and "Cancel". The text SHALL say
that after the activation phrase their voice (in the voice mode) or the text of their
speech (in the Whisper mode) is sent to the selected engine's recipient (OpenAI through
Codex; Anthropic through Claude Code; through OpenCode, the provider of the model in use
and, for web searches the model makes, OpenCode's search service), what is stored about
them, and that `/optout` withdraws.

#### Scenario: Consent text
- **WHEN** a member without a record runs `/optin` while a privacy policy URL is set
- **THEN** only that member sees the consent text with the policy link and the two buttons, and nothing is recorded yet

#### Scenario: Consent text with Claude Code
- **WHEN** the engine is Claude Code and a member runs `/optin` while a privacy policy URL is set
- **THEN** the text says the text of their speech goes to Anthropic through Claude Code, and does not name OpenAI

#### Scenario: Consent text with OpenCode
- **WHEN** the engine is OpenCode with the model `anthropic/claude-sonnet-4-5` and a member runs `/optin`
- **THEN** the text names Anthropic through OpenCode and says that web searches go to OpenCode's search service

### Requirement: `/optin` needs a privacy policy
While `discord.privacy_url` is empty, `/optin` SHALL record nothing and SHALL reply only to
the sender that the operator of this bot has not published a privacy policy yet.

#### Scenario: No policy
- **WHEN** a member runs `/optin` while no privacy policy URL is set
- **THEN** no buttons are shown, no record is made, and the reply says the operator has not published a privacy policy

### Requirement: "I agree" records the person
Pressing "I agree" SHALL record the account that pressed it, with its display name, the
time, the consent text's version and the assistant engine, and SHALL reply only to that
person that their voice is heard after the activation phrase while the mode is `agreed`.
When the mode is another one, the reply SHALL name it. "Cancel" SHALL record nothing.

#### Scenario: Agree in the agreed mode
- **WHEN** the mode is `agreed` and a member presses "I agree" under their `/optin` reply
- **THEN** the member is recorded, the Access card lists them within a second, and their voice reaches the wake word from the next frame on

#### Scenario: Agree in the everyone mode
- **WHEN** the mode is `everyone` and a member presses "I agree"
- **THEN** the member is recorded and the reply says that this bot currently hears everyone in the channel

#### Scenario: Cancel
- **WHEN** a member presses "Cancel" under their `/optin` reply
- **THEN** no record is made and the reply says nothing changed

#### Scenario: The text changed before the press
- **WHEN** a member presses "I agree" under a consent text whose version is older than the current one
- **THEN** no record is made and the reply asks them to run `/optin` again

### Requirement: Agreeing again changes nothing
`/optin` from a person who already has a record for the current consent text and engine
SHALL reply only to them with the time they agreed and how to withdraw, and SHALL show no
buttons.

#### Scenario: Second `/optin`
- **WHEN** a recorded member runs `/optin` again
- **THEN** the reply names the time of their consent and `/optout`, and the record is unchanged

### Requirement: `/optout` deletes the record
`/optout` SHALL delete the sender's record and SHALL reply only to them that their voice no
longer reaches the assistant in the `agreed` mode and that what they said before was
already sent. A sender without a record SHALL be told there was nothing to remove. The
change SHALL apply from the next voice frame, also during a session.

#### Scenario: Withdraw during a session
- **WHEN** the mode is `agreed` and a recorded member runs `/optout` while a session is active
- **THEN** their record is gone from the Access card and the file, and their audio is dropped from the next frame on

#### Scenario: Nothing to remove
- **WHEN** a member without a record runs `/optout`
- **THEN** the reply says there was nothing to remove

### Requirement: Remove in the window acts as `/optout`
Remove next to a person in the Access card SHALL delete their record as `/optout` does. The
app SHALL NOT send that person any message about it.

#### Scenario: Remove
- **WHEN** the user presses Remove next to a recorded member
- **THEN** the member leaves the list, the record is deleted from the file, their audio is dropped from the next frame on in the `agreed` mode, and no message is sent to them

### Requirement: `/privacy` shows the policy
`/privacy` SHALL reply only to the sender with the privacy policy URL from the settings, or
with a note that the operator has not published one.

#### Scenario: Policy set
- **WHEN** a member runs `/privacy` while a URL is set
- **THEN** only they see the URL

#### Scenario: No policy
- **WHEN** a member runs `/privacy` while no URL is set
- **THEN** the reply says the operator has not published a privacy policy

### Requirement: The records are stored encrypted in the app's folder
The app SHALL keep the records as one file in its own folder, encrypted so that only the
same Windows user on the same PC can read it, and never in the settings file. A file that
cannot be decrypted or read SHALL count as no records and SHALL be reported in the log once
at start and in the Access card.

#### Scenario: Restart
- **WHEN** two members are recorded and the app is closed and started again
- **THEN** both are still recorded

#### Scenario: Edited file
- **WHEN** one byte of the records file is changed and the app starts
- **THEN** no member is recorded, the Access card says the records could not be read, and the log names the file

### Requirement: Records end with a new consent text or engine
When the app starts with, or switches to, a consent text version or an assistant engine
other than the one a record was made for, it SHALL delete that record and SHALL log how
many records it deleted and why. For OpenCode the engine SHALL include the provider of the
model in use, so a change of that provider ends the records too.

#### Scenario: New consent text
- **WHEN** an update raises the consent text's version and the app starts with three records
- **THEN** no member is recorded, the log says three records were removed because the consent text changed, and each member must run `/optin` again

#### Scenario: Another engine
- **WHEN** two members agreed while the engine was Codex and the user picks Claude Code
- **THEN** both records are deleted, the log says two records were removed because the assistant engine changed, and the Access card lists nobody

#### Scenario: OpenCode's provider changes
- **WHEN** a member agreed while OpenCode used `openai/gpt-5` and the user sets `opencode.model` to `anthropic/claude-sonnet-4-5`
- **THEN** the record is deleted for the same reason

### Requirement: The agreed mode is named in `/join` and `/status`
While the mode is `agreed`, the `/join` reply and the `/status` reply SHALL add that only
the owner and people who ran `/optin` are heard, and SHALL point to `/privacy`.

#### Scenario: Join in the agreed mode
- **WHEN** a member runs `/join` while the mode is `agreed`
- **THEN** the reply in the channel says that only the owner and people who ran `/optin` are heard and names `/privacy`

#### Scenario: Status in the everyone mode
- **WHEN** a member runs `/status` while the mode is `everyone`
- **THEN** the reply has no line about `/optin`

### Requirement: The commands stay open to everyone
The `agreed` mode SHALL NOT limit who can use `/join`, `/leave`, `/status`, `/optin`,
`/optout`, `/privacy` or the panel's Mute button, and SHALL change nothing about who hears
the assistant's voice or sees its posted answers.

#### Scenario: A member without a record runs `/join`
- **WHEN** the mode is `agreed` and a member without a record runs `/join` from a voice channel
- **THEN** the bot joins that channel, and only the owner and recorded members are heard there
