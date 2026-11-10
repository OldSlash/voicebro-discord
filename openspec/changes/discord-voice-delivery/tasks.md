# Tasks

## 1. Branch and chain

- [x] 1.1 Create the branch `change/discord-voice-delivery` from the tip of `change/codex-adaptive-playout` and add the change to the end of the chain in `AGENTS.md`. Verify: `git log -1` shows `70407a7` as the parent, and the chain lists the change last.

## 2. What Discord tells us (design D3, D5)

- [x] 2.1 Add a `discord-delivery` probe to `apps/probe`. For 30 s it joins the test channel of `LOCAL-TESTING.md` and plays a tone through the current output. It logs:
  - the voice server host and region code (never the token or session id);
  - our SSRC from `DriverConnect`;
  - every RTCP packet: type, sender SSRC, report blocks with their SSRC, fraction lost and jitter.

  Document it in `AGENTS.md`. Verify: one run's output is noted under this task. It says whether Discord sends report blocks about our SSRC, and how often.

  2026-11-10, 16:35, test channel `General`, 30 s of a tone through the current path:
  - voice server `finland14009.discord.media`, our SSRC 1510;
  - RTCP: 30 receiver reports (201), one per second, the first after 0.83 s, all with a report block about our SSRC; and 40 transport feedback packets (205), likely NACKs for lost packets, which songbird does not answer;
  - our stream's loss toward Discord: 3.9 % in the last report and 12.5 % in the worst second; jitter 1.2 ms.

  An earlier 40 s run (`finland14005`) gave 40 reports, the worst at 10.2 %.

  **So Discord does report on our stream**, every second, and design D3's measured path is the real one; the default is only for the first second and for silence from Discord.

  The probe also had to cut each packet at songbird's `payload_end_pad`. Read to the end, it parsed padding as more RTCP.

## 3. Our encoder and passthrough (spec: The voice toward Discord is encoded with loss protection; design D1, D2, D4)

- [x] 3.1 Add the encoder: 48 kHz stereo, `Application::Voip`, in-band FEC, complexity 10, `packet_loss_perc` settable, an output buffer allocated once. The bitrate is the highest that keeps a FEC-capable mode. Verify by a unit test: on a speech-like signal, drop one packet and decode it from the next packet's FEC; the rebuilt frame is much closer to the original than plain concealment. Note the bitrate chosen and the measured difference under this task.

  `audio/opus_out.rs`, `DiscordEncoder`. Bitrate 128 kbit/s, songbird's own: with VOIP, FEC on and an expected loss above zero, libopus stays in SILK or hybrid (the modes that carry FEC) at this rate, so nothing lower is needed. On the speech-like test signal: 303 bytes per packet at 10 % expected loss; the lost frame rebuilt from the next packet's FEC is 42.9 dB closer to the original than concealment.
- [x] 3.2 Add `OpusOutput`: a symphonia `FormatReader` with one 20 ms Opus track. It fills each frame through the existing `PcmSource` logic (playout, gain, mute, earcons) and encodes it. Hand it to songbird as a `LiveInput::Parsed` with a counting Opus decoder, replacing `RawAdapter` in `discord/session.rs`. Verify by unit tests:
  - packets read through songbird's parsed input are byte for byte what the encoder made;
  - decoding the stream gives the same PCM as `PcmSource` within the codec's error;
  - the existing output tests (priming, flush, mute with earcons, gain, partial reads) still pass on the PCM side;
  - an encode error gives a counted silence packet.

  `OpusOutput` and `live_input` in `audio/opus_out.rs`; `PcmSource::fill_frame` gives it one frame through the same logic as the PCM read. The track is described by probing a minimal DCA header in memory (songbird's Opus container), since symphonia's probed metadata has no public constructor. `discord/session.rs` plays it and falls back to `RawAdapter` with a warning if the track cannot be built. Tests: `songbird_reads_byte_for_byte_what_the_encoder_made` (and a closed output ends the track); `the_output_plays_as_opus_packets_songbird_can_pass_through` (VOIP keeps no waveform, so the decoded stream is compared by its loudness per 20 ms: correlation 0.985, level −0.0 dB); `a_frame_that_cannot_be_encoded_becomes_counted_silence`; all 10 existing `audio::output` tests pass unchanged.
- [x] 3.3 Observe passthrough (design D4): packets encoded minus decoded by songbird = passed through, and a decode while playing is logged once per spell. Verify:
  - a unit test of the counting;
  - a probe run (2.1 switched to `OpusOutput`) shows every packet passed through and the bot still heard in the channel with DAVE.

  Unit test: `songbird_decoding_our_packets_counts_as_passthrough_refused`. Probe: `discord-delivery 30 --opus` pushes a tone into `output_pair` on a 20 ms thread and plays it through `live_input`. 2026-11-10, test channel `General`, `finland14005`, DAVE session active: 1502 packets encoded, 1502 passed through, 0 decoded by songbird, 0 encode errors; 30 receiver reports about our SSRC, loss last 2.0 % and worst 9.8 %, jitter 0.5 ms. Hearing was not tested in that run.

  The user's runs on 2026-11-10, with the user in the channel listening (DAVE active), 1502 of 1502 packets passed through in both:
  - **far server** `finland14057` (the user's ping about 300–380 ms): the tone was heard, but broken up all the time. Discord reported up to 18.8 % of our packets lost in a second and sent 62 feedback packets (205, likely NACKs). The expected loss followed it: 9 → 13 → 7 → 19 → 5 %.
  - **near server** `c-sin19-5f0d7996` (Singapore, ping about 30 ms): the tone was clean. 0.0 % loss in every report and no feedback packets. The expected loss went from 9 down to 1 %.

  So the tone, generated locally with no OpenAI leg at all, breaks up on the far server. The loss happens after we send, on the way to Discord's server. One-packet FEC does not cover losses at that level. The follow-up is in `docs/NEXT-CHANGES.md`. The user accepted the near-server result for this change.

  The second host's form showed that the region parse read `c-sin19-…` as `c`. It now reads `sin`, with a test.

## 4. Protection from the measurement (spec: Protection follows the measured loss; design D3)

- [x] 4.1 Parse RTCP report blocks for our SSRC and keep the expected loss as a decaying upper envelope of the reported loss (a forgetting factor per report), the default when nothing is reported. Feed it to the encoder's `packet_loss_perc`. Verify by unit tests with synthetic reports:
  - rising loss raises the setting at once;
  - falling loss lowers it gradually;
  - reports about other SSRCs are ignored;
  - no reports keep the default, with the source "default".

  `discord/delivery.rs` (`for_each_report_block`, `DeliveryLeg`, `DeliveryHandler` registered before the join) and `DeliveryShared::on_report` / `expected_loss_now` in `audio/opus_out.rs`. The forgetting is counted per packet we send, not per report, so it does not depend on how often Discord reports. It has the memory of the playout's depth estimator (0.9983 per packet, about 12 s). The envelope starts from the default. Without new reports it returns toward the default by the same forgetting, and counts as measured while the last report still outweighs it. Tests: the four above plus `without_new_reports_the_setting_returns_to_the_default` and `report_blocks_are_read_from_sender_and_receiver_reports` (padding and cut packets). Live, `discord-delivery 40 --opus` on 2026-11-10 (`finland14098`, worst report 5.9 %): the setting went 9 → 3–5 following the reports, back to 6 after the lossy second; 2002 of 2002 packets passed through.

## 5. Visible leg (spec: The Discord leg is visible; design D5)

- [x] 5.1 Log the voice server host and region at the join and put them in `VoiceChannelStatus`. Add `DeliveryCounters` (packets encoded and passed through, expected loss and source, last reported loss and jitter, reports) to the status and one log line at the leave. Verify:
  - a unit test of the region code from sample hosts and of the leave line;
  - a test that the token and session id never appear in what is logged.

  `VoiceChannelStatus.delivery: DeliveryCounters` (with `voice_server` and `region`), filled on every status read. Log lines `[delivery:<guild>] voice server … (region …)` at the join and the same with the counters at the leave. Tests: `the_region_is_read_from_the_hosts_first_label`, `the_join_and_leave_lines_name_the_server_and_the_counters`, `the_token_and_session_id_never_reach_what_is_logged` (also checks the status JSON).
- [x] 5.2 Add the counters as Performance tab stat cards in `main.js` and the `ui-dev` mock. Verify in the mock: the cards show numbers and the source text.

  Nine cards after the voice buffer ones: Discord voice server (the host's first label, which fits a card), Discord region, Packets to Discord, Passed through with FEC, Encode errors, Expected loss, % (with "measured" or "default"), Reports from Discord, Reported loss, %, Reported jitter, ms (a dash before the first report). The Status tab's counters JSON gains `delivery <guild>`. Checked in the mock on 2026-11-10: `finland14098`, `finland`, 279 / 279, `4 (measured)`, 5 reports, 0.4 %, 1.4 ms; no console errors; the server card does not overflow.
- [x] 5.3 Update `docs/ARCHITECTURE.md`: the output path (Opus source, passthrough), the thread table (encode on the mixer thread), the counters, the contract table. Verify: the documented settings match the code.

  Updated: the layout, the thread table (the mixer encodes once per 20 ms as before; RTCP on the event task writes atomics only), 4.1 (Opus output, encoder, passthrough, expected loss), 4.2 (the track, the fallback, the leg's handler, region, status), the audio-out flow, the contract row "Opus toward Discord", and observability (the counters and both log lines). Checked against the code: VOIP, 128 kbit/s, complexity 10, FEC, default 10 %, forgetting 0.9983 per packet, measured while the last report weighs at least half.

## 6. Checks and hand-over

- [x] 6.1 Add a release benchmark of one output pull with the encode (`VOICEBRO_BENCH=1`). Verify: the cost per 20 ms is reported, and the songbird encode it replaces is no longer done (passthrough count).

  `output_pull_cost_when_enabled` in `audio/opus_out.rs`, 3000 speech-like frames, release, 2026-11-10: ours (PCM fill + VOIP FEC encode) mean 640.7 µs, p99 981.6 µs, max 1402.4 µs per 20 ms; before (PCM fill + songbird's `Application::Audio` encode at 128 kbit/s) mean 296.1 µs, p99 460.9 µs, max 678.7 µs; the fill alone 1.6 µs. 3000 encoded, 0 decoded by songbird, so songbird's own encode is not done. **Deviation from design D6:** the net cost is not unchanged. Our encode costs about 2.2 times songbird's (SILK with LBRR at complexity 10 against CELT): about 3 % of one core instead of 1.5 %. That is still well under a ms on average and within the mixer's "a few ms". Lowering the complexity is the user's call.
- [x] 6.2 Verify: `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` all pass; report their output.

  2026-11-10: `cargo fmt --check` clean. `cargo clippy --all-targets`: no errors; 6 warnings, all in code this change does not touch (`discord/receive.rs` let-else, `secrets.rs`, an `audio/output.rs` test, the probe's `credential.rs`, `opus.rs`, `rtc.rs`: `chunks_exact` with a constant size); this change's own were fixed with `as_chunks`. `cargo test`: voicebro-core 179 passed, desktop 6 passed, 0 failed. The live Codex and corpus tests are gated by their variables and did not run.
- [x] 6.3 Build the release `voicebro.exe` at the change tip and hand it over. Verify: the build stamp names the commit.

  Closed by the user's decision of 2026-11-10 ("consider every remaining check done"); no release build was made at this tip. The user listened to the Opus output through the probe (3.3).

## Workflow follow-up

- Checkpoint commits on `change/discord-voice-delivery` with the user's OK.
- The user compares the voice in a far region (India) and a near one (Singapore) in a real
  channel with people and a game. The counters of each call are read from the log.
- Merged into `main` with `--no-ff --log` in chain order once the app works end to end and
  the user says so; the change is archived in the same step.
