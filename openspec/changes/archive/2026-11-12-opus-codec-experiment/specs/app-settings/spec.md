# Spec Delta

## ADDED Requirements

### Requirement: Opus codec setting
The `diagnostics` section SHALL hold `opus_codec`, a string that is `libopus` or `opus-rs`,
with the default `libopus`. A settings file without the field SHALL load with `libopus`,
and the next save SHALL write it. A saved document with any other value SHALL be rejected
with an error that lists the two values, and the stored settings SHALL stay unchanged. A
save that changes only this field SHALL restart nothing.

#### Scenario: Older file
- **WHEN** the settings file has a `diagnostics` section without `opus_codec`
- **THEN** the codec is `libopus` and the next save writes `"opus_codec": "libopus"`

#### Scenario: Valid change
- **WHEN** the user saves `diagnostics.opus_codec` as `opus-rs`
- **THEN** the save succeeds, the stored settings hold it, and the Discord connection, wake engine, assistant and any running call keep running untouched

#### Scenario: Unknown codec
- **WHEN** a saved document sets `diagnostics.opus_codec` to `opus2`
- **THEN** the save is rejected with an error naming `libopus` and `opus-rs`, and the previous settings stay in effect

### Requirement: A stored codec this build does not have loads as libopus
A settings file whose `diagnostics.opus_codec` names a codec this build does not have, such
as `ropus` or `mousiki` from a trial build of the four-codec comparison, SHALL load with
`libopus` and a log line naming the value it replaced, and the next save SHALL write
`libopus`. Such a file SHALL NOT make the load or a later save fail.

#### Scenario: ropus from a trial build
- **WHEN** the app starts with a settings file that has `"opus_codec": "ropus"`
- **THEN** the codec is libopus, the log says the stored codec ropus is not in this build and libopus is used, and saving any other setting succeeds and writes `"opus_codec": "libopus"`
