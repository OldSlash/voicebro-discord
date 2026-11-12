# Spec Delta

## ADDED Requirements

### Requirement: Every panic is logged
Release builds SHALL unwind on panic. The app SHALL install a panic hook that writes every
panic to the log file with the thread, the source location and the message. A panic inside
the codec module's guarded call SHALL then unwind to the codec's handling (spec
`opus-codecs`). Any other panic SHALL, after the log line, show a Windows dialog saying
that VoiceBro stopped because of an internal error, with the message and where the log is,
and SHALL end the process at once, as a release build did before with `panic = "abort"`,
so that no part of the app runs on in a broken state.

#### Scenario: Panic outside the codec
- **WHEN** a background task of the app panics
- **THEN** the log file has the panic's thread, location and message, a Windows dialog names the error and the log folder, and the process ends when the dialog is closed

#### Scenario: Panic inside the codec
- **WHEN** opus-rs panics inside a guarded call
- **THEN** the log file has the panic's line, no dialog of this kind is shown, and the process keeps running
