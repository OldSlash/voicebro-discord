## ADDED Requirements

### Requirement: The ChatGPT account notice in the window
While requirement "The ChatGPT account notice" of `codex-assistant` calls for it, the Codex card SHALL show the notice at its top and no other card SHALL show it. It SHALL link to OpenAI's Terms of Use and Service Terms and offer "Hear only me", which acts as the Claude notice's button does, and "Hide", which saves `codex.account_notice_hidden` as `true`. No dialog SHALL open for it.

#### Scenario: Notice in the Codex card
- **WHEN** the engine is Codex with a ChatGPT account and the access mode is "Everyone"
- **THEN** the Codex card shows the notice with both links, "Hear only me" and "Hide", and neither the Assistant nor the Access card shows it

#### Scenario: Hear only me
- **WHEN** the user presses "Hear only me" in the notice and no Discord account is linked
- **THEN** the access mode is saved as "Only my Discord account", the notice disappears, and the Access card opens with a link code

#### Scenario: Notice gone
- **WHEN** the user picks "Only my Discord account" in the Access card
- **THEN** the notice disappears from the Codex card

#### Scenario: Hide
- **WHEN** the user presses "Hide" in the notice
- **THEN** the notice disappears, `codex.account_notice_hidden` is saved as `true`, the access mode stays as it was, and the notice does not come back after a restart or another switch to Codex

#### Scenario: A refused save
- **WHEN** the user presses "Hide" and the save is refused
- **THEN** the notice stays with the reason under it

#### Scenario: No dialog
- **WHEN** the user picks Codex in the Engine list, signs in to Codex or presses Check Codex while the notice applies
- **THEN** no dialog opens and the notice shows in the Codex card
