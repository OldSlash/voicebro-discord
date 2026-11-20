# Spec Delta

## MODIFIED Requirements

### Requirement: The Claude subscription notice
While the engine is Claude Code, the sign-in is a Claude subscription or not known yet, the
access mode is not `owner`, and the user has not hidden the notice, the window SHALL show a
notice that a Claude subscription may be used only by its holder, with links to Anthropic's
Consumer Terms and Claude Code's legal page, and how to let only the holder's voice
through. With an API key or a cloud provider it SHALL show none.

#### Scenario: Subscription, everyone heard
- **WHEN** the engine is Claude Code, Claude Code is signed in with a Pro plan and the access mode is "Everyone"
- **THEN** the Claude Code card shows the notice, and the Assistant and Access cards show none

#### Scenario: Owner only
- **WHEN** the engine is Claude Code with a subscription and the access mode is "Only my Discord account"
- **THEN** no notice is shown

#### Scenario: API key
- **WHEN** the engine is Claude Code signed in with an API key and the access mode is "Everyone"
- **THEN** no notice is shown

#### Scenario: Hidden
- **WHEN** the user pressed Hide in the notice before, and the engine is Claude Code with a subscription and the access mode is "Everyone"
- **THEN** no notice is shown, also after a restart
