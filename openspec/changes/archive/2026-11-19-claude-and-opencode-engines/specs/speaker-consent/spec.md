## MODIFIED Requirements

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
