# Spec Delta

## MODIFIED Requirements

### Requirement: The Claude subscription notice in the window
While requirement "The Claude subscription notice" of `claude-assistant` calls for it, the
Claude Code card SHALL show the notice at its top; the Assistant and Access cards SHALL not
show it. It SHALL link to Anthropic's Consumer Terms and to Claude Code's legal page, and
offer a button "Hear only me" that acts as in the dialog below and a button "Hide" that
saves `claude.subscription_notice_hidden` as `true` and hides the notice for good.

#### Scenario: Notice in both cards
- **WHEN** the engine is Claude Code with a subscription and the access mode is "Everyone"
- **THEN** the Claude Code card shows the notice with both links, "Hear only me" and "Hide", and neither the Assistant nor the Access card shows it

#### Scenario: Notice gone
- **WHEN** the user picks "Only my Discord account" in the Access card
- **THEN** the notice disappears from the Claude Code card

#### Scenario: Hide
- **WHEN** the user presses "Hide" in the notice
- **THEN** the notice disappears, `claude.subscription_notice_hidden` is saved as `true`, the access mode stays as it was, and the notice does not come back after a restart or another switch to Claude Code

### Requirement: The Claude subscription dialog
The first time the user picks Claude Code in the window while its sign-in is a subscription
or not known yet and the access mode is not `owner`, a dialog SHALL say that a Claude
subscription may be used only by its holder and ask whether only the user's own voice
should reach the assistant, with "Hear only me" and "Not now". Either answer SHALL save
`claude.subscription_notice_shown` as `true`; the engine choice SHALL be saved either way.

#### Scenario: Hear only me, nobody linked
- **WHEN** the dialog is shown, no owner is linked, and the user presses "Hear only me"
- **THEN** `discord.access` is saved as `owner`, the Access card opens with a link code shown, and the card says to type `/link` with the code in Discord

#### Scenario: Hear only me, owner linked
- **WHEN** the dialog is shown, an owner is linked, and the user presses "Hear only me"
- **THEN** `discord.access` is saved as `owner` and no code is shown

#### Scenario: Not now
- **WHEN** the user presses "Not now" or closes the dialog
- **THEN** the access mode stays as it was, and the notice stays in the Claude Code card

#### Scenario: Shown once
- **WHEN** the dialog was answered before and the user picks Codex and then Claude Code again
- **THEN** no dialog opens

#### Scenario: API key
- **WHEN** Claude Code is signed in with an API key and the user picks Claude Code for the first time
- **THEN** no dialog opens

#### Scenario: Both dialogs
- **WHEN** the user picks Claude Code for the first time in the realtime mode with the Whisper files missing
- **THEN** the subscription dialog opens, no download dialog follows it, and Config → Assistant shows the note that the Whisper files are not downloaded
