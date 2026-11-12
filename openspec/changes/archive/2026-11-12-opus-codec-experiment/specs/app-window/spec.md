# Spec Delta

## ADDED Requirements

### Requirement: Codec failure notice
When the bot left a voice channel because opus-rs panicked, the window SHALL show a dialog
over whatever tab is open, titled "The bot left the voice channel". Its text SHALL name the
codec and its version, what the codec was doing (the voice toward Discord, the people in
the channel, the speech toward Codex or Codex's answer), the panic's message, and the
channel and server the bot left, and SHALL say that `/join` brings the bot back and that
libopus can be chosen in Settings → Diagnostics → Opus codec. The dialog SHALL have an
"Open log folder" button and an "OK" button that closes it. One failure SHALL show one
dialog; a second failure while it is open SHALL be added to it, not open another.

#### Scenario: The codec fails during a join
- **WHEN** opus-rs panics while the bot is in General on the test server
- **THEN** the window shows the dialog with opus-rs 0.1.42, the path, the panic's message, General and the server's name, and the hint on `/join` and libopus

#### Scenario: Open log folder
- **WHEN** the user presses "Open log folder" in the dialog
- **THEN** the log folder opens in Explorer and the dialog stays open

#### Scenario: Two failures
- **WHEN** opus-rs panics on two servers before the user closes the dialog
- **THEN** the one dialog lists both, and "OK" closes it
