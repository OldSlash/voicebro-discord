# Spec Delta

## ADDED Requirements

### Requirement: The Opus port carries the Opus license
The notices SHALL list opus-rs, the Opus port built into `voicebro.exe`, with the Opus
license text and the Xiph.Org copyright list, because its code is derived from the Opus
reference code. opus-rs ships that text as its own `COPYING`, which the notices take like
any crate's license file.

#### Scenario: opus-rs
- **WHEN** the notices of a build with opus-rs are read
- **THEN** opus-rs is listed with its `COPYING`, the Opus BSD-3-Clause license with the Xiph.Org copyright list

#### Scenario: No other port
- **WHEN** the notices of this change's build are read
- **THEN** they list no ropus or mousiki and no separate Opus text for a port
