# Design

## Context

See proposal.md, Why. The relevant state of `main` (`00e9cd4`):

- **Where Opus runs today.** `opus2` 0.4.0 with its default `backend-libopus`, built by
  `libopus_sys` 0.3.3 through `cmake` (the only user of `cmake` in the workspace; `cc` is
  also used by `ring` and `embed-resource`, which compile no CMake project). Users:
  - songbird 0.6.1: one `opus2::Decoder` per SSRC in `driver/tasks/udp_rx/ssrc_state.rs`
    (`DecodeMode::Decode`, 48 kHz stereo; it conceals with `decode(&[], …)` for a missed
    packet and for each missing sequence number, never uses FEC, and skips the RTP header
    extension with `RtpExtensionPacket`); the mixer's decode of our track and its own
    encode when passthrough is refused; `SoftClip` on the mix.
  - `audio/opus_out.rs`: `DiscordEncoder`, 48 kHz stereo VOIP, 128 kbit/s, complexity 10,
    in-band FEC, `set_packet_loss_perc` from the expected loss, `encode_float`; an encode
    error sends a silence packet made at the start.
  - `assistant/codex/rtc.rs` `new_encoder`: 48 kHz mono VOIP, 32 kbit/s, complexity 10,
    FEC, 5 % expected loss, on the `voicebro-rtc-send` thread.
  - `assistant/codex/jitter.rs`: 48 kHz mono decoder on songbird's mixer thread (through
    `PcmSource::read`), `decode(payload, out, fec)`, concealment with an empty payload, FEC
    from the next packet (`counters.concealed_fec`).
  - `apps/probe`: `opus-roundtrip`, `rtc`.
- **Receive.** `discord/mod.rs:326` builds songbird's config once with
  `DecodeMode::Decode(Stereo, 48 kHz)`. `discord/receive.rs` reads `decoded_voice` per
  speaker on each `VoiceTick` and mixes. In `DecodeMode::Decrypt` songbird still removes
  the transport encryption and DAVE (`udp_rx/mod.rs`, before the playout buffer), keeps the
  playout buffer, and reports a speaker on a tick with `packet: Some(RtpData)` or, for a
  missed packet, `packet: None`; `decoded_voice` is then `None`. `Driver::set_config` sends
  the new config to `udp_rx` (`driver/tasks/mod.rs`), so a decode mode can be set per call
  before `join`.
- **The candidates** (sources read from the crates.io tarballs on 2026-11-12):
  - opus-rs 0.1.42: `OpusEncoder::new(rate, channels, Application)` with public fields
    `bitrate_bps`, `complexity`, `use_cbr`, `use_inband_fec`, `packet_loss_perc`;
    `encode(&[f32], frame_size, out)` and `encode_i16`; `OpusDecoder::decode(input,
    frame_size, &mut [f32])` (input of 0 or 1 byte conceals), `decode_fec`. Decoding is f32
    only. Its changelog fixed, shortly before, a panic of the encoder with FEC at 48 kHz and
    20 to 28 kbit/s and the concealment of SILK and hybrid frames.
  - ropus 0.12.18: `Encoder::builder(rate, Channels, Application)` with `bitrate`,
    `complexity`, `inband_fec(InbandFec)`, `packet_loss_perc`, setters on the built encoder;
    `Decoder::decode(packet, &mut [i16], DecodeMode::{Normal, Fec})` and `decode_float`; an
    empty packet conceals; the frame size comes from the output buffer's length. Without the
    Xiph reference sources next to it its build script writes an empty DNN blob and prints
    a cargo warning on every build; concealment is then the classical one.
  - mousiki 0.2.1: `Encoder::builder(…)`, setters as libopus (`set_inband_fec`,
    `set_packet_loss_perc`, `set_complexity`, `set_bitrate`); `Decoder::decode` and
    `conceal`, `decode_float` and `conceal_float`. Its `LICENSE` is MIT only.
- **Settings.** `DiagnosticsSettings` has `performance`, `memory`, `verbose_log`; its doc
  says nothing in the core reads it. The Diagnostics card is in `apps/desktop/ui/index.html`.
- **Panics.** The release profile has `panic = "abort"` (chosen at the start for speed and
  size): a panic anywhere ends the process with no log line. Debug and test builds unwind.
  The app installs no panic hook.
- **After the comparison** (`docs/OPUS-CODEC-COMPARISON.md`, 2026-11-12): opus-rs matched
  libopus on every path (encode SNR to 0.1 dB, decode within about 100 dB of libopus's,
  FEC and concealment within 85 to 105 dB, the wake corpus file for file, eight live calls,
  Discord passthrough) and was two to three times faster; ropus matched it on encoding but
  concealed losses further from libopus (loudness r 0.937 against 0.970 for bursts of 3);
  mousiki garbled hybrid frames both ways. The user then kept libopus and opus-rs.

## Goals / Non-Goals

**Goals:**

- The user can switch between libopus and opus-rs in a build and hear the difference in a
  real channel.
- With `libopus` selected the app behaves exactly as today.
- The agent compares the candidates on the same inputs, with numbers, and writes down what
  it could not test (done with four codecs; the report keeps their numbers).

**Non-Goals:**

- Removing libopus or CMake, patching songbird or `opus2`, making opus-rs the default.
- Keeping ropus or mousiki as a choice.
- Switching songbird's fallback encode, decode and soft clipping.
- Neural concealment (DRED, deep PLC) for any codec.

## Decisions

### D1. Two codecs side by side, chosen at run time

`codec.rs` defines `OpusCodec { Libopus, OpusRs }` (names `libopus`, `opus-rs`) and two
types, `VoiceEncoder` and `VoiceDecoder`, each an enum over the two crates' types (enum
dispatch, no `Box<dyn>`, no allocation per frame). During the comparison the enum also held
ropus and mousiki (D7). An
`EncoderParams { channels, application, bitrate, complexity, inband_fec, loss_percent }`
builds an encoder; `VoiceEncoder` offers `encode_f32`, `encode_i16` and
`set_loss_percent`, `VoiceDecoder` offers `decode_i16` and `decode_f32` with a FEC flag and
`conceal_i16` / `conceal_f32` for a given number of samples. Where a crate lacks a format
(opus-rs decodes f32 only) the wrapper converts through a buffer it allocates once at
creation.

Rejected: the `opus2` `backend-mousiki` feature (cannot coexist with libopus, which songbird
pins, and covers only mousiki); a `[patch.crates-io]` of `opus2` (switches songbird too, but
at build time, not in the settings; it is the likely way for the later change that removes
libopus).

### D2. Same parameters, checked against each crate's defaults

Each wrapper sets every parameter the libopus path sets, and leaves the rest at what libopus
uses by default (VBR on, constrained VBR, signal auto, full bandwidth, 20 ms frames). Task
2.1 reads each crate's defaults for those and sets any that differ explicitly, so that the
comparison is of the codecs and not of their defaults.

### D3. Speakers: songbird decodes for libopus, the app for opus-rs

At each join the session sets songbird's config on the call before `join`:
`DecodeMode::Decode(Stereo, 48 kHz)` for libopus (today's path, untouched), and
`DecodeMode::Decrypt` for opus-rs. In `Decrypt` mode `ReceiveHandler` keeps a map
SSRC → `SpeakerDecoder` under its scratch lock: the codec's 48 kHz stereo decoder, the last
sequence number, and an i16 frame buffer. For each speaker on a tick:

- with a packet: take the payload as songbird does (`payload_offset..payload_end_pad`),
  skip the RTP header extension when the header's extension bit is set
  (`RtpExtensionPacket::packet_size`), conceal one frame for each sequence number missing
  since the last packet (as songbird's `missed_packets`), then decode the packet;
- without a packet: conceal one frame.

No FEC on this path, as songbird uses none, so the codecs are compared on equal terms. A
frame that is not 960 stereo samples is counted as malformed, as today. The decoder of an
SSRC is dropped when songbird reports the SSRC gone (`ClientDisconnect`) or after the same
state timeout songbird uses. A decoder is created the first time an SSRC speaks, which is the
warm-up the receive path's no-allocation rule allows.

Parity is tested, not assumed: a test feeds the same packet sequence (with a gap and a header
extension) to this path with libopus and to `opus2::Decoder` used the way songbird uses it,
and requires identical samples (spec `audio-transport`).

### D4. When a choice applies

The session reads `diagnostics.opus_codec` when it prepares a join (receive decode mode,
`DiscordEncoder`) and when the assistant opens a call (`new_encoder`, the jitter buffer's
decoder). A save changes neither a running join nor a running call; nothing restarts. The
status gets `opus_codec: { discord: Option<String>, call: Option<String> }` with the name and
version in use, and the log gets one line at each join ("Opus codec for this join: opus-rs
0.1.42") and at each call. The Diagnostics card shows those two values next to the list.

### D5. Errors are handled where they happen; an opus-rs panic ends the join

The codec module is the one isolated component: every encode and decode call of opus-rs
runs inside `std::panic::catch_unwind` (the codec state is `AssertUnwindSafe`; a panicked
codec is dropped and rebuilt with the same parameters before its next use), and nothing
else in the app catches panics. libopus calls are not wrapped: it returns errors, and C
does not unwind.

An error that is not a panic takes the existing error path of the caller:
`DiscordEncoder` sends its silence packet; the send loop toward the call sends silence;
the jitter buffer conceals; a speaker's frame is concealed. Each path counts codec failures
in an error counter of its own (the existing `encode_errors` toward Discord and
`decode_errors` of the call; new `encode_errors` toward the call and `decode_errors` of the
receive path) and logs the first failure of a spell and the count when the spell ends. The
join and the call go on.

A panic takes the same path for its frame, and is then reported, once per join, through a
`CodecFailures` sink each path gets with its guild when it is built: the guild, the path
(`discord-out`, `receive`, `call-send`, `call-receive`), the codec's label and the panic's
message. The report only sends a message and never blocks, since it comes from songbird's
mixer thread, the receive task or the call's send thread. The runtime receives it on a
task: it logs "<path>: opus-rs panicked: <message>; the bot leaves the voice channel",
leaves that guild's voice channel the way `/leave` does (the output closes, the call ends
with the assistant's detach, the panel goes), and publishes `AppEvent::CodecFailure` for
the window's notice. A second report for a guild already being left is logged and dropped.

The user's decision of 2026-11-12: no silent switch to libopus. While opus-rs is on trial
a failure must be seen; the setting stays opus-rs, and the user decides whether to `/join`
again or pick libopus. A switch to libopus would hide the failure, which is what a
production fallback might do later, not this experiment.

### D11. Release builds unwind; a panic hook keeps fail-fast everywhere else

The release profile changes from `panic = "abort"` to `"unwind"`, because an aborting
build cannot catch the codec's panic at all. To keep the rest of the app as it was, where a
bug ends the process at once rather than leaving a task dead and the app half working, the
desktop shell installs a panic hook at start:

- It writes the panic to the log: thread name, source location, message.
- `guarded()` in `codec.rs` sets a thread-local flag for the duration of its call
  (`codec::in_guarded_call()`). When it is set, the hook returns, and the panic unwinds to
  `catch_unwind` (D5).
- Otherwise it shows a Windows message box (`MessageBoxW`, `windows-sys`
  `Win32_UI_WindowsAndMessaging`): "VoiceBro stopped because of an internal error: <message>.
  The details are in the log: <log folder>", and then calls `std::process::abort()`.

The hook runs on the panicking thread, which may be songbird's mixer thread or a tokio
worker; a message box is safe there, while a dialog inside the WebView or a `/leave` would
not be. A panic in the hook itself aborts, as Rust does for a panic while panicking. Tests
and the probes do not install the hook. A debug-only switch (`VOICEBRO_TEST_PANIC=task`,
compiled only with `debug_assertions`) panics in a background task after start, to check
the hook in the real app; a release build has no such switch.

The cost, measured after the implementation: the panic strategy is one per linked binary
(Cargo refuses `panic` in a per-package profile), so unwinding applies to every crate of the
app, not only to the codec. `voicebro.exe` grows from 22.2 MB to 33.8 MB (+11.5 MB, +52 %;
the installer by 1.9 MB): cleanup funclets in `.text` (+7.3 MB), their unwind and EH tables
in `.rdata` (+3.6 MB) and `.pdata` (+0.7 MB, about 58,000 more entries). This was not
said before the decision, and it should have been. Memory and CPU after the switch were
not measured. On 2026-11-12, told the cost, the user kept unwind for the whole app for now.
The alternative that gives unwinding to the codec only: opus-rs behind a C ABI in a DLL of
its own, built with a profile that unwinds, while the app goes back to abort. A probe
(a DLL with opus-rs built with `panic = "unwind"`, a host built with `panic = "abort"`)
showed a panic caught inside the DLL with the host running on, a panic of the host ending
it with `0xC0000409` after its hook, and a DLL of 0.95 MB. It is left for a later change
(Open Questions).

### D6. The comparison

Done by the agent on the branch, the same inputs for every codec, release builds:

1. **Offline corpus** (`probe opus-compare <dir>`): the wake corpus WAVs (English, Russian
   and the eight Codex-voice languages) and the call test questions. For each codec and
   each path's parameters: µs per 20 ms frame for encode and decode (median and worst);
   decoding of every encoder's packets by libopus (errors must be zero); the decoded audio
   against the input and against libopus's own round trip (SNR and segmental SNR in dB, as
   an indicator, not a perceptual score); FEC recovery and concealment under simulated loss
   (5, 10 and 20 % random, and bursts of 2 and 3); WAV files of each for listening, under
   `target\opus-compare\`.
2. **Recognition**: the wake corpus encoded by libopus as a Discord client does, decoded by
   each codec, run through `scripts\run-wake-corpus.ps1`; the number of correct wake
   detections per codec against libopus.
3. **Benchmarks**: `output_pull_cost` and `receive_path_cost` with `VOICEBRO_OPUS_CODEC`;
   the receive bench gets a variant that decodes four speakers in the app (D3).
4. **Live calls** (`live_call_when_enabled` with `VOICEBRO_OPUS_CODEC`): the questions of
   2026-11-09 and 2026-11-10 remade with the Irina voice at 48 kHz ("remember the number
   forty-two", "which number did I ask you to remember", and delegated questions from the
   handoff report), each codec in turn. Compared: the canary and continuity checks, what the
   voice model heard (the delegated question's text and any transcript the app-server
   sends), the latency lines and the call's audio counters. With `VOICEBRO_CODEX_RECORD`
   each call is recorded, and the replay stand plays every recording through each codec's
   decoder, so the answer side is compared on identical packets.
5. **Loopback and Discord delivery**: `rtc-loopback` and `loopback_call_loses_nothing` per
   codec; `discord-delivery --opus --codec <name>` in the test guild per codec (encode
   errors, passthrough, Discord's loss reports).
6. **Size**: `voicebro.exe` of a `-Trial` release build with and without the three codecs.

The report `docs/OPUS-CODEC-COMPARISON.md` gives the numbers per codec, the WAV paths to
listen to, what was not tested (the real channel with two people and a game is the user's
step), and a recommendation. It is committed with the change. All of this ran with the four
candidates and stays in the report as measured; it gets a section with the user's decision.
From then on `opus-compare`, the tests and the benchmarks run libopus and opus-rs, and
`opus-compare` is the check an opus-rs update has to pass against the report's numbers.

### D7. ropus and mousiki removed after the comparison

The user kept libopus and opus-rs on 2026-11-12. Removed again: the `ropus` and `mousiki`
dependencies and their `Cargo.lock` entries; `[profile.dev.package.mousiki]` in the
workspace manifest (it only built mousiki like the release for the tests); the two enum
variants and their code in `codec.rs`, with `OpusCodec::hybrid_encoder_is_broken` and the
tests' special cases for mousiki; the two options in the window and the mock; the `$ports`
block of `scripts\third-party-notices.ps1`, which added the Opus text that mousiki did not
ship (opus-rs ships it as its `COPYING`); the mentions in `AGENTS.md`, `ARCHITECTURE.md`,
`LOCAL-TESTING.md` and the user guide, except where they report what was measured. The
report keeps every number of the four.

### D8. Version pinned

`opus-rs = "=0.1.42"`. opus-rs moves fast; an update is a decision with its own comparison
run, not a side effect of `cargo update`.

### D9. How a build reaches the user

The project hands over only release builds from `main` at a tag. So the switch reaches the
user when this change is merged and a version is tagged. With `libopus` the default and
today's paths untouched (D3, D4), merging changes nothing for anyone who does not touch the
setting. The agent's own runs use `-Trial` builds from the branch; the user ran one too,
before this change was merged, to test opus-rs in a channel.

### D10. A stored codec this build does not have

The trial build of the four-codec stage could save `ropus` or `mousiki`. Loading such a
file must not leave the user with a settings file that every later save rejects. So the
settings load maps an `opus_codec` that names no codec of this build to `libopus` before
validation, and logs "the stored Opus codec <value> is not in this build; libopus is used";
the next save writes `libopus`. A save that sends an unknown name is still rejected
(spec `app-settings`). The check is by name against `OpusCodec::parse`, not a list of the
removed names, so it covers any future removal as well.

## Risks / Trade-offs

- [opus-rs panics on a real stream] → D5 and D11: the app keeps running, the bot leaves
  that channel, the window and the log say why; libopus stays the default.
- [One panic takes the bot out of the channel] → chosen (D5): a failure of the codec on
  trial must be seen; `/join` brings the bot back.
- [Unwinding makes the release binary bigger and changes how every panic ends] → measured:
  `voicebro.exe` +11.5 MB (+52 %), the installer +1.9 MB, because unwinding covers every
  crate (D11); kept by the user for now. The hook (D11) keeps every panic outside the codec
  ending the process at once, now with a log line and a dialog.
- [A panic hook runs on any thread, also songbird's mixer] → it only logs, shows a message
  box and aborts; it touches neither the WebView nor the async runtime.
- [opus-rs is young (0.1.x, released every few days) and one 18 ms encode was seen once] →
  the version is pinned (D8), an update needs an `opus-compare` run against the report, and
  libopus stays selectable.
- [The app's own decoding of speakers differs from songbird's] → the parity test of D3 with
  libopus; with libopus selected the old path is used anyway.
- [Decoding speakers now runs in the tick handler] → it is bounded work per speaker per tick;
  the receive bench measured it (402 µs for four speakers with libopus, 148 µs with opus-rs).
- [Binary size grows with opus-rs] → the trial build measures it after the removal.
- [Objective quality numbers for perceptual codecs are weak] → SNR is shown as an indicator
  only; recognition by Vosk, what the Codex voice heard, and listening decide.

## Migration Plan

`diagnostics.opus_codec` is added with a serde default; an older settings file loads with
`libopus` and the next save writes the field. No schema version change, as for the other
diagnostics fields. A file that names a codec this build does not have loads with libopus
(D10).

## Open Questions

- Whether opus-rs replaces libopus: the user's decision after the real-channel test; a
  follow-up change does it, with songbird moved off libopus and CMake out of the build.
- Whether opus-rs moves into a DLL of its own that unwinds, with the app back on abort
  (D11): about 1 MB instead of 11.5 MB, at the price of a second build step, a DLL shipped
  next to the exe and an `unsafe` C ABI layer. The user's decision.
