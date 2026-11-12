# opus-codecs Specification

## Purpose

Lets the voice be encoded and decoded by one of two Opus codecs, the C library libopus or
the pure-Rust opus-rs, chosen in the settings, so that opus-rs can be tried as libopus's
replacement in real use, and makes a failure of opus-rs visible instead of hiding it.

## Requirements

### Requirement: Two Opus codecs behind one interface
The app SHALL build in two Opus codecs, libopus (through `opus2`) and opus-rs, and SHALL
reach both through the same encoder and decoder types, so that a path that encodes or
decodes does not depend on which codec runs. Both SHALL be configured with the same
parameters for the same path: sample rate, channels, application, bitrate, complexity,
in-band FEC and the expected packet loss. No other Opus codec SHALL be built in.

#### Scenario: Same parameters
- **WHEN** the encoder toward Discord is built with opus-rs
- **THEN** it encodes 48 kHz stereo VOIP at 128 kbit/s, complexity 10, in-band FEC on, with the expected loss from Discord's reports, as the libopus encoder does

#### Scenario: Codec named
- **WHEN** either codec is built
- **THEN** it reports its name and version: `libopus 1.5` or `opus-rs 0.1.42`

### Requirement: The selected codec runs on four paths
The codec selected in `diagnostics.opus_codec` SHALL encode the assistant's voice toward
Discord, encode the speech sent to the Codex call, decode the Codex call's answer with its
FEC and concealment, and decode the people speaking in the Discord channel. songbird's own
fallback when it refuses passthrough (decoding our packets, mixing, soft clipping and
encoding again) SHALL stay libopus whatever is selected.

#### Scenario: libopus selected
- **WHEN** `diagnostics.opus_codec` is `libopus`
- **THEN** every path encodes and decodes as before this change, and songbird decodes the people in the channel itself

#### Scenario: opus-rs selected
- **WHEN** `diagnostics.opus_codec` is `opus-rs` and the bot joins a channel and a call starts
- **THEN** the voice toward Discord, the speech toward the call, the call's answer and the people in the channel are all encoded or decoded by opus-rs

### Requirement: A codec choice takes effect at the next join and the next call
A saved codec choice SHALL take effect for the Discord side at the next join of a voice
channel and for the Codex side at the next call that opens. Saving it SHALL restart
nothing: a running join and a running call keep the codec they started with. The status
SHALL carry the codec in use on the Discord side and on the call, and the log SHALL say
which codec a join and a call use when they start.

#### Scenario: Changed while in a channel
- **WHEN** the bot is in a voice channel with libopus and the user selects opus-rs
- **THEN** the join keeps libopus until `/leave`, the status still names libopus for it, and the next `/join` uses opus-rs

#### Scenario: Changed between calls
- **WHEN** the user selects opus-rs while no call is open and then a session starts
- **THEN** the new call encodes and decodes with opus-rs and the log names opus-rs for that call

### Requirement: opus-rs packets decode with libopus
The packets of the opus-rs encoder SHALL decode with libopus without an error, in every
mode the app uses (mono and stereo, SILK, hybrid and CELT frames, FEC on), because Discord's
clients and the Codex call decode them with libopus or WebRTC's Opus.

#### Scenario: Cross-decoding
- **WHEN** a speech sample is encoded by opus-rs with the parameters toward Discord and toward the call
- **THEN** libopus decodes every packet without an error to the frame length the packet states; how close the decoded audio is to the sample is measured by the comparison, not required here

#### Scenario: libopus packets decoded by opus-rs
- **WHEN** packets made by libopus with the parameters of a Discord client and of the call are decoded by opus-rs
- **THEN** opus-rs decodes every packet to the frame length the packet states

### Requirement: FEC and concealment with both codecs
Both codecs' decoders SHALL rebuild a lost 20 ms packet from the in-band FEC of the next
packet when that packet carries it, and SHALL conceal a lost packet when no FEC is there,
returning a full frame in both cases.

#### Scenario: One packet lost
- **WHEN** one packet of a FEC-protected stream is lost and the next arrives, for each of the two codecs
- **THEN** the decoder rebuilds the lost frame from the next packet's FEC and counts it as recovered by FEC

#### Scenario: Two packets lost
- **WHEN** two packets in a row are lost
- **THEN** the first is concealed and the second rebuilt from the following packet's FEC, and every call returns a full frame

### Requirement: A codec error is handled where it happens
An encode or decode error of either codec that is not a panic SHALL be counted and logged
once per spell of failures, and the frame SHALL be replaced as on an encode or decode error
today: a silence packet toward Discord and toward the call, a concealed frame from the call
and from a speaker. The join and the call SHALL go on.

#### Scenario: Decoder error from a speaker
- **WHEN** a speaker's packet makes the selected decoder fail without a panic
- **THEN** that speaker's frame is concealed, the other speakers are mixed as usual, the failure is counted, and the bot stays in the channel

### Requirement: A panic in opus-rs ends the join honestly
A panic inside opus-rs SHALL be caught at the codec module's boundary on the thread that
called it, so that thread and the app keep running, and SHALL be logged with its message,
the codec, the path (the voice toward Discord, the people in the channel, the speech toward
the call, the call's answer) and the server. The frame SHALL be replaced as on an error.
The bot SHALL then leave that server's voice channel as `/leave` does, once per failed join
however many panics follow, and the window SHALL show the codec failure notice. The codec
SHALL NOT be swapped silently: `diagnostics.opus_codec` stays as it is, and the next `/join`
SHALL work.

#### Scenario: Panic while encoding toward Discord
- **WHEN** the opus-rs encoder panics on a frame of the assistant's voice
- **THEN** songbird's mixer thread keeps running, a silence packet is sent for that frame, the log names opus-rs, the path, the server and the panic's message, the bot leaves the voice channel, and the window shows the notice

#### Scenario: Panic in the call's decoder
- **WHEN** the opus-rs decoder of a Codex call panics on a packet of the answer
- **THEN** that frame is concealed, the bot leaves that server's voice channel (which ends the call), the window shows the notice, and the app keeps running

#### Scenario: After the failure
- **WHEN** the bot left the channel because opus-rs panicked and the user runs `/join`
- **THEN** the bot joins, `diagnostics.opus_codec` is still opus-rs, and the join uses it unless the user picked libopus in the meantime
