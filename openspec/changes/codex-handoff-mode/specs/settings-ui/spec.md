# Spec Delta

## ADDED Requirements

### Requirement: Answer handoff control
The Codex card of the Settings tab SHALL have an "Answer handoff" list with the four modes,
the default marked as recommended, and under the list a short hint for the chosen mode
saying what it does and how it behaved in the tests. A choice SHALL be saved automatically,
like the other lists of the Settings tab.

#### Scenario: Choosing a mode
- **WHEN** the user picks "Commentary" in the "Answer handoff" list
- **THEN** the hint under the list changes to the commentary hint, the choice is saved without a button, and a restart shows it

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `codex.handoff_mode`
- **THEN** the list shows the conversation-items mode, marked as recommended, with its hint

#### Scenario: Hint for the built-in routing
- **WHEN** the user picks the built-in routing
- **THEN** the hint says the voice model retells the agent's answer in its own words and, in the tests, sometimes answered with made-up facts before the agent had finished
