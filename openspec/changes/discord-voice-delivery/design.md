# Design

## Context

See proposal.md for the measurements.

**Today:**
- `discord/session.rs` hands songbird `RawAdapter::new(PcmSource, 48 kHz, 2 ch)`, and
  `call.play_only_input` plays it as the guild's only track.
- `PcmSource::read` builds each 20 ms of f32 stereo:
  - the call's `FrameProvider` (the Codex playout) or the output queue;
  - the output gain;
  - mute;
  - earcons.
- songbird decodes nothing for this track and encodes the mix with its own
  `opus2::Encoder::new(48 kHz, stereo, Application::Audio)` at `DEFAULT_BITRATE` (128 kbit/s),
  with no `set_inband_fec` and no `set_packet_loss_perc`.

**What songbird offers:**
- **Passthrough** (`driver/tasks/mixer/mix_logic.rs`): when the only live track's codec is
  Opus with 20 ms frames, songbird copies the packet into the RTP payload instead of mixing
  and encoding. DAVE encryption is applied to the payload afterwards in either case.
- **Connection info:** `Call::current_connection()` gives the voice server `endpoint`. It also
  holds the session id and token, which must never be logged.
- **Our SSRC:** `CoreEvent::DriverConnect` gives it.
- **RTCP:** `CoreEvent::RtcpPacket` (feature `receive`, already on) gives each decrypted RTCP
  packet as `discortp::rtcp::RtcpPacket`.

Whether Discord sends receiver reports about the bot's own stream is unknown: no
documentation, no probe yet.

The project rules: only necessary allocation after warm-up on the audio paths, no panics
(release is `panic = "abort"`), a read on the mixer thread never blocks for more than a few
ms. Behaviour comes from measurement, not from fixed time constants.

## Goals / Non-Goals

**Goals:**
- a lost packet toward a listener can be rebuilt from the next one;
- the protection follows what the Discord leg measures;
- the leg is visible in the log and the status.

**Non-Goals:**
- changing the channel's region;
- Opus DTX;
- adaptive bitrate;
- the receive path (people → bot);
- the OpenAI-leg regulator (a separate step of `codex-adaptive-playout`).

## Decisions

**D1. The output becomes an Opus source that songbird passes through.**

A new `OpusOutput` implements symphonia's `FormatReader` with one Opus track (48 kHz,
stereo, 960-sample frames).
- **`next_packet`:** fills one 20 ms PCM frame through the existing `PcmSource` logic, so
  playout, gain, mute and earcons stay exactly where they are. It then encodes the frame with
  our encoder and returns the packet.
- **songbird input:** gets it as a `LiveInput::Parsed` with that format and an Opus decoder,
  which songbird uses only if passthrough is refused.

**Alternatives:**
- **Forking songbird** to set FEC on its encoder: a maintained fork for one call.
- **Sending RTP ourselves** around songbird: it would bypass songbird's DAVE encryption,
  which is mandatory.
- **Forwarding the OpenAI packets untouched**, as Discodex does: no playout, gain or earcons
  would be possible, and other engines produce PCM.

**D2. The encoder carries FEC in a mode that can carry it.**
- **Settings:** `opus2::Encoder` at 48 kHz stereo (the mono voice is duplicated, as today),
  `Application::Voip`, `set_inband_fec(true)`, `set_packet_loss_perc(expected loss)` (D3),
  complexity 10.
- **Bitrate:** Opus carries FEC (LBRR) only in its SILK and hybrid modes. The bitrate is the
  highest at which the encoder still chooses such a mode for our voice; a test finds and
  guards it.
- **The test:** drop one packet from an encoded speech-like signal and decode it from the next
  packet's FEC. Then compare with plain concealment: the rebuilt frame must be much closer to
  the original. The incoming Codex voice is super-wideband at about 50 kbit/s, so this loses
  nothing audible against 128 kbit/s CELT.

**D3. The expected loss follows the Discord leg.**
- **Reports:** RTCP sender and receiver reports carry report blocks. A block whose SSRC is our
  SSRC (from `DriverConnect`) gives the fraction lost since the last report and the
  interarrival jitter (in 48 kHz units, so /48 → ms).
- **Expected loss:** an upper envelope of the reported loss. It rises at once to a new higher
  report and decays toward the newer reports with a forgetting factor per report, so a single
  quiet report does not drop the protection. The envelope, rounded to a whole percent, is the
  encoder's `packet_loss_perc`.
- **No reports:** with none about our stream (or none for a while, by the same forgetting),
  the encoder uses a cautious default, and the source shows as "default".

The probe (task 2) decides which of the two is real for Discord today. Nothing else in the
design depends on the answer.

**D4. Pass-through is observed, not assumed.**

songbird reports nothing when it refuses passthrough. Its fallback, though, decodes our
packets with the decoder we hand it. That decoder is a thin wrapper that counts calls:
- packets encoded minus packets decoded by songbird = packets passed through;
- any decode while playing means passthrough was refused, and is logged once per spell.

**D5. The leg is shown.**
- **At the join:** the endpoint's host (no port, no token) and the region code read from it
  go to the log and to `VoiceChannelStatus`. Hosts come in two forms: `finland14057` gives
  `finland`, and `c-sin19-5f0d7996` (seen for Singapore) gives `sin`. The code is the letters
  of the first dash-separated part that is letters then a digit, else the label's leading
  letters.
- **Counters** (`DeliveryCounters` in `VoiceChannelStatus`):
  - packets encoded and passed through;
  - expected loss and its source;
  - last reported loss and jitter, and the number of reports.
- **At the leave:** one log line with the same values. They are also stat cards on the
  Performance tab.

**D6. Cost and safety on the mixer thread.**
- **Cost:** one PCM fill and one Opus encode per 20 ms. songbird already did one encode per
  20 ms and no longer does. A release benchmark checks it against the existing pull path
  figure. Measured (task 6.1): our VOIP encode with FEC costs about 2.2 times songbird's CELT
  encode (mean 0.64 ms against 0.30 ms per 20 ms, worst 1.4 ms), so the mixer thread's encode
  cost roughly doubles. That is still within its "a few ms" rule.
- **Memory:** the encoder's output buffer is allocated once. Each packet handed to songbird
  is copied into its `Packet`, a necessary allocation under the rule.
- **Errors:** an encode error is counted and replaced by an encoded silence packet. Nothing
  panics.

## Risks / Trade-offs

- **[Discord does not report on our stream]** → the default protection still covers single
  losses, and the status says it is the default.
- **[songbird refuses passthrough]** (for example a second track, or a future songbird
  change) → it decodes and re-encodes without FEC, as today. D4 makes it visible, and a test
  covers the conditions.
- **[The listener's client does not use Opus FEC]** → the extra bits are wasted, nothing
  else. Hearing the difference in a far region is the user's check.
- **[VOIP mode sounds different from CELT at 128 kbit/s]** → the source is super-wideband
  speech at about 50 kbit/s, and the comparison WAV of task 3 is listened to before a build is
  handed over.
- **[DAVE with passthrough]** → songbird encrypts after the payload is chosen. The probe
  checks the bot stays audible in a DAVE call.

## Migration Plan

No settings or stored data change. Rollback is reverting the change's commits. The first
build carries the build stamp. The user compares a far region against a near one in a real
channel.

## Open Questions

- What Discord's RTCP holds for our stream: decided by the probe of task 2, without changing
  the design.
