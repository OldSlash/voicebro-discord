# Opus codecs compared: libopus and three pure-Rust ports

Change `opus-codec-experiment`, measured on 2026-11-12 on the development PC (Windows 11,
release builds unless a line says otherwise). The question: can a pure-Rust Opus replace
libopus, the C library `libopus_sys` builds with CMake, without the voice getting worse?

The codecs:

| Codec | Crate | What it is | License |
|---|---|---|---|
| libopus 1.5 | `opus2` 0.4.0 → `libopus_sys` 0.3.3 | the reference C library, built from source with CMake | BSD-3-Clause |
| opus-rs 0.1.42 | `opus-rs` | a port of libopus 1.6, float; actively developed (released 2026-11-10) | BSD-3-Clause |
| ropus 0.12.18 | `ropus` | a port of the fixed-point libopus, "bit-exact against the reference" | BSD-3-Clause |
| mousiki 0.2.1 | `mousiki` | a port of the reference opus-c, float; its GitHub repository answered 404 on 2026-11-12 | MIT (code derived from BSD-3-Clause Opus) |

For these measurements all four were built into the app side by side and chosen in
Settings → Diagnostics → Opus codec (`diagnostics.opus_codec`, default libopus). The
choice covers the app's four Opus paths: our speech toward Codex, the assistant's voice
toward Discord, the Codex answer and the people in the channel. songbird's fallback when it
refuses passthrough stays libopus.

## Decision

On 2026-11-12, after this report, the user kept two codecs: **libopus** (the default) and
**opus-rs**. ropus and mousiki were taken out of the build and the settings again: ropus
because its concealment under loss is measurably further from libopus's, mousiki because it
garbles hybrid frames and its repository is gone. A settings file that still names one of
them loads with libopus. The numbers below stay as measured with the four; they are the
reference an opus-rs update is checked against with `probe opus-compare`. Whether opus-rs
replaces libopus, and with it CMake leaves the build, is the user's decision after the test
in a real channel.

The same day the user decided how a panic of opus-rs is handled (step 2 of the
recommendation's next steps). Release builds unwind (`panic = "unwind"`). A panic inside
opus-rs is caught at the codec's boundary as before, logged with its message, and ends that
server's join: the bot leaves the voice channel as with `/leave`, and the window shows a
notice that names the server, the channel, the codec and the error. The codec is not
swapped for libopus behind the user's back, and the setting stays as it is; the user runs
`/join` again or chooses libopus. Any other panic is logged and ends the app after a message
box, as `panic = "abort"` did (`docs/ARCHITECTURE.md`, 4.1, 4.7 and 4.8).

Unwinding cannot be limited to one crate (the panic strategy is one per linked binary), so
it costs the whole app: `voicebro.exe` +11.5 MB (see "Size"). Told the cost, the user kept
it for now. Unwinding for the codec alone would need opus-rs in a DLL of its own; a probe
showed that it works (a 0.95 MB DLL), and it is left for a later decision.

## Findings in short

- **opus-rs** is indistinguishable from libopus on all four paths: the same SNR to a tenth
  of a dB when it encodes, a decode that matches libopus's to about 100 dB, FEC and
  concealment whose output is 85 to 105 dB from libopus's own (that is, the same signal)
  under every loss condition. It encodes about 2 times and decodes about 3 times faster than this
  build of libopus. One encode of 18 ms was seen (see Speed).
- **ropus** encodes as well as libopus and is the fastest encoder, but its concealment
  differs from libopus's: under 10 % loss and in bursts its output is only 15 to 46 dB
  from libopus's, and the loudness under burst loss tracks the speech less well (0.937 to
  0.964 against 0.970 to 0.986). Without the Xiph reference sources next to it, its build
  embeds no neural-PLC weights.
- **mousiki** garbles hybrid frames, the mode every path is in with in-band FEC, in its
  encoder and in its decoder: decoded by libopus its packets come out 8 to 16 dB too loud
  with a frame loudness correlation of 0.50 to 0.77; its decode of libopus packets is
  -1 dB from libopus's. Its CELT-only frames match libopus. A `debug_assert!` in its CELT
  band folding fires on the first hybrid or CELT frame. Its repository is gone. It is not a
  candidate.

## How it was measured

### The four paths, offline (`probe opus-compare`)

Input: the wake corpus (English and Russian Windows voices at 16 kHz, upsampled; the
eight Codex-voice languages at 48 kHz) and four call questions in Russian (the Irina voice
at 48 kHz): 53 files, 269 s of speech. Every file through every codec on each path:

- `call`: our speech toward Codex: each codec encodes with the call's parameters (mono VOIP,
  32 kbit/s, complexity 10, FEC, 5 %), libopus decodes, as the realtime server does.
- `discord`: the assistant's voice toward Discord: each codec encodes (stereo VOIP,
  128 kbit/s, complexity 10, FEC, 10 %), libopus decodes, as a listener's client does.
- `receive`: the people in the channel: libopus encodes as a Discord client (stereo VOIP,
  64 kbit/s, FEC, 10 %), each codec decodes.
- `answer`: the Codex answer: libopus encodes with the call's parameters, each codec
  decodes with FEC from the next packet and concealment, as the jitter buffer does.

Loss on `call` and `answer`: none, 5, 10 and 20 % random, and a burst of 2 or 3 every 25
packets, the same seeded pattern for every codec. SNR and segmental SNR in dB against the
input after aligning the codec delay (an indicator: a perceptual codec does not keep the
waveform; libopus itself scores 8.5 dB); loudness r is the correlation of the 20 ms
loudness with the input's; level in dB; for decoders also the SNR against libopus's own
decode of the same packets. Release build, one run, the files in turn, nothing else heavy
on the machine. Full tables: `target\opus-compare\out\summary.md`, per file
`results.csv`, and the decoded speech to listen to under `target\opus-compare\out\<path>\<codec>\`
(`call-loss10` and `answer-loss10` with 10 % loss).

Our speech toward Codex (each codec encodes, libopus decodes):

| loss | libopus | opus-rs | ropus | mousiki |
|---|---|---|---|---|
| none: SNR / seg. SNR / loudness r / level | 8.5 / 7.2 / 0.998 / -0.3 | 8.5 / 7.2 / 0.998 / -0.3 | 8.5 / 7.2 / 0.998 / -0.3 | -6.9 / -2.2 / 0.765 / +8.4 |
| 10 % random | 7.8 / 6.9 / 0.992 / -0.4 | 7.8 / 6.8 / 0.992 / -0.5 | 7.8 / 6.8 / 0.992 / -0.5 | -6.7 / -2.2 / 0.758 / +8.1 |
| 20 % random | 6.2 / 6.0 / 0.968 / -0.8 | 6.2 / 5.9 / 0.968 / -0.9 | 6.2 / 6.0 / 0.968 / -0.9 | -6.3 / -2.1 / 0.738 / +7.7 |
| burst of 3 | 6.2 / 6.2 / 0.970 / -0.8 | 6.2 / 6.2 / 0.971 / -0.8 | 6.2 / 6.2 / 0.971 / -0.8 | -6.6 / -2.1 / 0.744 / +7.9 |

The assistant's voice toward Discord (each codec encodes, libopus decodes), no loss:
libopus, opus-rs and ropus 8.7 / 7.5 / 0.999 / -0.1; mousiki -15.6 / -7.4 / 0.500 / +15.5.

The people in the channel (libopus encodes as a client, each codec decodes), no loss:
libopus, opus-rs and ropus 8.7 / 7.5 / 0.999 / -0.2; against libopus's decode opus-rs
105.9 dB, ropus 90.6 dB; mousiki -1.0 / 1.0 / 0.803 / +2.4 and -0.8 dB against libopus.

The Codex answer (libopus encodes, each codec decodes with FEC and concealment), the SNR
of each codec's output against libopus's decode of the same packets, and the loudness r:

| loss | opus-rs | ropus | mousiki | loudness r: libopus / opus-rs / ropus |
|---|---|---|---|---|
| none | 104.7 | 90.9 | -1.3 | 0.998 / 0.998 / 0.998 |
| 5 % random | 97.8 | 81.1 | -1.3 | 0.998 / 0.998 / 0.998 |
| 10 % random | 92.3 | 46.2 | -1.2 | 0.992 / 0.992 / 0.988 |
| 20 % random | 84.7 | 16.0 | -0.9 | 0.968 / 0.968 / 0.945 |
| burst of 2 | 95.6 | 15.0 | -1.0 | 0.986 / 0.986 / 0.964 |
| burst of 3 | 94.6 | 14.4 | -1.0 | 0.970 / 0.970 / 0.937 |

### Speed

Per 20 ms frame in µs, median / p99 / max, from the same run (`opus-compare`, release):

| | libopus | opus-rs | ropus | mousiki |
|---|---|---|---|---|
| encode toward Codex (mono, 32 kbit/s) | 417 / 826 / 3244 | 227 / 394 / 1640 | 158 / 394 / 956 | 236 / 415 / 996 |
| encode toward Discord (stereo, 128 kbit/s) | 685 / 1004 / 1358 | 280 / 556 / **18379** | 262 / 500 / 1003 | 293 / 558 / 834 |
| decode a client's stereo stream | 90 / 183 / 584 | 31 / 70 / 1000 | 32 / 65 / 353 | 30 / 77 / 478 |
| decode the answer (mono) | 49 / 103 / 455 | 16 / 41 / 674 | 17 / 39 / 362 | 17 / 42 / 407 |

The app's own benchmarks (`cargo test --release`, `VOICEBRO_BENCH=1`,
`VOICEBRO_OPUS_CODEC`; power plan Balanced, nothing else over 3 % CPU), µs per 20 ms:

| | libopus | opus-rs | ropus | mousiki |
|---|---|---|---|---|
| one pull toward Discord (fill + our encode), mean / p99 / max over 3000 frames | 630 / 828 / 1132 | 258 / 401 / 994 | 273 / 435 / 1050 | 316 / 453 / 984 |
| the receive path, 4 speakers, with the call encode and one playout decode | 295 | 131 | 145 | 140 |
| decoding 4 speakers in the app, mean / p99 / max | 402 / 557 / 954 | 148 / 247 / 537 | 145 / 239 / 507 | 142 / 230 / 428 |

The single 18 ms encode of opus-rs in `opus-compare` (one frame of about 13,450 at
128 kbit/s stereo) did not come back in 3000 frames of the benchmark (max 994 µs); it is noted, not
explained. With libopus the app's own decoding of four speakers would cost 402 µs a tick
(songbird does the same work today in its receive task); with opus-rs or ropus it is under
150 µs. This libopus is built by `libopus_sys` through CMake with MSVC; the ports are
compiled by rustc with the app's release profile.

### Discord and the loopback

`probe discord-delivery 30 --opus --codec <name>` in the test channel (30 s of a tone
through the app's Opus output, release): for all four codecs Discord passed every packet
through (1501 to 1502 encoded, the same number passed through, 0 decoded by songbird),
0 encode errors, 29 receiver reports on our stream with 0 % loss, jitter 0.6 to 2.0 ms
(voice servers in Singapore, a different one per run). Discord took mousiki's packets as
readily as the others': a server does not decode them, so this shows nothing about how
they sound.

`probe rtc-loopback --codec <name>` (two peers in one process, a tone): 100 packets, 0
decode errors, tone correlation 0.999 (mousiki 0.998). `loopback_call_loses_nothing`
(the app's send path and jitter buffer, 10 s each way, `VOICEBRO_OPUS_CODEC`): 615 frames
sent, no drop, no late or concealed packet for all four; correlation at the far end
(libopus) 1.000 for all, of our playback 1.000 except mousiki 0.969.

### The wake word on decoded speech

The people in the channel reach the wake engine through the receive decoder. The wake
corpus (98 runs: each file with and without noise 300, the test's own phrases per
language) was run with `scripts\run-wake-corpus.ps1` on the original files and on the
copies `opus-compare` made by encoding each file as a Discord client does (libopus) and
decoding it with each codec (`-CorpusRoot target\opus-compare\out\receive\<codec>`):

| | original | libopus | opus-rs | ropus | mousiki |
|---|---|---|---|---|---|
| right of 98 | 77 | 79 | 79 | 79 | 58 |

opus-rs and ropus give exactly libopus's results, file by file. The libopus copies differ
from the original in two runs, both now right (Zira's "Bro, how do I craft a sword?" and
"Goodbye.", which the original missed; the copies went through the 16 to 48 kHz upsampling
and Opus). mousiki's copies differ from libopus's in 29 runs, for the worse: "отбой" heard
as "от боли", "братан" as "награждён", "brother" as "rather", "hey bro" as "haber".

### Live calls to Codex

`live_call_when_enabled` (debug build, the ChatGPT account, the realtime voice and
`codex.handoff_mode` items) with `VOICEBRO_OPUS_CODEC` set to each codec, twice each: run
a is q1 "Запомни число сорок два", then in a second call q2 "Какое число я просил тебя
запомнить?" (it must answer forty-two), then q3 "Сколько сейчас стоит видеокарта RTX
5090?", which the voice hands to the Codex agent (web search) and retells; run b is q1 and
"Какая погода будет завтра в Москве?". The questions were made with the Windows Irina voice
at 48 kHz. The codec encodes our speech and decodes the answer in each call. Every call was
recorded (`VOICEBRO_CODEX_RECORD`).

| | libopus | opus-rs | ropus | mousiki |
|---|---|---|---|---|
| canary first, both runs | yes | yes | yes | yes |
| q2 answered forty-two | yes | yes | yes | yes |
| what Codex heard (run a: q1 / q3) | "сорок два" / "RTX 5000 девяносто" | "42" / "RTX 5090" | "сорок два" / "RTX 5090" | "42" / "RTX 5090" |
| what Codex heard (run b: weather) | right | right | right | right |
| the agent searched for | RTX 5090 | RTX 5090 | RTX 5090 | RTX 5090 |
| encode errors, decode errors, speech gaps | 0, 0, 0 | 0, 0, 0 | 0, 0, 0 | 0, 0, 0 |
| packets concealed (FEC), long call of run b | 23 (13) | 0 | 0 | 29 (26) |
| last speech → first answer audio, ms | 1208 to 1799 | 1302 to 1984 | 1098 to 2348 | 1265 to 2029 |

All eight calls passed. Codex understood every question with every codec, mousiki's
included: its garbled hybrid frames (louder, see above) were still clear enough for the
realtime model's recognition of a clean TTS voice. The latency differs by run more than by
codec (the server's own timing); the concealed packets are network loss on those calls.

The replay stand (`replay_recordings_when_enabled` with `VOICEBRO_OPUS_CODEC`) played all
eight recordings through each codec's decoder: 0 holes and 0 speech gaps with every codec.
The speech the playout found was the same to the frame for libopus and opus-rs in all
eight, within 20 ms for ropus, and 0.5 to 6 % shorter for mousiki (its decode is
different, which moves the speech detector), with 2 bridged frames in one recording. The
recordings that hold two calls replay only the first call when the second started with
lower sequence numbers (counted as late); that is the stand's, the same for every codec.
The WAVs of each replay are next to the copies of the recordings in the session's
scratchpad (`replay\<codec>\*.rec.wav`).

### Unit tests

The app's own tests run every codec (`audio::codec`, `audio::opus_out`,
`discord::speaker_decode`) and pass for libopus, opus-rs and ropus; mousiki is held out of
the two sound checks of the Discord output and reported (FEC better than concealment by
-0.3 dB against libopus 42.9, opus-rs 36.9, ropus 36.1). The jitter buffer's tests, tuned
on libopus's concealment, pass for libopus and opus-rs (28 of 28 with the speaker tests);
with ropus a long stall in speech plays a fourth frame after the three bridged ones; with
mousiki a 5 % slow stream with stalls has one 20 ms hole that libopus does not have.

### Size

| | 1.0.0 release (libopus only) | trial build of this change (four codecs) | difference |
|---|---|---|---|
| `voicebro.exe` | 21,395,968 B | 23,764,480 B | +2,368,512 B (+11 %) |
| installer | 6,664,432 B | 7,540,818 B | +876,386 B |
| portable zip | 9,305,819 B | 10,494,523 B | +1,188,704 B |

The difference holds the three codecs and the rest of this change (the codec layer, the
speaker decoder, the window's list) and, in the zip, the longer notices.

After the decision, with libopus and opus-rs only (trial build of the same change):
`voicebro.exe` 22,196,224 B (+800,256 B, +3.7 % against 1.0.0), installer 6,960,669 B
(+296,237 B), zip 9,693,810 B (+387,991 B).

After the panic decision, with `panic = "unwind"` (trial build of the same change):
`voicebro.exe` 33,782,272 B (+11,586,048 B, +52 % against the two-codec build above),
installer 8,817,932 B (+1,857,263 B), zip 12,351,878 B (+2,658,068 B). Nearly all of it is
the unwinding itself (landing pads and unwind tables in every crate): the same code built
with `panic = "abort"` (a plain `cargo build --release -p voicebro-desktop` with
`CARGO_PROFILE_RELEASE_PANIC=abort`) gives 22,230,528 B, against 33,747,456 B for the
same plain build with unwind. The memory the app uses was not measured after the switch.

### The setting in the app

The trial release build (`scripts\build-release.ps1 -Trial`) with a scratch data folder and
the WebView2 debugging port: the list in Settings → Diagnostics switched to opus-rs, ropus,
mousiki and back to libopus; each save wrote `diagnostics.opus_codec` to `settings.json`,
the window said "Saved: Opus codec (experiment)", the line under the list said "In use:
none (the bot is in no voice channel)", the app kept running, and the log had "settings:
Opus codec <label> from the next join and the next call" for each.

## Not tested

- A real channel with two or more people and a game running, with each codec selected in
  turn (the user's step; nothing counts as verified before it).
- Listening: the decoded files are written for it; nobody has listened to them yet.
- What a listener in Discord hears from each encoder; Discord's reports only show the
  packets arrived.
- Neural concealment (deep PLC, DRED): none of the four is built with it here.
- Long runs: the longest stream was one file of a few seconds per codec and path, a 30 s
  tone toward Discord and the live calls.
- A real panic of opus-rs in a running app: none was seen, and one cannot be caused on
  purpose. When this report was measured the release profile aborted on panic; since the
  decision above it unwinds, and the path (the frame replaced, one report, the bot leaving,
  the notice) is covered by tests with a test-only panic and by the window's mock.

## Recommendation

**opus-rs** is the candidate to replace libopus. On everything measured here it does what
libopus does: the same encode quality to a tenth of a dB on both directions, a decode
within rounding of libopus's, FEC and concealment that track libopus's under every loss
pattern, the same wake-word results file by file, eight live calls understood, Discord
passing its packets through. It is also two to three times faster than this libopus
build, and it is the only one of the three under active development. Its risks: it is
young (0.1.x, released every few days; its changelog fixed a panic in exactly our FEC
mode shortly before 0.1.42), one unexplained 18 ms encode was seen, and a panic in it
would end a release build as long as the release profile aborts on panic (since the
decision above it ends only that server's join).

**ropus** is the second choice: as good and faster at encoding, but its concealment
under loss is measurably further from libopus's (loudness r 0.937 against 0.970 for a
burst of 3 on the answer), and it plays the playout's bridging slightly differently.

**mousiki** is out: it garbles hybrid frames both ways and its repository is gone.

Next steps, each the user's decision:

1. Listen to the files under `target\opus-compare\out\` and test opus-rs in a real channel
   with two people and a game (Settings → Diagnostics → Opus codec → opus-rs, then
   `/leave` and `/join`).
2. Decide whether release builds should unwind on panic, so that a codec panic costs one
   frame instead of the app. (Decided on 2026-11-12: they unwind, and a codec panic ends
   that server's join with a notice; see "Decision".)
3. If opus-rs holds up: a follow-up change that makes it the default and removes libopus.
   That needs songbird off libopus too: its receive decode (already replaceable by the
   app's own, as here) and its fallback path (decode, mix, soft clip, encode when it
   refuses passthrough), for example through a `[patch.crates-io]` of `opus2` with a shim
   over opus-rs. Only then does CMake leave the build. ropus and mousiki would be removed
   in the same change.
