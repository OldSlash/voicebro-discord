# VoiceBro architecture

How the project is built: crates, components, their boundaries and contracts. This file
describes the target shape of the code, not the order in which it is written. Every agent working here reads it before touching code and
updates it in the same change as the code it describes.

Related files:

- `AGENTS.md` (local, untracked): working rules, build prerequisites, commands.
- `docs/MVP-PLAN.md`: the implementation plan for the first milestone. Plans change; this
  file only changes when the architecture does.
- `openspec/`: specs and change proposals.

## 1. What the product is

A Windows desktop app that runs a Discord voice bot. The people in a voice channel talk to an
AI assistant: an offline wake phrase opens a session, the assistant answers with voice, a
deactivation phrase or an idle timeout closes it. The app runs in the background on the same
PC the user is gaming on. The assistant is one of three engines (`assistant.engine`,
change `claude-and-opencode-engines`), each installed and signed in by the user and never
shipped: Codex through the Codex CLI app-server with the user's ChatGPT sign-in (the
default; no API key is involved), Claude Code run headless with the user's Claude sign-in
or API key, or OpenCode with the providers the user set up in it. Only Codex has the
realtime voice mode; Claude Code and OpenCode answer in the Whisper mode. The app never
reads or stores an engine's credentials.

Two assistant modes (`assistant.mode`, change `whisper-mode`):

- **Realtime voice** (the default): Vosk spots the phrases on the PC, and during a session the
  channel's audio goes to a ChatGPT realtime voice call that Codex opens; the call answers
  aloud.
- **Whisper**: whisper.cpp turns each speaker's speech into text on the PC and spots the
  phrases in that text; the questions go to the engine as text turns (with Codex on the
  same thread); a local voice reads the answer into the channel (4.12). Only text goes to
  the engine's provider.

Fixed constraints that shape the architecture:

| Constraint | Consequence |
|---|---|
| Rust, Tauri 2, Windows first | One native process plus WebView2 for the settings UI. The core never depends on the UI. |
| Low CPU and memory while idle | Heavy things (speech model, Whisper model, voice, assistant process, call transport) are loaded only while their feature is on, and unloaded when it is off. |
| Steady 20 ms audio | Pacing loops run on dedicated OS threads with `std::thread::sleep`. Tokio timers are never used for pacing. |
| Users install nothing but the app | No Node, Python or build tools at runtime. Native libraries and models are downloaded on first use with pinned SHA-256, and only after the user agrees in the window to what, from where and how much. |
| Replaceable engines | Wake-word recognition, speech to text, the local voice and the assistant sit behind traits (`WakeEngine`, `SpeechToText`, `Synth`, `Assistant`). Vosk and whisper.cpp are the one implementation of each of the first two; the assistant has Codex, Claude Code and OpenCode behind one `EngineSwitch` (4.7); the voice has sherpa-onnx and the Windows voices. |
| Diagnosable in the field | A file log in every build, including release: small (about 2 MB) and without what people said unless the user turns on the verbose log; the window shows everything, the recognizer's own results included. The status shows measured values only. |
| Secrets stay out of files | The Discord token lives in the Windows Credential Manager, never in the settings file or the UI snapshot. |

## 2. Repository layout

A Cargo workspace. The core is a plain library crate with no Tauri dependency, so it is
testable without a window and without the Common-Controls manifest problem that the Tauri
test executable has.

```text
voicebro-discord/
  Cargo.toml                   workspace: crates/*, apps/*
  .gitattributes               LF line endings for text, CRLF only for *.ps1 / *.cmd
  crates/
    voicebro-core/             the application core, no Tauri
      src/
        lib.rs
        assets.rs              the shared asset store: pinned downloads, archives and single files (4.4)
        audio/                 frame types, queues, resamplers, earcons, output source, Opus output
        discord/               serenity + songbird adapter, slash commands, member panel, the leg toward Discord
        gate.rs                session gate: mute, wake session state machine, pre-roll, Whisper's transcripts
        wake/                  wake-word engine trait, phrase spotter, model catalog, local models, Vosk
        stt/                   the Whisper mode's speech to text: whisper.cpp, catalog, cutter, transcriber (4.12)
        tts/                   the Whisper mode's local voice: sherpa-onnx, Windows voices, splitter, player (4.12)
        assistant/             assistant trait and the Codex implementation (realtime call and text turns)
        settings/              schema, file store, secret store
        events.rs              AppEvent and the broadcast bus
        runtime.rs             composition root: owns every subsystem, applies settings
  apps/
    desktop/                   the Tauri app (what `src-tauri` is in a plain Tauri project)
      Cargo.toml, build.rs, windows-app-manifest.xml, tauri.conf.json, capabilities/
      installer-hooks.nsh      the NSIS uninstall hook (section 4.10)
      icons/                   app-icon.png, the 1024 px source, and icon.ico generated from it (section 4.10)
      src/                     commands, event bridge to the webview, global hotkey, lifecycle
      ui/                      static HTML/CSS/JS, no bundler, `withGlobalTauri: true`
    probe/                     headless dev binary: live probes and self-tests, not shipped
  scripts/                     build-release.ps1, third-party-notices.ps1, make-wake-corpus.ps1, measure.ps1
  README.md                    the landing page: features, quick start, building from source, license
  docs/                        this file, the user guide (user-guide.md) and its images (images/)
  openspec/
```

Crate dependency direction is one way: `apps/*` depend on `voicebro-core`; the core depends
on nothing in `apps/`. The core exposes types and traits; the desktop app wires them to
Tauri commands and events.

## 3. Runtime topology

### Processes

| Process | Lifetime |
|---|---|
| `voicebro.exe` (Rust + Tauri) | Always, while the app runs. Closing the main window exits the app. |
| WebView2 process group (settings UI) | While the main window exists. The core never assumes a window exists, so a tray mode can be added later without touching it. |
| `codex app-server` (child process, JSON-RPC over stdio) | Started when the bot connects; stopped with the app. Exiting the app always stops it. |
| `claude.exe -p` (child process per guild, stream-json over stdio; change `claude-and-opencode-engines`) | Only with Claude Code as the engine: started at a join's preparation, stopped 0.5 s after the last leave, at an engine switch and with the app. |
| `opencode.exe serve` (child process, HTTP and SSE on 127.0.0.1; change `claude-and-opencode-engines`) | Only with OpenCode as the engine: as Claude Code. |

The realtime call (WebRTC) lives inside the Rust process, and so do Whisper and the local
voice of the Whisper mode (4.12): whisper.cpp with ggml (and CUDA), sherpa-onnx with
onnxruntime, and piper-phonemize are DLLs loaded with `libloading`, the Windows voices are
WinRT calls. None of them is a child process.

### Threads and tasks

| Work | Runs on | Rule |
|---|---|---|
| Discord gateway, songbird driver, HTTP, downloads | Tauri's tokio runtime | Ordinary async code. |
| Receive path: `VoiceTick` → mix → gate → assistant input, wake feed | Songbird's event task (tokio) | Must return within a fraction of 20 ms: no blocking calls, no locks held across awaits, only necessary allocation after warm-up. With opus-rs selected at the join it also decodes each speaker's packet (one decoder per SSRC, created the first time the SSRC speaks). |
| Discord output: songbird's mixer pulls one packet from `OpusOutput` (one `PcmSource` frame, then our Opus encode) | Songbird's mixer thread | A read never blocks for more than a few ms; on underrun it returns silence and counts it. Our encode replaces songbird's (passthrough), so the thread still encodes once per 20 ms; it costs about 0.64 ms on average (worst 1.4 ms) against songbird's 0.30 ms (`output_pull_cost`, release). |
| Discord RTCP: report blocks about our stream → expected loss | Songbird's event task (tokio) | Only atomics are written; the mixer thread reads them per packet. |
| Wake worker: recognizers, trailing silence, phrase detectors | One dedicated OS thread per engine, 20 ms tick | Fed through a bounded channel; a full queue drops and counts. |
| Whisper cutter: 16 kHz mono per speaker, utterance ends (`voicebro-whisper-cut`) | One dedicated OS thread while a Whisper model is loaded | Fed through a bounded channel of 256 messages (frames and ticks); a full channel drops the frame and counts it. Only downsamples and appends. |
| Whisper transcription (`voicebro-whisper`) | One dedicated OS thread while a model is loaded; on the CPU, whisper.cpp's own worker threads | One utterance at a time from a queue of 8; the oldest waiting is dropped and counted. Only this thread touches the model. |
| Local voice: synthesis and resampling (`voicebro-tts`) | One dedicated OS thread while a voice is loaded; onnxruntime's threads | Synthesizes the queued pieces in order, at most 20 s ahead of playback; a piece of a flushed epoch is skipped or stopped. |
| Local voice playback | Songbird's mixer thread, through `PcmSource::read` | `SpeechPlayer` hands out one 20 ms frame per read and never synthesizes. |
| Loading and unloading Whisper and the voice | `spawn_blocking` at a join; short-lived threads (`voicebro-whisper-load`, `voicebro-tts-load`, `voicebro-whisper-unload`, `voicebro-tts-unload`) for downloads, background reloads and the unload after the last leave | Never on the receive path or the mixer thread. |
| WebRTC call: ICE/DTLS/SRTP, RTP in and out | The webrtc crate's runtime tasks | Incoming RTP packets are only inserted into the call's jitter buffer; nothing is decoded here. |
| WebRTC send: pacing and Opus encode | One dedicated OS thread per call (`voicebro-rtc-send`), 20 ms tick | Sends exactly one packet per tick: the oldest waiting frame or silence. |
| WebRTC receive: Opus decode, FEC, concealment, adaptive playout | Songbird's mixer thread, through `PcmSource::read` | Decodes up to 100 ms ahead and plays one 20 ms frame per read, stretched or shrunk by the playout; buffers allocated when the call opens. |
| JSON-RPC to `codex app-server` | tokio task reading stdout line by line | Requests have timeouts; server-initiated requests are answered automatically. |
| Global hotkey | The desktop app's main thread (Tauri plugin) | Only toggles the app-wide mute through `Runtime`. |

### Event bus

The core publishes `AppEvent` values on a `tokio::sync::broadcast` channel. The desktop app
subscribes and forwards them to the webview as one Tauri event; the UI also pulls a full
`Status` snapshot on demand. The core never calls into Tauri.

```rust
pub enum AppEvent {
    Log { line: String },              // human-readable line; also written to the log, which reaches the window
    MuteChanged { muted: bool },       // the app-wide mute, from any of its three sources
    SettingsChanged,
    SecretChanged { name: String, is_set: bool },   // never the value
    ConnectionError { source: String, message: Option<String> }, // None = recovered
    // Added by later changes:
    Discord(DiscordStatus),
    Gate { guild: GuildId, status: GateStatus },
    Wake(WakeStatus),
    Whisper(WhisperStatus),            // the Whisper mode's recognizer: files, load, device, counters (4.12)
    Tts(TtsStatus),                    // the Whisper mode's voice: files, load, time to the first audio (4.12)
    Assistant(AssistantStatus),
    Chat(ChatEntry),                   // one assistant chat-log entry
    Assets(AssetProgress),             // downloads: label, percent, error
    CodecFailure { guild_name, channel_name, path, activity, codec, message }, // the bot left after an opus-rs panic
    Owner(OwnerStatus),                // the owner link or its waiting code changed (4.11)
}
```

Serialized with an `event` tag in `snake_case`, so the UI switches on `payload.event`. The
tag is not `kind`: a chat entry carries its own `kind` (user, assistant, task, …), and a
`kind` tag collided with it, so the window read every chat entry as some other event. A unit
test serializes every variant and checks that `event` appears exactly once.

## 4. Components

### 4.1 Audio core (`audio/`)

Owns the formats, queues and conversions shared by everything else.

- **Formats.** Discord side: 48 kHz, 2 channels, 20 ms frames (960 frames, 1920 `i16`).
  Wake engines: 16 kHz mono (320 samples per 20 ms). WebRTC: Opus at 48 kHz, 20 ms, mono
  payload. Whisper: 16 kHz mono `f32` in -1..1, whole utterances. The local voices: 16,
  22.05, 24 or 44.1 kHz mono `f32`, resampled to 48 kHz and played as stereo frames (4.12).
- **`Frame`**: a fixed-size 48 kHz stereo packet. Everything between Discord and the gate and
  between the gate and the assistant is a `Frame`.
- **Mixing** several speakers: the average. Sum into `i32`, divide
  by the number of speakers in the tick, then clamp to `i16` once; one speaker passes
  unchanged. Never saturate before averaging.
- **Resamplers** (`dsp.rs`): fixed-ratio windowed-sinc FIR, 48 k stereo → 16 k mono, and the
  24 k paths (kept and tested for a later websocket path, unused by the MVP). Deterministic, no
  external resampler. `Resampler` (change `whisper-mode`, design D7) takes the local voices
  to 48 kHz: a polyphase windowed-sinc filter for any rational ratio (`to / from` reduced by
  their greatest common divisor), 32 taps per phase, a Blackman window, the cut-off at 0.92
  of the lower Nyquist frequency, each phase normalized to unity gain. It streams: audio fed
  in chunks comes out exactly as fed at once, so a chunk border makes no click; `finish`
  lets out the filter's tail, and `convert` resamples a whole clip. Tests check that 16,
  22.05, 24 and 44.1 kHz sines keep their frequency and level.
- **`OutputQueue` / `PcmSource`**: the assistant's audio toward Discord. A small adaptive
  jitter buffer (3 to 8 frames), drop-oldest when full, a flush epoch (so a cut answer never
  plays after unmute), an output mute that still lets earcons through, and counters
  (supplied silence, drops, underruns). `PcmSource::fill_frame` gives one 20 ms frame of
  f32 stereo to the Opus output below; `PcmSource` still implements songbird's
  `MediaSource` for the `RawAdapter` fallback. A producer with its own clock-free buffer (the Codex call's jitter
  buffer) attaches as a `FrameProvider`: while attached, `PcmSource` pulls one frame from it
  per read before the queue, flush and mute changes reset it, and a muted read still pulls
  and discards so the provider's playout keeps time. A provider also says whether voice is
  ahead (`FrameProvider::voice_ahead`, `false` by default): the call's jitter buffer sets it
  after each pull from the samples its playout has decoded ahead of the frame just played
  (up to `LOOKAHEAD`, 100 ms; in practice two to four frames) against `SIGNAL_PEAK` (256),
  so the output can turn sending toward Discord on before that voice plays (below). The
  second provider is the Whisper mode's `SpeechPlayer` (4.12). A provider that measures its
  own turns says so (`FrameProvider::own_latency`, `false` by default; the player's is
  `true`): `PcmSource` then reports none of its frames to the latency tracker, and the
  Whisper mode's latency line comes from the text session instead (7).
  `OutputHandle::push` stays for the stub assistant and the tests.
- **Opus codecs** (`codec.rs`, change `opus-codec-experiment`): two Opus codecs built in
  side by side, chosen at run time by `diagnostics.opus_codec` (4.6): libopus 1.5 through
  `opus2` (C, built by `libopus_sys` with CMake; the default) and opus-rs 0.1.42, a
  pure-Rust port of libopus 1.6 (float), pinned to its exact version. The comparison that
  chose it also ran ropus and mousiki; they are not built in
  (`docs/OPUS-CODEC-COMPARISON.md`).
  - **`OpusCodec`**: the two, with the names `libopus` and `opus-rs` (settings,
    `VOICEBRO_OPUS_CODEC`, probes) and `label()` (name and version, for the status and the
    log).
  - **`VoiceEncoder`**: built from `EncoderParams` (channels, application, bitrate,
    complexity, in-band FEC, expected loss); both codecs get all of them, the rest stays at
    libopus's defaults (opus-rs's own defaults of complexity 9, 64 kbit/s and FEC off are
    all overridden). `encode_f32`, `encode_i16`, `set_loss_percent`.
  - **`VoiceDecoder`**: libopus's convention for both codecs: an empty packet conceals
    `out.len() / channels` samples, `fec` rebuilds that many from the packet's in-band FEC,
    otherwise the packet is decoded into `out`; each call returns samples per channel.
    opus-rs decodes f32 only, so its i16 output goes through a buffer allocated at creation
    and is rounded and clamped as libopus does.
  - **Failures**: calls into opus-rs run under `catch_unwind`; a panic comes back as
    `CodecError::Panicked`, the codec's state is dropped and rebuilt with the same
    parameters at the next call, and the frame is replaced (silence out, concealment in).
    libopus calls are not wrapped (C returns errors and does not unwind). `FailureSpell`
    gives a path two log lines per spell of failures. Release builds unwind
    (`panic = "unwind"`, the user's decision of 2026-11-12), so this works there too. While
    a guarded call runs, a thread-local flag is set (`in_guarded_call()`); the desktop
    shell's panic hook (4.8) reads it to tell a codec panic from any other.
  - **A panic ends the join** (`CodecPath`, `CodecFailure`, `CodecFailureHub`,
    `CodecFailures`): every path gets a `CodecFailures` sink with its guild when it is
    built: `DiscordEncoder` (`discord-out`), the speaker decoders (`receive`), the call's
    send loop (`call-send`) and its jitter buffer (`call-receive`). A `Panicked` error is
    reported once per guild per join, with the path, the codec's label and the message,
    through an unbounded channel that never blocks the audio thread; an error that is not a
    panic reports nothing. The hub is reset for a guild at each join
    (`VoiceHost::codec_failures`), and the call gets the same sink in `AudioPorts.failures`.
    The runtime handles the report (4.7). The codec is never swapped behind the user's
    back: the setting stays as it is.
- **Opus output** (`opus_out.rs`, change `discord-voice-delivery`): the assistant's voice
  leaves as Opus packets the app encodes, so each packet carries in-band FEC that rebuilds a
  lost predecessor at the listener. songbird's own encoder has none.
  - **`OpusOutput`**: a symphonia `FormatReader` with one 48 kHz stereo Opus track of 20 ms
    frames. Each `next_packet` fills one frame through `PcmSource` (playout, gain, mute and
    earcons as before) and encodes it with `DiscordEncoder`. A closed source ends the
    track.
  - **`DiscordEncoder`**: VOIP, 128 kbit/s, complexity 10, in-band FEC, `packet_loss_perc`
    from the expected loss; the output buffer is allocated once. With FEC on and a loss
    expected, libopus stays in SILK or hybrid, the modes that carry FEC. An encode error
    sends a silence packet encoded at the start and is counted.
  - **Passthrough**: `live_input` hands songbird a `LiveInput::Parsed` with this format and
    songbird's Opus decoder wrapped in `CountingDecoder`. When the track is the only one
    playing, songbird copies each packet into the RTP payload (DAVE encrypts it after) and
    never calls the decoder. Encoded minus decoded is the number passed through; a decode
    means passthrough was refused and is logged once per spell.
  - **Expected loss** (`DeliveryShared`): an upper envelope of the loss Discord reports on
    our stream. A higher report raises it at once; otherwise it moves toward each report by
    a forgetting of 0.9983 per packet we send (the playout's memory, about 12 s), counted in
    packets so it does not depend on how often Discord reports. It starts from the default
    (10 %). With no new report it returns toward the default by the same forgetting, and the
    source shows "default" once the default outweighs the last report.
- **Source lifetime.** One output track per join. An empty queue (session over, nothing to
  say) produces silence, so a temporary underrun never ends the track. Only leaving the
  channel closes the source: `OutputHandle::close()` makes the next `read` return end of
  stream, and the track ends with it. Nothing else may end it. That silence is played but
  not sent toward Discord (next item).
- **Sending toward Discord** (change `transmit-only-when-speaking`): `PcmSource` decides, on
  songbird's clock, whether anything should reach Discord. Each frame it notes whether
  something is audible now (the assistant's frame played has a peak of `SIGNAL_PEAK` or
  more, before the gain, as for `last_signal`) or ahead (the provider's `voice_ahead`, a
  queued pushed frame with a signal, the peak stored at push, or earcon samples waiting).
  While output is muted the assistant's frames never count, only earcons. The state goes
  Silent → Opening → On → Silent:
  - Silent: anything audible asks for sending and starts Opening for `SEND_LEAD` (2 frames,
    40 ms). That is the time songbird needs to apply the driver's mute, which it does at
    the start of a later tick, normally the next one.
  - Opening: earcons wait, unmixed. The assistant's frames play on their clock: a voiced
    one played before Opening is over counts once as a cut start.
  - On: anything audible resets the hangover. After `SEND_HANGOVER` (25 frames, 500 ms)
    with nothing audible, it asks for silence. That is longer than any pause inside an
    answer in the recorded calls (at most 300 ms).

  The source starts Silent. The wish is a `tokio::sync::watch` channel
  (`OutputHandle::sending()`), sent only on a change. The session applies it to songbird's
  driver (4.2). `OutputCounters` carry `sending`, `sends_started`, `frames_not_sent` and
  `cut_starts`. Playout, gains, mute, earcons, latency lines and the Opus encoder run
  exactly as while sending.
- **Earcons**: four tones generated in Rust, mixed into the output even
  while muted, at volume 0.18 by default: session start 523.25, 659.25 and 783.99 Hz (70, 70,
  110 ms), session end 659.25 then 392 Hz (80, 130 ms), mute 660 then 440 Hz (90, 110 ms),
  unmute 440 then 660 Hz (90, 110 ms), each note with a 6 ms fade at both ends. An earcon
  requested while nothing is sent waits `SEND_LEAD` (40 ms) and then plays whole.
- **Gains** (`gain.rs`): `audio.input_gain` and `audio.output_gain`, 0.0 to 4.0, default
  1.0. `AudioGains` holds one `GainStage` per direction (the gain
  in an atomic, a clip counter, the peak of the last second), owned by the
  `RelayRegistry` and shared by every gate and output, so a saved change reaches the next
  frame without rebuilding anything. The input gain scales the frame the gate is about to
  send to the assistant (live and replayed alike) in `f32` and clamps to `i16`; the wake
  feed and the pre-roll keep the channel's own level. The output gain scales the
  assistant's samples where `PcmSource` turns them into floats (pushed frames and the
  call's jitter buffer alike) and clamps to -1.0..1.0, before earcons are mixed in, so an
  earcon keeps its own volume. Every clamped sample counts as a clip. The status carries
  both gains, peaks and clip counts (`Status.audio`).
- **Queue budget**: about 100 ms toward the assistant, about 160 ms toward Discord. Deeper
  buffering is voice latency and is not added without a measurement.

### 4.2 Discord adapter (`discord/`)

serenity 0.12 (gateway, HTTP, cache limited to guilds, voice states and members) and
songbird 0.6 with `receive`. DAVE is handled by songbird.

- **Start**: the client starts whenever a token is stored, at launch and after a token is
  saved; Connect and Disconnect stay as manual controls. There is no auto-connect setting.
- Slash commands `/join`, `/leave`, `/status`, `/link`, `/optin`, `/optout` and `/privacy`,
  registered globally on `ready` and, when `discord.guild_id` is set, also in that server
  (where they appear at once); each registration's result is logged. `/link code:<8 to 12
  characters>` (change `owner-only-access`, 4.11) and the three consent commands (change
  `voice-consent`, 4.13) are global with the contexts `Guild` and `BotDm`, so they also work
  in a direct message to the bot; the guild's copies have no contexts (Discord takes them
  only on global commands). `/link` hands the sender's id from the interaction, a cleaned
  name (global name, else user name) and the typed code to `VoiceHost::complete_link` and
  answers only the sender. No access mode limits a command: `/join`, `/leave` and the Mute
  button work for everyone. `/status` and the reply to a successful `/join` add one line for
  a limited mode: "Only the owner's voice reaches the assistant here." (`owner`) or "Only the
  owner and people who ran /optin are heard here. See /privacy." (`agreed`). `/status` also
  names the assistant mode, "Mode: Realtime voice." or "Mode: Whisper mode."
  (`VoiceHost::assistant_mode`, change `whisper-mode`).
- **Consent commands** (change `voice-consent`, 4.13). Every reply is ephemeral.
  - `/optin` with no `discord.privacy_url` answers that the operator has not published a
    policy and records nothing. With a record for the current text it gives the time and
    `/optout`. Otherwise it shows the consent text (`consent::consent_text`) with the buttons
    "I agree" (`vb:optin:agree:<version>`) and "Cancel" (`vb:optin:cancel`).
  - `handle_component` parses these ids before the Mute button's. Agree hands the presser's
    id and cleaned name and the version to `VoiceHost::agree`. An ephemeral message is seen
    and pressed only by the person who ran the command, so the presser is the person who
    agrees. The message is replaced by the outcome without buttons: recorded (naming the
    mode when it is not `agreed`), already agreed, or an older text. Cancel replaces it with
    "Nothing changed."
  - `/optout` calls `VoiceHost::withdraw` and says that earlier speech was already sent.
    `/privacy` gives the URL or says none is published.
- `/join` first awaits `VoiceHost::prepare_join(guild)`: the assistant starts its process
  and checks its sign-in, and in the Whisper mode Whisper and the voice load (4.7). An error
  is the reply ("Not joined: …") and the bot stays out of the channel; only then does the
  join below run.
- **Member panel** (`panel.rs`): an embed in the channel the command came from, with the
  engine, the assistant's state (waiting for the wake word / listening / muted), the mode
  ("Realtime voice" or "Whisper mode") and a "Mute GPT" button. A mode switch marks the
  panels for a refresh. The button toggles the app-wide mute through `Runtime`; the panel is
  refreshed from `AppEvent::Gate` and `AppEvent::MuteChanged`.
- **Join order is fixed:** get or create the `Call`, set its decode mode, register the
  `SpeakingStateUpdate` and `VoiceTick` handlers, then `join`. Discord announces a user's
  SSRC only the first time they speak.
- **The Opus codec of a join** (change `opus-codec-experiment`): `join_voice` asks
  `VoiceHost::opus_codec()` (`diagnostics.opus_codec`, 4.6) once and logs "Opus codec for
  this join: <label> (speakers decoded by songbird | the app)". With libopus songbird
  decodes the speakers as before (`DecodeMode::Decode`, 48 kHz stereo). With opus-rs the
  call's config gets `DecodeMode::Decrypt` before `join` (`Driver::set_config`
  hands it to the next connection; songbird's receive task reads the mode only when it
  connects, checked in the 0.6.1 source), so songbird still removes the transport
  encryption and DAVE and keeps its playout buffer, and the `VoiceTick` handler decodes
  each speaker itself (`speaker_decode.rs`): one `VoiceDecoder` per SSRC, 48 kHz stereo,
  the payload songbird would decode (`payload_offset..payload_end_pad`, the RTP header
  extension skipped), a concealed frame for each missing sequence number (at most 10) and
  for each tick that reports a speaker without a packet, no FEC, speakers dropped after
  songbird's 60 s state timeout. A decoder failure conceals that speaker's frame and counts
  `VoiceCounters.decode_errors`. A test feeds the same packets (a gap, a missed tick, header
  extensions) to this path with libopus and to `opus2::Decoder` used as songbird does and
  requires identical samples. The output track is built with the same codec (4.1), and
  `VoiceChannelStatus.opus_codec` carries its label. songbird's fallback when it refuses
  passthrough (decoding our packets, mixing, soft clipping, encoding again) stays libopus.
- **Leave** uses `Songbird::remove`, not `leave`, so the `Call`, its handlers and tracks are
  gone, and closes the output source. Rejoin builds a new gate. Join and leave are
  serialized per guild across the whole awaited operation.
- **Speaker directory** per guild maps SSRC → `SpeakerEntry { user, bot, name }`: the
  Discord account, whether it is a bot, and for people the display name (nickname, then
  display name, then username; printable, trimmed, at most 32 chars). It outlives the
  call. Every speaking event records its account, bots included (change
  `owner-only-access`), so a bot's stream is known and has no name; the account goes in
  at once (`set_user`) and the name after the member lookup, which may be an HTTP call, so
  the owner filter knows the source from its first frame. The first time a
  source with no entry at all is heard, and only then, the receive handler checks the
  channel's people: when exactly one non-bot member is there, that member's name and
  account are taken for the source and logged "assigned as the only person in the
  channel"; a real mapping from a later speaking event replaces it. (Before the change a
  bot's stream, having no name, got the only person's name this way.)
- **Participants** are kept current from `voice_state_update`. The session keeps them with
  their account ids (`VoiceSession.people`) for the rule above, the panel and the status show
  them all, and `VoiceHost::set_participants` gets them with their ids. The host keeps them
  per guild and gives the gate only the names the access filter lets through
  (`AccessState::passing_names`), again after each change of the filter (change
  `voice-consent`, D6): those names are the call context sent to the assistant.
- **The access filter** (changes `owner-only-access` and `voice-consent`, 4.11 and 4.13).
  Each `VoiceTick` reads `VoiceHost::voice_filter()` once, an `Arc<VoiceFilter>`
  (`Everyone` or `Only(ids)`). For `Only`, the "only person" rule runs for every source
  first, then only the sources whose account is in the set pass (`passing_sources`); an
  empty set lets none pass. The others are not decoded (opus-rs), not mixed and not handed
  to the host; each dropped frame counts in `VoiceCounters.ignored_frames`, and the first
  drop of a source per join logs "receive: ignored <name or an unknown source> (ssrc N) in
  guild G: <not the owner | has not run /optin>". The gate, the wake feed and the assistant
  need nothing of their own: they only ever see the voices that passed.
- The receive handler decodes per-speaker PCM at 48 kHz stereo (never mono or 24 kHz: songbird
  0.6's packet-loss recovery for those modes is unproven), mixes it into reusable buffers,
  and calls the app's `VoiceHost::on_tick(guild, mixed, per_speaker)`. `VoiceHost` is the
  one trait the adapter needs from the rest of the app (ticks, speaker names, participants,
  join and leave, the assistant state for the panel, mute); the session gate implements it.
  Participant identity is preserved up to the gate and the wake engine; the assistant hears
  the mix.
- Output is one `Track` from `opus_out::live_input` (Opus packets songbird passes through),
  attached with `play_only`. If that track cannot be built, the session falls back to
  `RawAdapter<PcmSource>` (f32 PCM, songbird encodes, no FEC) with a warning. The crate
  that owns the track enables symphonia's `pcm` feature; without it songbird cannot decode
  the raw stream and the fallback fails with "no compatible track found".
- **Sending only while audible** (change `transmit-only-when-speaking`). Before
  `play_only` the driver is muted with `songbird::Driver::mute(true)`. It is never
  `Call::mute`, which also sets the bot's self-mute in Discord's voice state, so members
  would see the bot muted.
  - A task per join (`session::apply_sending`) applies each change of
    `OutputHandle::sending()` (4.1) as `Driver::mute(!on)`. It is kept in
    `VoiceSession.sending_task` and aborted at the leave, before the call is removed.
  - A muted driver still pulls the track every 20 ms. It sends five `F8 FF FE` silence
    frames and then nothing; the mute takes effect at the start of a later mixer tick,
    normally the next one (hence `SEND_LEAD`).
  - songbird moves the RTP sequence number and timestamp on every tick, sent or not, so
    the timestamp of the first packet after a pause is on time.
  - Its speaking flag (op 5) stays set while the track lives. There is no API to clear it.
  - The encoder and the loss path do not read the state. Every frame is still encoded,
    and `packets_encoded` includes the packets the muted driver dropped; the output's
    `frames_not_sent` sits next to it.
- **The leg toward Discord** (`delivery.rs`): a `DeliveryHandler` for `DriverConnect`,
  `DriverReconnect` and `RtcpPacket` is registered before the join. It keeps the voice
  server's host and our SSRC (never the session id or token). Each RTCP packet is cut at
  songbird's `payload_end_pad` and its report blocks (sender and receiver reports) about
  our SSRC feed the expected loss. Discord sends a receiver report about our stream about
  once a second (probe of 2026-11-10). The host and the region read from it are logged at the
  join. Discord names hosts in two forms, `finland14057.discord.media` (region `finland`)
  and `c-sin19-5f0d7996.discord.media` (region `sin`): the region is the letters of the
  first dash-separated part that is letters then a digit, else the label's leading letters.
  `VoiceChannelStatus.delivery` (`DeliveryCounters`) carries them with the encoder's
  counters and the last report, and the leave logs one line with them and the output's
  sending counters (times sending turned on, time nothing was sent, cut starts).
- Changing the token restarts the client; a running client never keeps stale credentials.
- **The agent's answers in a text chat** (`answers.rs`, change `post-assistant-answers`).
  `DiscordRuntime::post_answer(answer, destination)` posts one `AgentAnswer` (4.5) for the
  runtime's handler (4.7) and returns a `PostOutcome`.
  - *Destinations* (`assistant.post_answers`, read per answer). `/join` stores the channel
    it was typed in as `VoiceSession.command_channel` before the panel is posted.
    `resolve` (a pure function) turns the destination into a channel:

    | Setting | Channel | Bot has left the voice channel |
    |---|---|---|
    | `voice_channel` | the session's voice channel; its built-in chat has the same id | not posted, logged |
    | `command_channel` | the session's `command_channel` | not posted, logged |
    | `channel` | `answers_channel`, if the cache finds it in the answer's server | posted |

    A chosen channel of another server posts nothing anywhere and is logged once per voice
    session of the answer's server (`Shared.answers_elsewhere`, cleared at each join). A
    chosen channel in no cached server is a refusal without a request. The last column
    applies only to an answer already handed over when the bot left: the Codex session
    forgets its open turns and unmaps its thread at the leave (4.5), and Codex stops 0.5 s
    after the last leave, so a task that completes after `/leave` produces no answer.
  - *Format* (`format_answer`): a first line `-# Codex: <task>` (Discord's small subtext;
    the task's whitespace folded, cut to 150 characters with `…`; `-# Codex` without a
    task), then the agent's text with its Markdown. Each Markdown table (a row with `|`, a
    separator row of dashes, then rows with `|`) outside a code block goes into a code block
    of its own, its columns padded to line up (`wrap_tables`). Plain content, never embeds,
    so links get Discord's previews.
  - *Split* (`split_for_discord(text, 2000)`): parts of at most 2000 UTF-16 units (never
    less than characters). A cut falls at a blank line, else a line break, else a space,
    else at the limit, outside code blocks first, the last of its kind before the limit; a
    cut in the second half of the message is preferred to a better kind earlier. Only a
    code block longer than a message is cut: the part ends with a closing fence and the next
    one starts with the fence and its language again. No cut right after a fence opens or
    before it closes.
  - *No pings*: every part is sent with `CreateAllowedMentions::new()` (`parse: []`, no
    users, no roles; `answer_message`), so `@everyone`, `@here`, roles and users stay text.
    The parts go out in order; serenity waits out rate limits; the first refused part stops
    the rest, nothing is retried. The refusal carries Discord's message, status and code,
    and, when the cache holds the bot's member, the post permissions it lacks there.
  - *The channel list* (`DiscordRuntime::text_channels`, command `discord_text_channels`):
    for every cached server (by name) its text and announcement channels in Discord's order
    (outside any category first, then by the category's position, then by position), each
    `{id, name, can_post}`. `can_post` comes from `channel_permissions`, a pure function of
    Discord's permission algorithm over the guild's roles, the channel's overwrites and the
    bot's member (owner and Administrator pass): View Channel, Send Messages and Embed Links.
    Without the bot's member in the cache it is `null`. Voice channels are not listed (their
    chat is `voice_channel`). Not connected: no servers.

### 4.3 Session gate (`gate.rs`)

One `RelayGate` per guild. It sits between Discord and the assistant and owns the session
state machine. A `RelayRegistry` holds all gates and the single app-wide mute flag.

```text
                ┌──────────┐  activation phrase / always-on   ┌────────┐
   Waiting ────►│ Starting │────── assistant ready() ────────►│ Active │
      ▲         └──────────┘                                   └───┬────┘
      │              │ deactivation                                │ deactivation,
      └──────────────┴──────────────────────────────────────────── ┘ idle timeout
```

- **Modes**: `always` (session is always wanted while connected) and `wake_word`.
- **Mute** is one app-wide state with three sources: the app button, the global hotkey and
  the Discord panel button. All three end in `Runtime::toggle_mute`, which goes through
  `RelayRegistry::set_muted` into every gate. Mute ends the session: the session is no
  longer wanted (the call closes, nothing is spent while muted),
  nothing is sent to the assistant, the output is flushed and discarded, and phrases are
  ignored. Unmute in always mode wants a new session at once; in wake-word mode the next
  session needs the activation phrase. The mute and unmute tones play on every change when
  earcons are on (`wants_session = !muted && (always || active)`).
- **Pre-roll** of about 5 s of frames. On activation the replay starts at the end of the
  phrase (recognizer timing, or a 1 s lookback plus 80 ms margin), holds while the
  assistant is not `ready()`, and catches up by skipping silent frames. Replay is capped at
  about 6 s: reaching the cap while the assistant is still not ready drops the held audio,
  ends the session with the falling earcon and reports the connection error `assistant:
  the assistant did not become ready in time`. Nothing waits forever.
- **Idle timeout** ends the session after N seconds without input; the assistant's own audio
  counts as activity, so a long answer is never cut.
- In wake-word mode the output is muted whenever no session is active, so a deactivation is
  silent at once, and it flushes the answer in progress.
- **One deactivation path** (`GateState::deactivate`, change `voice-recognition-rework`,
  design D4) for both sources, the wake engine (`on_phrase`) and the call's transcript
  (`on_heard`): it ends the session, flushes the output, queues the falling earcon (only
  with earcons on; the Codex session's later flushes do not remove it, since `flush` and
  `set_muted` leave the earcon queue alone), logs "deactivation phrase «x» from the wake
  engine | the call's transcript; …" and sets `deactivated_at`. Activations within 1.5 s of
  `deactivated_at` (`REACTIVATION_GUARD`) are ignored, so the end of a goodbye heard by
  another member's microphone, or the wake word right after the phrase, starts nothing;
  the guard lives in the gate and covers both sources. `on_phrase` returns a
  `PhraseOutcome` (`Applied`, `AlreadyInState`, `RightAfterDeactivation`, `NotListening`)
  for the worker's decision line; `on_phrase_at` takes the instant for tests.
- **The call's transcript as a second source** (`on_heard(turn, text, done)`, design D2).
  In wake-word mode, with mute off and a session active, the people's transcript of the
  turn so far is checked for the selected model's deactivation phrases in effect
  (`GateSettings.deactivation_phrases`, stored or default, in the matching form of 4.4;
  no vocabulary check). `spotter::transcript_match` finds whole words in a row; a match
  that ends before the last word, or at a last word followed by a space or punctuation, is
  closed and ends the session at once. A match at the last word with nothing after it is
  held (`HeardHold`), since the next piece may extend the word (" Good" + "bye", " stop" +
  "watch"): the next piece of the turn re-evaluates the whole text, the turn's `done`
  confirms it, a later turn confirms it, and `process_tick` confirms it once
  `TRANSCRIPT_HOLD` (300 ms) passed without a newer piece. While a match is held the
  output is muted (`output_muted` includes the hold), so the voice's reply to the phrase,
  which in the live calls began 270 to 1 500 ms after the phrase, is discarded instead of
  heard; if the word went on, the hold is dropped and the voice plays again (a few hundred
  ms of it lost). The call's own voice detection may end a turn at a pause inside a
  phrase ("Отбой" | ", братан" came as two turns in the live calls), so a turn that ended
  less than `PREVIOUS_TURN_WINDOW` (2 s) before is read with the next one
  (`HeardTurn`), and only a match that ends in the newer turn counts
  (`transcript_match_after`). Always mode, mute, no session
  and another model's phrases change nothing. A settings change of the phrases drops a
  held match but never ends a session; the runtime reapplies `GateSettings` when the
  `wake` section changes.
- Activation counts only while waiting, deactivation only while active. Repeating the
  activation phrase during a session does nothing.
- The gate tells the assistant `session_wanted`, `call_context(caller, participants)` before a
  session, and `speakers_changed(names)` when the set of talking people changes (the
  latter only with `speaker_labels: changes`; `start` sends names in the call context only).
  With one speaker at a time on, the floor decides the notes instead (below).
- **One speaker at a time** (change `speaker-turns`; `assistant.one_speaker_at_a_time`,
  `GateSettings.one_speaker_at_a_time`, off by default). A session decides once whether it
  uses the floor, on its first passing tick or at its activation (`GateState.floor_on`), so
  a change applies from the next session; off, everything above is unchanged.
  - *The floor* (`gate/floor.rs`, `Floor`). The first voiced speaker of a tick takes a free
    floor (the louder of two). Their frames go to the call as they come. Whoever else
    speaks waits: their frames go to a buffer of their own (`MAX_WAIT_FRAMES` = 750, 15 s;
    the oldest dropped and counted in `floor_dropped`; after 15 silent frames in a row a
    waiting person's silence is no longer kept).
  - *Passing.* The holder releases the floor after `FLOOR_PAUSE_TICKS` = 15 receive ticks
    (300 ms) without a voiced frame, once their own buffer has played. The floor then goes
    to whoever started waiting first, unless the assistant's voice played within 200 ms
    (`OutputHandle::last_signal`): the buffer then waits for the answer to end, so it does
    not cut it. A new speaker while people wait queues behind them.
  - *The buffer.* It goes to the call one frame per tick, silent frames at its front skipped
    while more wait (the replay's catch-up), and the holder's live frames follow it (a
    silent live frame only right after a voiced one, so a finished speaker's buffer
    empties). Unless the labels are `off`, a passed buffer is held `NOTE_HOLD_TICKS` = 12
    ticks (240 ms) for its note.
  - *Notes.* At `start` and `changes` a note at every change of the floor (not for the
    caller the call context already named), at `off` none; the gate sends the name through
    `speakers_changed`, and the Codex session sends any note it gets unless the labels are
    `off`. Check 6.1 of the change (`docs/SPEAKER-TURNS-PROBES.md`): with a note at every
    change the answers named who asked what right in 7 of 10 calls and never the wrong way
    round; without notes, right in 3 and wrong in 5. A detail line "floor: passes to
    <name>, who waited; …" is logged on each pass.
  - *The pre-roll per speaker.* While the setting is on, `Floor::remember` keeps each
    speaker's last 250 frames, and a ring is given back after 5 s without a frame. An
    activation gives the caller the floor and replays the caller's ring from the end of the
    phrase. Every other ring's frames from that point that hold speech go to that person's
    buffer, in the order they started talking. The mixed pre-roll stays for a session
    without the floor.
  - The end of a session, mute and a deactivation clear the floor and every buffer.
    `GateStatus.floor_waiting` and the counters `floor_passes` and `floor_dropped` show it.
- The gate is the receive path's tick sink (`VoiceHost::on_tick` finds the guild's gate in
  the registry), the wake worker's `PhraseSink` (the registry routes `on_phrase` to the
  guild's gate) and the assistant's `HeardSink` (the registry routes `on_heard` the same
  way). It numbers its receive ticks per guild (`u64` from 1). In wake-word mode, when not
  muted, it forwards each speaker's frame with the tick's number to the `WakeFeed`, then
  calls `WakeFeed::tick(guild, tick)` once per `process_tick`, after the frames.
- **The Whisper mode** (change `whisper-mode`, designs D2 and D4; `GateSettings.whisper`,
  with the Whisper phrases in effect in `GateSettings.whisper_phrases`):
  - *Audio.* Whenever mute is off, in both session modes, the gate forwards each speaker's
    frame and the tick to the feed, which the runtime routes to Whisper (4.7, 4.12). It
    sends no frame to `AudioPorts.input`, and the replay is not used: a question is text.
  - *Text.* The registry is the transcriber's `TranscriptSink` and routes each `Transcript`
    to the guild's gate. `on_transcript` asks `stt::phrases::decide` (4.12) what the
    utterance does:
    - waiting, in wake-word mode: an activation phrase starts the session (the rising tone,
      the speaker as the caller of the call context), and the words after it in the same
      utterance are the first question; any other text is ignored; the 1.5 s
      reactivation guard applies as for Vosk;
    - active: a deactivation phrase anywhere in the utterance ends the session through
      `GateState::deactivate` with `DeactivationSource::Whisper` ("Whisper's text"), and
      the words before it are dropped; any other utterance is a question;
    - always mode: every utterance is a question;
    - muted, or outside the Whisper mode: nothing counts.
  - *Questions* go to the session as `AssistantSession::user_text(UserText)`, with the
    speaker's name from the speaker map, the language Whisper used, and the instants, audio
    length, model and device that the latency line needs (7).
  - *Idle timeout.* It counts the activation, each question, the output's last signal, and
    every tick on which `AssistantSession::working()` is true (a turn runs or its voice
    plays), so a long web search does not end the session. It ends with the falling tone
    and the log line "session ended after no input; waiting for the activation phrase".
  - *A change of the flag* ends the session, as a change of the session mode does. A mode
    switch builds new gates anyway (4.7).
- `GateStatus` per guild: mode, muted, active, listening for the wake word, session wanted,
  assistant ready, last wake phrase, frames held for replay, people waiting for the floor,
  and counters (input frames the assistant did not take, replay frames dropped, replay
  timeouts, floor passes, waiting frames dropped). Every transition is published as
  `AppEvent::Gate`.

Speech detection for these decisions is a peak threshold (700) per 20 ms frame.

### 4.4 Wake-word engine (`wake/`)

Three layers, so the engine can change without touching the phrase logic:

```rust
/// A streaming speech recognizer for one speaker. Vosk today; sherpa-onnx or a KWS later.
/// There is no flush: the worker feeds trailing silence instead, which ends an utterance
/// the same way for every engine.
pub trait SpeechRecognizer: Send {
    fn accept(&mut self, pcm_16k_mono: &[i16]) -> Option<Recognition>; // partial or final
}

/// A loaded model, shared by all recognizers of one engine.
pub trait LoadedModel: Send + Sync {
    fn new_recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, WakeError>;
    fn knows_word(&self, word: &str) -> Option<bool>; // None: engine cannot tell
}

/// Where a model comes from: the catalog (downloaded by the app) or the models folder.
pub enum ModelSource { Catalog(&'static ModelEntry), Local { folder: String } }

/// An engine: which archives it downloads for a model, and how it loads a model directory.
pub trait WakeEngine: Send + Sync {
    fn id(&self) -> &'static str;                 // "vosk"
    fn assets(&self, model: &ModelSource) -> Vec<Asset>;  // library (+ the catalog model's archive), pinned SHA-256
    fn load(&self, assets_dir: &Path, model_dir: &Path) -> Result<Box<dyn LoadedModel>, WakeError>;
}

pub struct Recognition { pub text: String, pub is_final: bool, pub words: Vec<Word> }

/// Receives confirmed phrases (the session gate), and says what each one did.
pub trait PhraseSink: Send + Sync {
    fn on_phrase(&self, guild: u64, kind: PhraseKind, phrase: &str, ssrc: u32, since_phrase_end: Duration) -> PhraseOutcome;
}
pub enum PhraseOutcome { Applied, AlreadyInState, RightAfterDeactivation, NotListening }

/// Receives each speaker's 48 kHz stereo frames from the receive path, numbered by the
/// guild's receive tick, and the end of each tick (after its frames).
pub trait WakeFeed: Send + Sync {
    fn feed(&self, guild: u64, ssrc: u32, tick: u64, frame: &[i16]);
    fn tick(&self, guild: u64, tick: u64);
}

/// A phrase as typed (trimmed) and its words in the matching form.
pub struct Phrase { pub text: String, pub words: Vec<String> }
```

- **One matching form** (`settings::phrase_words`, change `voice-recognition-rework`, design
  D3): lower case, "ё" as "е", every character that is not a letter or a digit
  (`char::is_alphanumeric`) a space, split on whitespace. A phrase is its word list, and a
  phrase with no word is no phrase. The wake engine, the call's transcript (4.3) and the save
  validation (4.6) compare in this form; `PhraseSet` holds `Phrase`s.
  `spotter::find_occurrences` brings the recognizer's words to the same form, so a
  recognized "по-моему" becomes two words with the original word's end time, and "Братан
  всё" counts when the model writes "братан все". A phrase counts when its words appear in
  a row as whole words (`phrase_occurrences`, the first phrase of the list that fits at a
  place wins). The log and the status show phrases as the user typed them.

- **`WakeManager`** owns the engines, the assets directory and the worker. `configure(settings)`
  is called at start and whenever the `session`, `wake` or `mode` part changes. Outside
  wake-word mode it stops the engine (`Off`) and checks nothing. In the Whisper mode (change
  `whisper-mode`, design D1) Vosk is off too, whatever the session mode: Whisper spots the
  phrases, the state says so, and no Vosk download is asked for. In wake-word mode it first
  checks, in the calling thread, which files of the engine and the selected model are
  missing (two file stats), before it looks at the phrases. Missing files without the
  user's consent publish `NeedsDownload {items}` at once, with or without phrases, and
  nothing is requested from the network. Each `DownloadItem` has the label, the host, the
  bytes, the archive's `file` name and the official `page` a person downloads it from by
  hand (`Asset.page`: the Vosk release page on GitHub for the library,
  `catalog::MODELS_PAGE` for every catalog model).
  The phrases are those in effect for the selected model (`WakeSettings::selected_phrases`):
  each list as stored, or the model's default phrases for a list never set. With everything
  on disk it stops the engine when both lists in effect are empty, which only a user who
  emptied both fields gets (`NoPhrases`), sends new phrases to the running worker when only the phrases
  changed, and otherwise loads and starts on a thread of its own (`Loading` → `Ready`, or
  `Error {message}`). An unknown model or engine, or a local folder that is gone, skips the
  check and ends in the loader's `Error`. `WakeManager::download(model)` (the Download
  buttons, command `wake_download {model}`) is refused unless `model` is the selected
  model; it records the consent for that engine and model in memory and runs `configure`
  again with the last settings, which then downloads (`Downloading {label, percent}`),
  checks and extracts each archive (`Installing {label}`) and loads, or ends in
  `NoPhrases` when the model has none. The loader logs "using <file> from the assets
  folder for the <label>" instead of "downloading the <label>" when the archive is
  already there. The consent is not stored: files
  on disk need none, after a restart with files still missing the question comes back, and
  another model asks again. A load that ends after a newer `configure` (a generation
  counter) applies the newest settings instead of publishing its own result.
  `WakeManager::requirements(model)` (command `wake_requirements {model}`) lists what
  choosing any catalog or local model would download, without changing anything; the
  window's install dialog lists it after a refused join, and the join's preparation asks it
  in the realtime mode with the wake word (4.10, change `models-in-settings`).
  Every transition is published as `AppEvent::Wake(WakeStatus)` and visible in
  `Status.wake`. The manager implements `WakeFeed`; `feed` drops when the worker's queue is
  full and counts the drop.
- **Model list** (`WakeManager::models`, command `wake_models`): the catalog entries with
  whether each is downloaded and its size, then the local models, each with its default
  phrases (`default_activation`, `default_deactivation`; a local model's those of its
  folder's language). Sizes shown anywhere are decimal megabytes from the bytes
  (`wake::megabytes`).

- **`PhraseSpotter`** is engine-independent. One recognizer per speaker, one detector per
  phrase kind (activation, deactivation), whole-word matching on open-vocabulary text in the
  matching form. A phrase counts in a final result, or in 15 partials in a row for
  activation and 5 of the last 10 for deactivation. Each occurrence is reported once per
  utterance; the recognizer is never reset after a hit. An activation after a deactivation
  in the same utterance is ignored (so "отбой братан" never also counts as the activation
  "братан"); the 1.5 s guard after a deactivation is the gate's (4.3). The deactivation
  `Detector` counts the partial results that held its next unreported occurrence; a final
  result that confirms nothing new while that count is above 0 is a near miss
  (`take_near_miss`), which the worker logs.
- **Trailing silence, counted in receive ticks** (design D1). Discord stops sending audio
  when a user stops talking, and the recognizer needs silence to end the utterance. Every
  `Audio` message carries the guild's receive tick and the gate's `Instant`; a
  `WakeMessage::Tick` follows each tick's frames. `SpeakerState` keeps the tick of the
  speaker's last frame. On a `Tick`, `absent = tick - last_tick`: below
  `SILENCE_AFTER_TICKS` (15 ticks, 300 ms) nothing is fed; from 15 on, silence is fed until
  `min(absent, 75)` chunks of 20 ms (1.5 s) were fed, the owed chunks at once. A frame that
  comes back sooner continues the stream with nothing inserted (a gap of 3 ticks or more is
  counted as bridged). The reason is songbird's receive: a speaker whose playout buffer ran
  empty after a late packet is reported silent until 5 packets (100 ms) are buffered again,
  then the delayed audio plays; a late receive tick and a busy worker hold frames back the
  same way. The old rule (60 ms by the worker's own clock) put zeros into the word each
  time. A dropped `Tick` only delays the silence until the next one, since each carries its
  number. Wall time is used only to drop a speaker's recognizer after 30 s without a frame.
- A recognizer exists only while its speaker is talking and is dropped after 30 s idle. The
  whole engine is loaded only in wake-word mode.
- **Model catalog** (`catalog.rs`): vetted entries with engine, language, display name,
  download URLs, SHA-256, license and the model's **default phrases**
  (`default_activation`, `default_deactivation`: the language's slang for "bro", a slang
  farewell rare in game talk). The settings refer to a catalog id. A list the user never set
  uses the defaults (`catalog::default_phrases`); a local model takes the defaults of the
  catalog entry whose primary language subtag its folder name gives
  (`entry_for_language`), or none. The user's lists replace the defaults list by list, and
  an emptied list stays empty. The table of defaults is in `docs/user-guide.md`; every
  default word is checked against its model's vocabulary by
  `catalog_models_load_when_enabled` and in the corpus.
  Today ten small models, in the order the window lists them:
  `vosk-model-small-en-us-0.15` (the default of a new settings file),
  `vosk-model-small-ru-0.22`, then by language name `-cs-0.4-rhasspy` (MIT), `-nl-0.22`,
  `-fr-0.22`, `-de-0.15`, `-it-0.22`, `-pl-0.22`, `-pt-0.3` (Brazilian; Vosk's older flat
  layout, so its marker is `final.mdl`) and `-es-0.42`, all Apache 2.0 but Czech; plus the
  Vosk 0.3.45 Windows library. None ships with the app. Sizes and hashes were measured
  from a download (alphacephei publishes neither), and `catalog_models_load_when_enabled`
  loads every entry through the real library. Swedish (289 MB), Greek (1.1 GB only) and the
  EU languages without a Vosk model are left out; the models folder takes any other.
- **Local models** (`local.rs`): `models\` in the data directory holds Vosk models the user
  extracted for other languages. `WakeManager::scan_local_models` creates the folder and
  lists it once at start (a new folder needs a restart): every subfolder with
  `am\final.mdl` and `conf\mfcc.conf` is a model with the id `local:<folder>` and the
  language from a Vosk folder name (`vosk-model-small-de-0.15` → `de`); other folders, and
  names that cannot be an id, are skipped with a log line. `ModelSource::from_id` resolves a
  settings id (catalog first); a local model needs only the Vosk library, its directory is
  `models\<folder>`, and a missing folder is the engine's error, so its phrases stay stored.
  The app never downloads, changes or deletes anything in `models\`.
- **Asset store** (`crate::assets`, `assets.rs` at the crate root since change
  `whisper-mode`, design D10; `wake::assets` re-exports it). The wake word, Whisper and the
  voices (4.12) share it.
  - *An `Asset`*: the label, the official `page`, the URL, the SHA-256, its `directory`
    under `assets\`, a `marker` file that proves the install, the size, and a `kind`:
    `Archive` or `File` (a Whisper model). An archive may also have a `root`, the folder
    inside it whose contents become `directory` (`Release` of a whisper.cpp zip), and a
    `keep` list of paths relative to `root` with `*` for any characters (empty keeps
    everything). `Asset::ARCHIVE` holds the defaults of a plain archive.
  - *The file name* (`Asset::file_name`): for an archive the URL's last segment; for a
    single file its installed name, `marker`, so two models published as `ggml-model.bin`
    still differ. It is the name a download is saved under and the name a file downloaded
    by hand must have in `assets\`.
  - *`ensure`* has two routes to `assets\<file name>`. If that file is already there (one
    the user downloaded by hand), it is used. Otherwise `ensure` downloads with the system
    `curl.exe` to a `.part` name and renames. Either way it then reports
    `Progress::Unpacking` and verifies the SHA-256. A single file is moved to
    `assets\<directory>\<marker>` and is its own marker. A plain archive is extracted with
    the system `tar.exe` into `assets\`. An archive with a `root` or a `keep` list is
    extracted into a staging folder `assets\.unpack-<directory>`, its kept files are moved
    into `assets\<directory>`, and the rest is deleted, which is how the whisper.cpp zips
    shrink to the DLLs the app loads. The system `tar.exe` (bsdtar) also unpacks the
    `.tar.bz2` archives of sherpa-onnx, piper-phonemize and the voices, so no new crate is
    needed. The archive is then removed and the marker checked.
  - *A mismatch* deletes the file, installs nothing, and fails with "<file> (downloaded |
    found in the assets folder) does not match the pinned SHA-256 (<digest>); it was
    deleted". An installed folder with the marker counts as installed without the
    download (`is_installed`), but nothing checks its hash.
  - *Cleanup.* `.part` files and `.unpack-*` folders are removed at start. Download progress
    (`Progress::Downloaded`) goes to the event bus as `AppEvent::Assets {label, percent,
    error}` in steps of at least 20 %. No HTTP crate.
- Vosk is loaded at runtime through `libloading` (`LOAD_WITH_ALTERED_SEARCH_PATH`), so the
  app starts without the DLL. Partial word timestamps are not requested (they delay
  partials by seconds). Grammar-restricted recognition is not used (it maps ordinary speech
  onto the wake words).
- The worker logs every final result (a detail line), every phrase decision, and dropped
  audio (once per 10 s, as seconds of skipped speech). Phrases with a word the model does
  not know are not spotted; those words are listed in `WakeStatus.unknown_words` and in the
  "wake word ready" log line. A word counts as known in its lower-case form, its folded
  form ("всё" or "все"), or, written with an apostrophe or a hyphen, piece by piece
  (`spotter::word_known`). The diagnostic lines of design D5 are in section 7.
- Corpus tests: `wake::vosk::tests::vosk_corpus_when_assets_are_provided` runs only when
  `VOICEBRO_VOSK_DIR` points at an assets directory, with `VOICEBRO_WAKE_PHRASES`,
  `VOICEBRO_WAKE_WAV`, `VOICEBRO_WAKE_EXPECT`, `VOICEBRO_WAKE_SEQUENCES` and
  `VOICEBRO_WAKE_NOISE` describing the samples (16 kHz mono WAV). Without the variables the
  test passes without doing anything, so the suite stays runnable on any machine.
  `vosk_gaps_when_assets_are_provided` (same gate) feeds a WAV as a speaker whose frames go
  missing for 6 ticks every 0.5 s (songbird's refill), with zeros in the holes (the old
  worker), back to back, and through the ticks of design D1, in five phases; the D1 run must
  hear what the back-to-back run hears. It also measures the time from the end of a
  sentence to the deactivation, old worker against D1.

### 4.5 Assistant (`assistant/`)

```rust
pub trait Assistant: Send + Sync {
    fn id(&self) -> &'static str;                                 // "codex"
    fn attach(&self, guild: u64, ports: AudioPorts) -> Result<Arc<dyn AssistantSession>, AssistantError>;
    fn detach(&self, guild: u64);
    fn capabilities(&self) -> Capabilities;   // models with reasoning efforts, voices, auth state
    fn status(&self) -> AssistantStatus;
}

/// What the gate talks to. One per guild.
pub trait AssistantSession: Send + Sync {
    fn session_wanted(&self, wanted: bool);
    fn ready(&self) -> bool;
    fn call_context(&self, caller: Option<String>, participants: &[String]);
    fn speakers_changed(&self, names: &[String]);
    fn user_text(&self, _question: UserText) {}   // a question in words (the Whisper mode); a realtime session ignores it
    fn working(&self) -> bool { false }            // a turn runs or its voice plays: the idle timeout waits
}

/// A question Whisper heard (change `whisper-mode`), with what the latency line needs.
pub struct UserText {
    pub speaker: Option<String>, pub text: String, pub language: Option<String>,
    pub speech_end: Instant, pub text_at: Instant, pub audio_seconds: f64,
    pub model: String, pub device: String, pub latency: SharedLatency,
}

/// Audio in and out of the assistant, in 48 kHz stereo frames, and where its codec panics
/// and its agent's answers go.
pub struct AudioPorts {
    pub input: FrameReceiver,
    pub output: OutputHandle,
    pub failures: CodecFailures,
    pub answers: AnswerSink,
    pub heard: Arc<dyn HeardSink>,
}

/// The final answer of a task the agent completed, as written (change `post-assistant-answers`).
pub struct AgentAnswer { pub guild: u64, pub task: Option<String>, pub text: String }

/// The text of the people's speech the assistant's own recognizer heard, per turn: the
/// turn's text so far, or with `done` its final text (change `voice-recognition-rework`).
pub trait HeardSink: Send + Sync {
    fn on_heard(&self, guild: u64, turn: u64, text: &str, done: bool);
}
```

`AudioPorts` connect the gate and the assistant in process: the gate pushes gated
frames into `input`, the assistant pushes decoded answer audio into `output` (the
`OutputQueue` from 4.1) and can `flush` it. `Runtime::on_join` builds the ports and the gate,
calls `attach`, and gives the session to the gate; `on_leave` calls `detach`. In the Whisper
mode the gate pushes nothing into `input` and hands the session questions in words instead
(`user_text`, 4.3); the session attaches its own `FrameProvider` to `output`.

**The shared text session** (`assistant/text.rs`; change `claude-and-opencode-engines`,
design D2). The Whisper mode's session is one `TextSession` for every engine. It holds
what any engine needs: the speaker prefix, the `SentenceSplitter` into the `SpeechPlayer`,
barge-in, the one-speaker-at-a-time queue, the channel facts and history
(`TurnContext`), the chat entries, the answer posting and the Whisper latency line. It
talks to the engine only through `TurnBackend`:

```rust
#[async_trait]
pub trait TurnBackend: Send + Sync {
    fn engine(&self) -> AssistantEngine;               // name in the chat, log and errors
    fn bind(&self, sink: Weak<dyn TurnSink>);          // where TurnEvents go
    fn conversation_id(&self) -> Option<String>;       // the history goes once per conversation
    fn ready(&self) -> bool;
    async fn start(&self, text: String, context: TurnContext) -> Result<Option<String>, String>;
    async fn add(&self, turn: &str, text: String) -> Result<(), String>;  // refused: the question waits
    async fn interrupt(&self, turn: &str);
    fn detach(&self);
}
pub enum TurnEvent { Delta { turn, text }, Message { turn, text }, Chat { turn, kind, text },
                     Completed { turn, ok, detail }, Exited { message } }
```

Events of a turn the session no longer runs (interrupted, replaced) are ignored; the
first event of a turn whose `start` has not returned yet adopts its id. An engine with no
context field of its own puts the `TurnContext` before the question as text
(`TurnContext::prefixed`).

**Two kinds of Codex session** (change `whisper-mode`, design D6). `CodexAssistant::attach`
reads `assistant.mode` from its config (`CodexConfig.mode`) at each attach and builds a
`CodexSession` (the realtime call) or a `TextSession` over `CodexTurns`
(`text_session.rs`, below); its map of sessions holds an `Attached` enum of the two. Both
get their thread's notifications through `app_server::ThreadListener` (`on_notification`,
`on_declined`, `on_server_exit`), which `AppServer::map_thread` maps by thread id. The runtime hands the assistant its
`TtsManager` once (`Assistant::use_tts`, a no-op by default); an attach in the Whisper mode
without one fails. The join's preparation, the thread, the sign-in, the lists, the chat log,
the instructions and the answer posting are the same in both modes.

**The people's transcript to the gate** (change `voice-recognition-rework`, design D2):
`heard` is the `RelayRegistry` in the app (it routes to the guild's gate, 4.3) and
`assistant::no_heard()` for the stub, the probes and the tests. The Codex session keeps the
people's pieces of the current turn apart from the voice's: on each
`thread/realtime/transcript/delta` with `role: "user"` (the field that tells them apart in
the Codex CLI 0.167.0 schema; the voice's pieces have another role) it appends the
`delta` and hands the turn's joined text over; the people's `transcript/done` hands over its
final `text` with `done` and starts the next turn; a new call (`realtime/started`) starts
a new turn too. Nothing is handed over unless the call is `Live`, and always after the
session's locks are released, because the gate may call `session_wanted(false)` from
inside `on_heard`. The pieces come about 0.2 s after the audio, one per 200 ms window, and
split words wherever a window ends ("Брат" + "ан").

**Answers for Discord** (change `post-assistant-answers`): `AgentAnswer` and `AnswerSink`
are engine-independent (`assistant/mod.rs`). A session sends each completed task's final
answer through `ports.answers`; the runtime posts it (4.7, 4.2). `AnswerSink` is a clonable
sender over a bounded tokio channel of 16 (`ANSWER_QUEUE`) whose `send` never waits: a full
channel drops the new answer with a warning and counts it (`dropped()`). `AnswerSink::none()`
sends nowhere (tests, probes). `GateHost::on_join` gives every join a clone of the
runtime's one sink.

**Stub implementation** (`assistant/mod.rs`, `StubAssistant`): becomes ready a configurable
time after a session is wanted, drains its input on a thread, answers nothing and records
every call. It is what the tests use and what the runtime falls back to when no real
assistant is given, so the app never panics on an unknown engine setting.

**Codex implementation** (`assistant/codex/`), ChatGPT sign-in only:

- Finding Codex (`find_codex_in`, every status read and every start): the configured
  executable (only that one when set); else `codex.exe` on the PATH; else winget's
  command links folder `%LOCALAPPDATA%\Microsoft\WinGet\Links\codex.exe` (winget adds it to
  the PATH of new processes only, so a Codex installed while the app runs is found there);
  else npm's native exe under `%APPDATA%\npm`; else the `codex.cmd` shim.
- `app_server.rs`: spawns `codex app-server --disable shell_tool`, JSON-RPC over stdio with
  `experimentalApi: true`, request timeouts, a generation counter (thread ids die with the
  process), and automatic decline of every approval, tool call and user-input request.
  Account: `account/read`, `account/login/start` and `account/login/completed`. Models:
  `model/list` with their reasoning efforts. Voices: `thread/realtime/listVoices`. The
  status shows the running server's version, or else the found executable's: one
  `codex --version` per file, cached by its path, size and modification time
  (`CodexAssistant::executable_version`), so a status read starts no process unless
  Codex was replaced.
- `thread.rs`: one ephemeral, read-only thread per guild (`sandbox: read-only`,
  `approvalPolicy: "untrusted"`, so the agent's
  server-side tools such as web search run; client-side approval requests are still
  declined), `developerInstructions`, optional `model` and
  `config.model_reasoning_effort`). A changed model or effort opens a new thread and remaps
  the guild's session; history carries over through `initialItems`. A rejected model falls
  back to the account defaults.
- `rtc.rs`: the realtime call over WebRTC inside the process (`webrtc` 0.21 over its sans-io
  core `rtc`, decided by the live call probe on 2026-11-09): one Opus audio track (registered by hand: 48 kHz,
  2 channels in SDP, `minptime=10;useinbandfec=1`), one data channel named `oai-events`
  created before the offer, a non-trickle offer (ICE gathering waited for, at most 2 s),
  `thread/realtime/start` with `transport: {type: webrtc, sdp}` and `version: "v3"`, the
  answer from the `thread/realtime/sdp` notification. The `voice` field is sent only when
  `codex.voice` is set; empty means the server's default voice ("server default" in the
  log and chat).
  *Answer handoff* (`codex.handoff_mode`, `HandoffMode` in `session.rs`): how the Codex
  agent's answer reaches the voice model. `items` (the default) sends
  `codexResponsesAsItems: true`, `commentary` and `bem_tags` send
  `codexResponseHandoffMode` `commentary` and `bemTags`, `thinking` sends neither, which is
  Codex's built-in routing and what the app sent before. With the
  built-in routing the voice model sometimes answered with made-up facts before the agent
  had finished; with `items` it kept the agent's facts in every call of the 2026-11-10
  tests. The fields are experimental: if Codex refuses a start that carries one, the start
  is retried once without it, before the retry without history, and `log_used` names the
  requested mode, the built-in routing and Codex's error (also in the chat). The call's
  first chat line reads "Call starting: voice …; Codex: …; answers: <mode>". A changed mode
  applies from the next call.
  *Send:* the gate's frames wait in a FIFO of 5 frames (100 ms, drop-oldest, drops
  counted). The `voicebro-rtc-send` thread keeps a 20 ms clock and sends exactly one
  packet per tick: the oldest waiting frame, downmixed to mono and Opus-encoded
  (`rtc::call_params`: complexity 10, 32 kbit/s, in-band FEC, 5 % expected loss), or
  encoded silence when the FIFO is empty. The call's codec is `diagnostics.opus_codec` read
  from the Codex config when the call opens (`RtcCall::open(…, codec)`, logged "Opus codec
  for this call: <label>"); it encodes here and decodes in the jitter buffer for the whole
  call, and `CallStatus.opus_codec` carries its label while the call runs. A frame the codec
  fails on is sent as a silence packet encoded at the start and counted
  (`CallAudioCounters.encode_errors`). After the call connects and after every underrun it waits for 2 queued
  frames (40 ms) before sending speech again (`pacing.rs`, a pure function with its own
  tests), so a late songbird tick does not starve the next send tick.
  *Receive:* the remote track (its `on_track` fires with the first RTP packet) only inserts
  packets into the call's jitter buffer (`jitter.rs`, at most 50), keyed by extended
  sequence number.
  - **Packets** (`jitter.rs`): the buffer is attached to the output as its `FrameProvider`
    and is pulled by songbird's mixer thread. It does the following:
    - decodes contiguous packets up to 100 ms ahead;
    - decodes a missing packet from the next one's FEC, or conceals it, only when its turn
      has come (at most 5 frames in a row);
    - drops late packets;
    - tells a pause (the timestamps jump) from a late stream.
  - **Playout** (`playout.rs`, change `codex-adaptive-playout`): the server sends the voice
    unevenly and a few percent slower than real time, so the decoded samples go through an
    adaptive playout.
    - *Depth.* It follows the measured lateness: each packet's arrival against its RTP clock,
      relative to the earliest of the last 2 s, in a histogram of 20 ms buckets that forgets
      over about 15 s. The depth is the 95th percentile + 20 ms, clamped to 60–500 ms. No
      server rate is assumed.
    - *The start of a call.* A call starts from what the guild's previous call learnt
      (`CallReport::learned`; the assistant keeps one per guild for the app's run, across
      leaving and rejoining voice). The first call starts from a prior of 200 ms that fades
      like anything learnt: back to 60 ms after about 21 s of a steady server. The call's
      first log line, `[rtc] call opens with a voice buffer of N ms (initial | learnt in the
      previous call)`, names the start.
    - *Waiting for the depth.* Playback starts when that much audio is held, and again after
      a dry spell outside speech.
    - *Below depth − 20 ms.* A pause is lengthened by 20 ms (at most 500 ms without a new
      packet). Voiced speech is lengthened by one pitch period only when the level is under
      half the depth and no pause has come for 200 ms.
    - *Above depth + 60 ms.* A pause with at least 100 ms of it ahead is shortened by 20 ms,
      so at least 60 ms of it remain. Speech is shortened by one period only above twice the
      depth.
    - *Pitch.* The period comes from normalized correlation on a 4 kHz decimation (2.5–15
      ms), refined at 48 kHz, taking the shortest lag within 0.05 of the best correlation.
      Below a correlation of 0.5 there is no splice. Splices cross-fade over one period (5 ms
      for pauses), at most one voiced splice per 60 ms.
    - *Dry in speech.* Up to 3 frames are bridged with Opus concealment without consuming
      the sequence, so the late packet plays when it arrives. Silence follows, counted as a
      speech gap once the stream goes on.
    - *Memory and safety.* Every buffer is allocated when the call opens. The playout code
      has no `unwrap` and no unchecked indexing.
  *Counters* (`CallAudioCounters`): frames sent, input drops, input underruns, packets
  received, late, concealed (lost packets only), concealed by FEC, playout underruns, output
  drops, decode errors, pauses stretched, speech stretched, removed (ms), bridged frames,
  speech gaps, voice buffer depth now and at its highest (ms). They are also Performance tab
  stat cards. They are part of `CallStatus.audio` while a call runs (the last call's after it)
  and are logged in one `[rtc] call audio for guild …` line when the call closes. The
  `oai-events` channel carries the realtime server's JSON events (`session.started`,
  `input_transcript.added`, `turn.created`, `output_transcript.added`, with millisecond
  timing); they are logged and available for speech timing. The call exists only while a
  session is active. A ChatGPT sign-in allows realtime voice over WebRTC only; there is no
  websocket path.
- `conversation.rs`: continuity across calls for the app's lifetime. Same thread while the
  server generation matches, same `voiceSessionId`, last `realtimeSessionId`, transcript
  history as `initialItems` (v3 only, capped at 100 items or 24 KB), retried once without
  history if rejected. Instructions and call context (participants, who called) go into the
  **last** developer item of `initialItems`; the session `prompt` is ignored by the ChatGPT
  sign-in and is not relied on. Nothing is sent mid-call by default; the `changes`
  speaker-label mode sends a note only when the speaker changes.
- `assistant/chat_log.rs` (engine-independent): transcripts, call context, call start and
  end, errors, delegated agent work (`turn/*`, `item/completed`), unwrapped from
  `<realtime_delegation>`. Kept in memory (bounded at 600) and pushed as `AppEvent::Chat`;
  appended to `logs\codex-chat-YYYY-MM-DD.log` only while `assistant.save_chat_history` is
  on (`ChatLog::set_saving`, called from `CodexAssistant::configure`; off by default, so
  nothing of the chat reaches the disk). Files written earlier are left alone. Each entry
  is also a detail line (`detail::chat`) of the app log.
- `session.rs`: the state machine idle → starting → live → stopping. `session_wanted(true)`
  checks the sign-in, ensures the thread, opens the WebRTC call, sends
  `thread/realtime/start` with the history (retried once without it), applies the answer
  from `thread/realtime/sdp`, waits for the peer to connect and for
  `thread/realtime/started`. `ready()` is live and connected. A `closed` notification or a
  lost transport restarts the call while wanted, at most three times per session, with a
  backoff of 2 s times the attempt; the fourth failure sets the connection error. A stop
  closes the peer, sends `thread/realtime/stop` and flushes the output; a restart waits for
  the stop. Barge-in is left to the realtime server (it stops its own output when the user
  speaks); the app does not flush its output on the user's speech.
  *The agent's answers* (change `post-assistant-answers`): the session keeps the agent's
  turns in progress by turn id (`Turns`): `turn/started` (`turn.id`) opens one,
  `item/completed` (`turnId`; an unknown id counts for the most recent turn) sets its task
  from a `userMessage`, unwrapped as the chat shows it, and replaces its last text with each
  `agentMessage`. `turn/completed` closes it and, when `turn.status` is `completed` and the
  last text is not empty, sends `AgentAnswer { guild, task, text }` (the text trimmed, as
  written otherwise) through `ports.answers`. An interim line before a tool call is never
  sent, nor is an `interrupted` or `failed` turn. The voice's own speech and the people's
  transcripts are never answers. The turns are forgotten at `shutdown`. The `phase` field of
  an `agentMessage` (`commentary`, `final_answer`) is not used: the schema says it is not
  always sent. Checked against the Codex CLI 0.167.0 schema.
- `text_session.rs` (change `whisper-mode`, design D6): Codex's text turns, `CodexTurns`, the
  `TurnBackend` of the shared `TextSession` (above); what follows is the two together for
  Codex. It opens
  no call. `ready()` is true while the guild's thread exists and the app-server is alive,
  which is right after the join's preparation. It attaches a `SpeechPlayer` (4.12) to the
  output as its `FrameProvider` and detaches it at `shutdown`.
  - *A question* (`user_text`) becomes `turn/start { threadId, input: [{ type: "text", text
    }] }`, the text prefixed "<name>: " unless `speaker_labels` is `off`. The first turn of
    each session carries `additionalContext."voicebro.channel"` (`kind: "application"` with
    a `value` only: Codex CLI 0.167.0 refuses a `name` on application context; the value is
    facts only, "Discord voice channel." with the people in the channel and the caller),
    unless speaker labels are off. The first
    turn on a thread that has not had the history carries `voicebro.history`, the guild's
    earlier exchanges (`Conversation::history_text`, within the realtime history's 24 KB).
    The question also goes to the chat log and the conversation.
  - *Barge-in.* A question while a turn runs or its voice plays flushes the player at once
    and sends `turn/interrupt { threadId, turnId }` before the new `turn/start` (one
    `turn_lock`). The old turn id is stale, and its late notifications are ignored. A turn
    that started after its session ended is interrupted as soon as its id is known. This
    reacts to words Whisper heard, not to any sound. This is the behavior with
    `assistant.one_speaker_at_a_time` off.
  - *Questions wait their turn* (change `speaker-turns`, D6'; the setting on, read per
    question from `CodexConfig`). Nothing is interrupted or flushed. With a turn running,
    the question is added to `Running.added`, the log says "question added to turn <n>",
    and `steer_turn` waits for `turn_lock` (the turn's start has returned and its id is
    known), then sends `turn/steer { threadId, expectedTurnId, input }`. A refusal (Codex
    CLI 0.167.0: "no active turn to steer" once the turn has ended) moves the question to
    `TextState.waiting`, or starts it at once when no turn runs any more. With no turn
    running the question starts one (`begin_turn`), and its answer's sentences queue in the
    voice's FIFO behind the current answer's, under the same epoch. On `turn/completed` the
    waiting questions start the next turn together, joined by line breaks
    (`start_waiting`). The posted answer's task joins the turn's questions with " / ".
    Deactivation, mute, `/leave`, the idle timeout and a mode change still stop everything
    (`stop_running` also drops the waiting questions). Probe 2.2 of the change (live, five
    runs): every steer was accepted and the answer named both people and both answers.
  - *The answer.* Each `item/agentMessage/delta` of the running turn goes through the turn's
    `SentenceSplitter` into the voice queue; the first piece marks the turn on the player,
    and the first delta is "Codex first words" for the latency line. `item/completed` with
    an `agentMessage` sends the rest of that message to the voice and keeps its text; the
    item goes to the chat log. Every agent message is spoken, the interim line before a
    web search too. `turn/completed` with the status `completed` adds the last agent message
    to the conversation and sends `AgentAnswer { guild, task: question, text }` through
    `ports.answers`, so every completed answer can be posted (4.2, 4.7); another status
    adds a chat error and posts nothing.
  - *Ending.* `session_wanted(false)` (deactivation, idle timeout, mute) and `shutdown`
    (leave, mode switch) flush the player and interrupt the running turn; the call context
    goes again with the next session's first turn. `working()` is true while a turn runs or
    the player holds audio. An app-server exit sets the `codex` connection error "Codex
    app-server exited; run /join again".
  - *Model, reasoning, instructions* are the thread's (`codex.model`,
    `codex.reasoning_effort`, `codex.instructions`): in this mode they set how fast every
    answer comes. `codex.voice` and `codex.handoff_mode` are not used.
- `CodexAssistant` (`codex/mod.rs`) owns the process, the account, the voices and models,
  the conversations per guild and the chat log. **Nothing of Codex runs before `/join`,
  Sign in, Check or Refresh lists, except the read of the lists at start** (below).
  `prepare(guild)` (`Assistant::prepare`, awaited by the `/join` handler through
  `VoiceHost::prepare_join`) starts the app-server if it is not running, reads the account
  and opens or reuses the guild's thread, so the first activation only places the call. Its
  typed errors (`PrepareError`: not installed, did not start, not signed in, no thread)
  are the join's refusal reply. The app-server stops 0.5 s after the last guild detaches
  (the conversations stay in memory: the next join opens a new thread and the history
  carries over through `initialItems`), one minute after an on-demand Sign in or Check
  while the bot is in no channel (re-armed while a sign-in waits for
  `account/login/completed`, at most ten minutes), and at app exit (`stop` kills the child
  after stopping every call). A preparation or a newer stop cancels a pending stop.
- **Saved lists** (change `config-tab-and-codex-lists`, `codex/lists.rs`): every
  successful `model/list` that returns at least one model replaces the models held, marks
  the lists as read in this run (`lists_fresh`), sets the read time and writes
  `codex-lists.json` in the data folder (`SavedLists`: `version` 1, `read_at_ms`,
  `codex_version`, `models`, `voices`, `default_voice`; a temporary file renamed over the
  old one) with the voices held at that moment. An empty answer or a failed read changes
  neither the lists held nor the file. `CodexAssistant::new` loads the file (a file with no
  model, or one that does not parse, counts as none; what the load found, the saved read's
  time or the parse warning, is logged by `read_lists_at_start`, because the assistant is
  built before the desktop shell's log is up), so
  `capabilities()` offers the saved models and voices, with `lists_read_at_ms`, before any
  process runs. Check (`check()`, behind Check Codex, Refresh lists and the read at start)
  reads the voices and the models every time (`refresh_voices(force)`); a join preparation
  reads both when they were not read in this run, so the first join after a start without a
  read renews the saved lists and later joins reuse them.
- **The read at start**: `read_lists_at_start(enabled)`, called by the desktop shell right
  after `Runtime::start` with `codex.refresh_lists_at_start`. Without an executable it does
  nothing (no error; the status already says "codex not found"). With one it spawns
  `check()` when the setting is on or no lists were saved, and logs "Codex: reading the
  lists at start (the setting is on | no saved lists)"; otherwise nothing starts. `check()`
  schedules the stop a minute later like any Check, and a `/join` meanwhile reuses the
  server and cancels that stop.
- Tests: `testdata/fake-codex` is a Node stand-in for the app-server (handshake, account,
  voices, two models, `thread/start`; `fake_codex_signed_out.cmd` reports no account;
  `--log <file>` records each request's method, `--models a,b|none` sets the model list;
  its own method `fake/delegate {threadId, outcome}` plays a delegated task on that thread:
  `turn/started`, the task, an interim agent message, a web search, the final agent message
  and `turn/completed` with the given status). For the text turns of the Whisper mode,
  `turn/start` answers with the turn and streams an interim and a final agent message as
  `item/agentMessage/delta` pieces, each item completed, then `turn/completed`;
  `turn/interrupt` ends the running turn as `interrupted`; `fake/turnDelay {ms}` spaces the
  pieces and `fake/lastTurnStart` returns the last `turn/start`'s params. The text-session
  tests check one question answered, spoken and posted with the channel context, a second
  question that interrupts the first (the interrupt before the new start, only the second
  posted), and a deactivation during a turn.
  The tests that use it check nothing where Node is not installed.
  `VOICEBRO_CODEX_SMOKE=1 cargo test -p voicebro-core real_codex_starts` runs the real
  start at a preparation and the stop after the last leave.
- Live check: `VOICEBRO_CODEX_CALL_WAV=<q1.wav> [VOICEBRO_CODEX_EXPECT1="a|b"
  VOICEBRO_CODEX_CALL_WAV2=<q2.wav> VOICEBRO_CODEX_EXPECT2="a|b"
  VOICEBRO_CODEX_CALL_WAV3=<q3.wav>] cargo test -p voicebro-core live_call_when_enabled --
  --nocapture` runs a real call through the engine without Discord: the question frames
  are fed on a jittery tick (20 ms ± 7 ms), the output is read every 20 ms like songbird
  does, and the test checks the canary, audio back, zero input drops, continuity and prints
  the call's counters and the chat log (a delegated question shows the agent's web search).
  The session gets its own answer receiver: every answer handed over for posting is
  printed, and with `VOICEBRO_CODEX_CALL_WAV3` the test requires one answer per completed
  task, the last one holding the agent's final text as the chat shows it.
  `VOICEBRO_CODEX_SMOKE=1` runs the handshake only. `VOICEBRO_RTC_LOOPBACK=1 cargo test -p
  voicebro-core loopback_call_loses_nothing` sends a 10 s tone between two peers in one
  process through the same send path and jitter buffer and checks zero drops, zero late,
  zero concealed frames and the correlation both ways.
- Live check of the Whisper mode: `whisper_text_turn_when_enabled`, gated by
  `VOICEBRO_WHISPER_DIR` (an assets folder with the Whisper files and the voice) and
  `VOICEBRO_WHISPER_QUESTION_WAV` (16 or 48 kHz), with `VOICEBRO_WHISPER_MODEL`,
  `VOICEBRO_WHISPER_DEVICE`, `VOICEBRO_TTS_VOICE`, `VOICEBRO_WHISPER_PEOPLE` (the channel's
  people, the first as the caller, for the `additionalContext` canary) and
  `VOICEBRO_WHISPER_ANSWER_WAV` optional: the WAV goes through Whisper, a real Codex text
  turn and the voice, the output is read every 20 ms like songbird does, the answer is
  written as a WAV, and the latency line prints.

In the realtime mode the thread's `model` and reasoning effort affect only work the voice
model delegates to the Codex agent; the voice itself is the realtime model. In the Whisper
mode they answer every question. The UI says so next to the settings.

**Claude Code implementation** (`assistant/claude/`, change `claude-and-opencode-engines`,
designs D4 and D5; checked against Claude Code 2.1.317). The Whisper mode only; the app runs
the user's unmodified `claude.exe` and never reads, stores or passes a Claude credential.

- *Finding it* (`find_claude_in`): `claude.executable` when set, and only that file; else
  `claude.exe` on the PATH, winget's `%LOCALAPPDATA%\Microsoft\WinGet\Links`, then
  `%USERPROFILE%\.local\bin` (Claude Code's own installer). `claude --version` is cached per
  file by size and time.
- *The sign-in* (`read_sign_in`, `parse_auth_status`): `claude auth status` prints JSON
  without a model call (`loggedIn`, `authMethod`, `apiProvider`, `subscriptionType`, and for
  claude.ai `email`, `orgId`, `orgName`). `claude.ai` (or an OAuth method) is a
  subscription with its plan, Team and Enterprise included; `api_key` an API key; a
  `bedrock`, `vertex` or `foundry` provider a cloud provider; `loggedIn: false` none;
  anything else unknown. It is read at Check, at a join's preparation and when the window
  picks Claude Code, with the turns' environment, so an `ANTHROPIC_API_KEY` that wins over
  the subscription shows as the key. `AssistantStatus.sign_in_kind` carries the kind
  (`subscription`, `api_key`, `cloud`, `none`, `unknown`) for the window's notice.
- *The process* (`process.rs`): one per guild, started at `prepare` in the empty
  `claude-workspace\` with `claude_args`: `-p --input-format stream-json --output-format
  stream-json --verbose --include-partial-messages --permission-mode dontAsk
  --strict-mcp-config --setting-sources "" --no-session-persistence --tools
  WebSearch,WebFetch --allowedTools WebSearch,WebFetch --mcp-config claude\mcp.json
  [--model m] [--append-system-prompt-file claude\instructions.txt]`. `--tools` makes the
  two web tools the only ones offered, `--allowedTools` allows them in advance: in the
  don't-ask mode an offered tool that is not allowed is denied at its first use (Claude Code
  2.1.317, found in the user's live test on 2026-11-20). The instruction file holds
  `assistant.instructions` (none when empty); `mcp.json` is `{"mcpServers":{}}`. `--bare`
  is never used: it accepts only an API key. When the app itself runs inside a Claude Code
  session (`CLAUDECODE` set, development only), every `CLAUDE*`, `ANTHROPIC*` and `OTEL_*`
  variable is removed for the child. The `initialize` control request, answered without a
  model call, gives the models. A `control_request` from Claude Code is refused. The
  process stops 0.5 s after the guild's leave (a per-guild epoch lets a new join cancel
  it), at an engine switch (detach) and at `stop`. A model or instruction change applies
  from the next process, which is the next join.
- *Turns* (`turns.rs`, `ClaudeTurns`, the `TurnBackend`): a question is one `user` line
  with the `TurnContext` before it as text. Claude Code has no turn ids and answers the
  lines in order, so the backend numbers its turns and gives every message to the oldest
  turn without a `result`: `stream_event` `text_delta` → `Delta`, each `assistant` text
  block → `Message` and an `agent` chat line, a `tool_use` → a `tool` chat line ("Web
  search: …", "Web fetch: …"), a `tool_result` with `is_error` in a `user` message →
  a `tool` chat line "<tool> failed: <first line>", `result` → `Completed` (`success` and not `is_error`;
  `aborted_streaming` reads "interrupted"). `interrupt` is a `control_request` with subtype
  `interrupt`, sent only for the running turn. `add` is refused ("Claude Code answers one
  question at a time"), so with one speaker at a time the question waits and starts the
  next turn. The process ending is `Exited` ("Claude Code stopped; run /join again", the
  `claude` connection error). The conversation id is the process's number, so a new
  process gets the guild's history in its first question.
- Tests: `testdata/fake-claude` (Node) answers `--version`, `auth status` (`--kind=api_key`,
  `--kind=none`), stream-json turns with deltas, a web search (denied, as the real CLI denies
  it, in the don't-ask mode without `--allowedTools`) and a `result`, a queue, and
  the control requests `initialize`, `interrupt` and its own `fake_delay`,
  `fake_last_user` and `fake_exit`; `--fake-log` records the arguments and every stdin
  line. Live: `VOICEBRO_CLAUDE_LIVE=1 cargo test -p voicebro-core
  claude_text_turn_when_enabled -- --nocapture` (Claude Code of this PC, `haiku` unless
  `VOICEBRO_CLAUDE_MODEL`), and `probe claude-smoke`.

**OpenCode implementation** (`assistant/opencode/`, change `claude-and-opencode-engines`,
designs D7 and D8). The Whisper mode only, against the V2 server API of OpenCode 2.x as read
from its source at tag v2.3.2; `probe opencode-smoke` checks it live. The app runs the
user's `opencode.exe`; OpenCode keeps its own providers and keys, which the app never reads.

- *Finding it* (`find_opencode_in`): `opencode.executable` when set, and only that file;
  else `opencode.exe` on the PATH, winget's `%LOCALAPPDATA%\Microsoft\WinGet\Links`,
  winget's own folder of the package (`WinGet\Packages\Anomaly.OpenCode_*`: the 2.3.2
  package adds that folder to the user's PATH and makes no link, so an app started before
  the install would not find it on its PATH), then npm's native program under `%APPDATA%\npm\node_modules\opencode-ai\node_modules\
  opencode-windows-x64[-baseline]\bin` (never npm's `opencode.cmd` shim, which would leave
  the server running when the app ends it).
- *The configuration* (`config_document`, `server_env`): the app writes
  `opencode\opencode.json` next to its executable at every server start: an agent
  `voicebro` (`mode: primary`, the default agent) whose permissions deny every action but
  `webfetch` and `websearch`, its `system` the user's `assistant.instructions` (none when
  empty), `share: disabled`, `update: disable`, and `websearch.provider` set, so a search
  never waits on a question: the first provider whose key variable is set (`EXA_API_KEY`,
  `TAVILY_API_KEY`, `FIRECRAWL_API_KEY`, `PARALLEL_API_KEY`, `TINYFISH_API_KEY`), else `exa`
  without a key. The server gets `OPENCODE_CONFIG` (that file),
  `OPENCODE_DISABLE_CLAUDE_CODE`, `OPENCODE_DISABLE_PROJECT_CONFIG` and
  `OPENCODE_DISABLE_AUTOUPDATE`, so no other agent's instructions or skills and no project
  file reach the model.
- *The server* (`server.rs`, `OpencodeServer`): `opencode serve --hostname 127.0.0.1 --port
  <a free port>` in the empty `opencode-workspace\`, with `OPENCODE_PASSWORD` a random
  password made for that start; every call is HTTP Basic with the user `opencode`. Ready is
  `GET /api/info` answering within 20 s; a start that does not become ready is tried once
  more on another port. `GET /api/event` (server-sent events `{id, created, type, data}`) is
  read while the server runs and opened again a second after it ends; each event goes to
  the listener of its `data.sessionID` (`*` hears every event whole, for the probe). One
  server serves every guild: it starts at the first join's preparation (or at Check, which
  stops it after reading when no guild uses it), and stops 0.5 s after the last leave (an
  epoch lets a new join cancel that), at an engine switch and at `stop`.
- *Preparing a join* (`prepare_guild`): the providers (`GET /api/provider`, by id, a
  disabled one left out; `/api/integration` lists only integrations from configuration
  files and is empty with a stored sign-in), the models (`GET /api/model`, as
  `providerID/id`, each once: several entries share a `modelID`) and the default (`GET
  /api/model/default`, with any provider often one of OpenCode's own free models) are
  read; the join is refused when OpenCode offers no model, or not
  the one `opencode.model` names ("run `opencode auth login` …"). A guild without a session
  gets one: `POST /api/session` with the title "VoiceBro <guild>", the agent, the working
  folder and the chosen model. The leave deletes it (`DELETE /api/session/{id}`).
- *Turns* (`turns.rs`, `OpencodeTurns`, the `TurnBackend`): a question is `POST
  /api/session/{id}/prompt` with the `TurnContext` before it and `delivery: "queue"`, after
  a model chosen since (`POST /api/session/{id}/model`), so a model change applies from the
  next question. The turn runs from `session.execution.started` to `.succeeded`, `.failed`
  or `.interrupted`; `session.text.delta` → `Delta`, `session.text.ended` → `Message` and an
  `agent` chat line, both without OpenAI's citation marks (`\u{e200}cite…\u{e201}`) or any
  other private-use character (`strip_citations`, which keeps an open mark across the
  stream's pieces); `session.tool.input.started {id, name}` names a call, and
  `session.tool.called {id, input}` becomes a `tool` chat line ("Web search: <query>"), a
  `.failed` or `.error` one "<tool> failed: <message>". `add` sends the question
  with `delivery: "steer"` while the execution runs, so with one speaker at a time it joins
  the running answer; with no execution it is refused and the question starts the next
  turn. `interrupt` posts `/api/session/{id}/interrupt` and waits up to 3 s for the
  execution's end before the next question goes out. The server ending is `Exited`
  ("OpenCode stopped; run /join again", the `opencode` connection error).
- Check (`opencode_check`) returns `{providers, models, default_model}`; the status summary
  names the providers, and the models fill the Model list.
- Tests: `testdata/fake-opencode` (Node) serves those routes with Basic auth (401 without
  it), executions with deltas, a web search and an end, interrupts, `queue` against `steer`,
  and `FAKE_OPENCODE_PROVIDERS`, `FAKE_OPENCODE_MODELS` and `FAKE_OPENCODE_FAIL_FIRST`; its
  own `/fake/delay`, `/fake/state` and `/fake/exit`. Live: `probe opencode-smoke`
  (`OPENCODE_EXECUTABLE` names another program), and `VOICEBRO_OPENCODE_LIVE=1 cargo test
  -p voicebro-core opencode_text_turn_when_enabled -- --nocapture` (OpenCode of this PC,
  its default model unless `VOICEBRO_OPENCODE_MODEL`).

### 4.6 Settings (`settings/`)

One schema, one place to validate, one place to apply.

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub version: u32,                      // schema version, migrated on load
    pub discord: DiscordSettings,          // guild_id (an older auto_connect is ignored and dropped at the next save); access: everyone | agreed | owner, whose voice passes (4.13), default everyone (a file without it takes owner from owner_only: true of builds up to 1.1.0, everyone otherwise; owner_only is read only, never written); privacy_url: empty or https:// with a host, at most 512 characters, stored trimmed; access and privacy_url are their own `owner` part in changed_sections
    pub session: SessionSettings,          // mode: always | wake_word; idle_timeout_seconds (0 = off, else 5..=3600); earcons, earcon_volume
    pub audio: AudioSettings,              // input_gain, output_gain: 0.0..=4.0, default 1.0 (a file without the section gets the defaults)
    pub wake: WakeSettings,                // engine id, model id (catalog id or local:<folder>), phrases: { <model id>: { activation?, deactivation? } } (an absent list: the model's defaults)
    pub assistant: AssistantSettings,      // engine: codex (default) | claude | opencode (its own `engine` part; realtime is refused with claude or opencode), mode: realtime (default) | whisper (its own `mode` part in changed_sections), speaker_labels: start | changes | off, instructions (the user's, for every engine, no default; its own `instructions` part; moved from codex.instructions at load), save_chat_history (default false), post_answers: off (default) | voice_channel | command_channel | channel, answers_channel (a channel id, digits or empty; required by channel, kept while another value is chosen)
    pub codex: CodexSettings,              // executable (empty = auto-detect), model, reasoning_effort, voice (empty = server default, the default), handoff_mode: items (default) | commentary | bem_tags | thinking, refresh_lists_at_start (default false, read at start)
    pub claude: ClaudeSettings,            // executable (empty = found automatically), model (empty = Claude Code's default; no spaces, at most 200 characters), subscription_notice_shown (default false; the window's one-time dialog, 4.5)
    pub opencode: OpencodeSettings,        // executable (empty = found automatically), model (empty = OpenCode's default, else provider/model; the model part may hold more /)
    pub hotkey: HotkeySettings,            // mute: shortcut string, empty disables it
    pub ui: UiSettings,                    // theme: dark | darcula | light | intellij (default dark); font_size: 11..=20 (default 13); collapsed_cards: ids of the collapsed cards of the Settings and Config tabs (default empty; unknown ids kept, ignored by the window)
    pub diagnostics: DiagnosticsSettings,  // performance, memory, verbose_log: all default false, read by the window and the desktop shell; opus_codec: libopus (default) | opus-rs, read by the core at each join and each call
    pub whisper: WhisperSettings,          // the Whisper mode's recognition: model (catalog id or local:<file>, default whisper-base-q8_0), device: cpu (default) | gpu, threads: 0 (automatic) ..= 64, language: auto (default) | an ISO 639-1 code Whisper knows, phrases: { activation?, deactivation? } (an absent list: "bro" or "peace out", whatever the language, 4.12)
    pub tts: TtsSettings,                  // the Whisper mode's voice: voice (catalog id or windows:<token>, default supertonic3:F1), speed: 0.5..=2.0, default 1.0
    pub extra: Map<String, Value>,         // flattened: unknown fields from a newer version, kept as they are
}
```

Every string in the project (UI, log, notes to the model, code, docs) is English. The wake
phrases are each catalog model's defaults (4.4) or the user's own in the settings file, in
whatever language the user's model recognizes; the Whisper mode's phrases default to "bro"
and "peace out" whatever the language (4.12). The code ships no assistant instruction.

- Stored as JSON at `settings.json` next to the executable. Written to a temp file and
  renamed over the old one. Unknown fields are kept; missing fields take defaults.
- Schema version 2 keeps the wake phrases per model: `wake.phrases` maps a model id to its
  `activation` and `deactivation` lists, and only the selected model's lists are spotted.
  `ModelPhrases` holds `Option<Vec<String>>` lists (change `voice-recognition-rework`,
  design D7): a list absent from the file (`None`, never set) means the model's default
  phrases, a present list, even empty, is the user's. Serde skips `None` when writing, so a
  save that names one list writes only that one and the other keeps its default; the
  defaults are never written into the file. `WakeSettings::selected_phrases()` and
  `phrases_of(model)` return the lists in effect (`PhraseLists`). An entry written by an
  earlier build has both lists present and stays the user's. Phrases are stored as typed.
  Loading a version-1 file runs `store::migrate_v1` on the JSON first. When it has a
  non-empty phrase list, its `activation_phrases` and `deactivation_phrases` move under the
  model the file names (or `vosk-model-small-ru-0.22`, the only model of version 1, which
  is then written into `wake.model`), both of them, an empty one too, so those phrases keep
  working and a list the user left empty stays empty. When it has none,
  `wake.model` becomes the default of a new file (English): version 1 only ever had the
  Russian model, so the model it names was never the user's choice. The old fields are
  removed either way, and the next save writes version 2.
- A stored `diagnostics.opus_codec` that names no codec of this build (`ropus` or
  `mousiki`, saved by a trial build of the four-codec comparison) becomes `libopus` when
  the file loads (`DiagnosticsSettings::replace_unknown_codec`, called by
  `SettingsStore::load`), with the log line "settings: the stored Opus codec <value> is not
  in this build; libopus is used", so the file loads and later saves pass validation; the
  next save writes `libopus`. A save that sends such a name is still refused.
- `Settings::validate()` is the only validator (one model's phrase lists in effect, the
  defaults of a list never set included, must not share a phrase in the matching form of
  4.4, so "Братан всё" against "братан  все" is refused naming the model and both lists,
  timeout range, the model is a catalog id or `local:<plain folder name>` of the engine,
  shortcut parses, `assistant.answers_channel` is digits and set when `post_answers` is
  `channel`). An unknown `assistant.post_answers` is refused while the document is read
  (`PostAnswers` deserializes through `TryFrom<String>`), with the field and the four
  values in the message. `Runtime::apply(settings)` diffs against the
  current settings and reconfigures only the subsystems whose section changed.
  `changed_sections` reports the two answer posting fields as their own part, `answers`,
  so a change of only those restarts nothing and reconfigures no gate or assistant: the
  answer handler (4.7) reads them when each answer arrives.
- **The Whisper mode's settings** (change `whisper-mode`, design D14). An unknown
  `assistant.mode` or `whisper.device` is refused while the document is read
  (`TryFrom<String>`), naming the field and the values. `validate()` refuses a
  `whisper.model` that is neither a catalog id nor `local:<plain file name>` (no `\`, `/`,
  `:` or `..`), `whisper.threads` above 64, a `whisper.language` Whisper does not know, a
  Whisper phrase with no letter or digit, the two Whisper lists in effect sharing a phrase
  in the matching form (so "Goodbye!" against "goodbye" is refused), a `tts.voice` that is
  neither a catalog voice nor `windows:<token>`, and a `tts.speed` outside 0.5 to 2.0. A
  local model's file and a Windows voice are checked when they load, not at save.
  `changed_sections` reports `mode` (the mode switches at once, 4.7), `whisper` and `tts`
  as parts of their own; the mode is not part of `assistant`. A 1.0.x file loads in the
  realtime mode with the new sections at their defaults; the schema version is unchanged,
  and an older build keeps the new sections as unknown fields.
- **The engines' settings** (change `claude-and-opencode-engines`, design D11).
  `assistant.engine` is an `AssistantEngine` read through `TryFrom<String>`: an unknown
  value is refused while the document is read, naming the field and the three values.
  `validate()` refuses `assistant.mode = realtime` with Claude Code or OpenCode ("the voice
  mode works only with Codex"), a `claude.model` with spaces or over 200 characters, and an
  `opencode.model` without a provider part and a model part around its first `/`.
  `changed_sections` reports `engine` (switched at once on the mode switch's path, 4.7),
  `instructions`, `claude` and `opencode` as parts of their own. `store::
  migrate_codex_instructions` runs on every load: a `codex.instructions` of builds up to
  1.2.0 moves to `assistant.instructions` when that is missing, and is removed either way;
  an older build reading a newer file finds no instruction. The schema version is
  unchanged.
- Adding a setting means: a field with a default, its validation, its `apply` effect, and its
  UI control. Nothing else.
- **Secrets** go through `SecretStore` (Windows Credential Manager via `windows-sys`, entry
  `VoiceBro/discord_token`). The UI writes a secret with one command and only ever reads back
  `is_set: bool`. Secrets never appear in `Settings`, in the status snapshot, in events or in
  the log. Codex's own ChatGPT sign-in is stored by Codex itself (`~/.codex`), not by the app.
- The app is portable: its data directory is the folder the executable runs from
  (`VOICEBRO_DATA_DIR` overrides it for tests). It holds `settings.json`,
  `codex-lists.json` (the Codex models and voices of the last read, 4.5), `logs\`, `assets\`
  (what the app downloaded: the Vosk library and models, the Whisper runtimes, models and
  VAD model, the voice engine, the phonemizer and the voices), `models\` (Vosk models the
  user added, only read), `whisper-models\` (Whisper models the user added, only read),
  `codex-workspace\` (the empty, read-only cwd for Codex threads) and `webview-data\` (the window's WebView2 data, cache and component downloads,
  tens of MB). The app creates files only there. Outside it are only the Discord token in
  the Windows Credential Manager (written by the app), the user's own Codex data in
  `~\.codex` (written by Codex) and, when Codex was installed through the app, Codex itself
  where winget puts it. Builds before change `ui-layout-port` left a
  `%LOCALAPPDATA%\com.voicebro.desktop` folder (WebView2 data) that the user may delete.
- **The folder must be writable.** The first thing `main()` of the desktop app does is
  `check_writable(data_dir)`: create the folder if needed, write and remove
  `.voicebro-write-test`. On failure a `MessageBoxW` names the folder and says to install
  or extract VoiceBro into a folder the user can write to, and the process exits with
  code 1 before logging, settings or a window exist (a copy under Program Files, for
  example).

### 4.7 Composition root (`runtime.rs`)

`Runtime` owns the registry of gates, the Discord adapter, the wake engine, the assistant,
the settings store and the event bus. It is the only place that knows every subsystem.
Start-up: load settings, read secrets, start Discord whenever a token is set, load the wake
engine if the mode is wake-word and the assistant mode is realtime. The assistant's process is not started: it starts at the
first `/join` (`GateHost::prepare_join` calls `Assistant::prepare`). The one exception is
the read of the Codex lists at start (4.5), which the desktop shell starts right after
`Runtime::start` when `codex.refresh_lists_at_start` is on or no lists were saved, and only
when Codex is installed. Shutdown: stop the assistant (and its child process), leave every call, stop
the wake worker, unload Whisper and the voice. `Runtime::toggle_mute` is the single entry for all three mute sources.

**The Whisper mode** (change `whisper-mode`, designs D1 and D12). The runtime owns a
`WhisperManager`, whose transcripts go to the registry, and a `TtsManager`, which it also
hands to the assistant (`use_tts`); both live in the gate host (4.12).

- *At start* Whisper scans `whisper-models\`, and both are configured: in either mode they
  report missing files, download nothing and load nothing.
- *The feed.* Every gate's feed is a `ModeFeed`, which hands each frame and tick to the wake
  manager and to Whisper. Each takes them only while it runs: Vosk only in the realtime mode
  with the wake word, Whisper only while a model is loaded.
- *Join.* `GateHost::prepare_join` first checks the wake word's files (change
  `models-in-settings`): in the realtime mode with the wake word, a selected model or Vosk
  library not on disk (`WakeManager::requirements`, the disk only) refuses the join before
  the assistant starts, with "The wake word needs downloads (<labels>): open VoiceBro →
  Config and press Download under Wake model, then run /join again"
  (`wake::join_refusal`, its start `JOIN_NEEDS_WAKE_FILES`); always listening and the
  Whisper mode do not need Vosk. It then awaits the assistant's preparation, and in the
  Whisper mode `prepare_whisper`: `WhisperManager::prepare_join` and
  `TtsManager::prepare_join` on a blocking thread. Missing files refuse the join with "The
  Whisper mode needs downloads: open VoiceBro → Settings and press Download in the Whisper
  and Local voice cards, then run /join again" (`JOIN_NEEDS_DOWNLOADS`); a load error
  refuses it with the error. Either refusal is logged as "join to guild <id> refused:
  <reply>", and the window opens its install dialog when it sees one of the two starts in
  that line (4.8).
- *Leave.* `on_leave` detaches the assistant and drops what Whisper cut or queued for the
  guild. After the last leave it calls `release` on both, which unload 0.5 s later unless
  the bot joined again first.
- *Settings.* `mode` configures the assistant, the wake manager (Vosk off or on), Whisper
  and the voice, then switches every gate (below); when the bot is in a channel and the new
  mode is Whisper, both `prepare_join` on a thread of their own, and missing files wait for
  the window's OK. `whisper` configures Whisper (a model or device change reloads in the
  background while the old model serves; language, threads and phrases apply to the next
  utterance) and the gates (the phrases). `tts` configures the voice (the speed at once, a
  voice change reloads in the background).
- *The switch at once* (`switch_mode`): for every gate the assistant is detached and the
  output flushed; the falling tone plays when a session was active and earcons are on. A new
  gate in the new mode is built on the same output, with the old gate's speaker names and
  participants, registered, and the assistant attached in the new mode. The bot stays in
  the channel. The log says "guild <g>: the assistant switched to the <realtime | whisper>
  mode", and the panels are refreshed.
- `assistant_mode()` names the mode for `/status` and the panel (4.2).
- *Shutdown* configures both with the realtime mode, which unloads them.

**The engine switch** (change `claude-and-opencode-engines`, design D1). The desktop shell
builds every engine at start (each starts nothing until used) and hands the runtime one
`EngineSwitch` (`assistant/switch.rs`) as its `Assistant`. It forwards `prepare`, `attach`,
`status` and `capabilities` to the engine `assistant.engine` names, `configure`, `stop` and
`use_tts` to all of them, and `detach` to the engine the guild was attached to, whatever is
selected by then; an engine missing from the build refuses the join with "<product> is not
available in this build". `changed_sections` reports `engine` as its own part. `apply`
configures the switch (which selects the engine), moves the consent records to the new
recipient (4.13), and then, instead of `switch_mode`, rebuilds every gate with
`rebuild_gates` (the same steps: the old session detached from its engine, the falling tone,
a new gate on the same output) and spawns `switch_engine`, which awaits the new engine's
`prepare` for each guild before attaching it. A guild whose preparation fails stays in its
channel with a gate and no session, and the `assistant` connection error says "<product>
could not start for this channel: <reason>". The old engine's process stops as after its
last leave. `VoiceHost::engine_name` gives `/status` and the panel the engine's name.

**Codec failures** (change `opus-codec-experiment`): `Runtime::start` spawns a task that
drains the `CodecFailureHub` (4.1). For each report `handle_codec_failure` logs
"<path>: <codec> panicked: <message>; the bot leaves the voice channel <channel> in
<guild>", leaves that guild's
voice channel as `/leave` does (`VoiceControl`, which `DiscordRuntime` implements with its
status and `leave`), writes the console line "the bot left <channel> in <guild>: <codec>
failed while <activity>" and publishes `AppEvent::CodecFailure { guild_name, channel_name,
path, activity, codec, message }`. Other guilds stay; `diagnostics.opus_codec` is not
changed. A report for a guild the bot is no longer in is logged and dropped.

**The agent's answers** (change `post-assistant-answers`): `Runtime::start` takes the
receiver of the one `AnswerSink` (4.5) and spawns `spawn_answer_handler`, which takes the
answers one after another. For each it reads the `assistant` settings stored at that moment
(`Runtime.settings` is shared with the task), so a save applies to the next answer and
restarts nothing (`changed_sections` reports the two fields as `answers`, which `apply`
only logs). `handle_answer` maps `post_answers` to an `AnswerDestination` (`off` makes no
call and logs "not posted: posting answers is off") and calls `AnswerPoster::post`, a trait
implemented by `DiscordRuntime` (4.2) and faked in the runtime tests. Its `PostOutcome`:
`Posted` logs the post line and clears the `answers` connection error; `NotPosted` logs the
reason (unless it was already said in this voice session); `Refused` sets the `answers`
error to "cannot post to #<channel> in <server>: <reason>" (the Status tab shows it as
"answers: cannot post …") and logs it at Warn. Nothing is retried.

### 4.8 Desktop shell (`apps/desktop`)

- Tauri commands are thin: validate input, call `Runtime`, return serializable snapshots.
  Errors cross the boundary as `String`.
- One event bridge task forwards `AppEvent` to the webview.
- **Panic hook** (`install_panic_hook`, change `opus-codec-experiment`): installed first
  thing in `main` once the data folder is known. It logs every panic as "panic in thread
  '<name>' at <file:line:col>: <message>" (and to stderr). A panic inside a guarded opus-rs
  call (`codec::in_guarded_call()`) is left to the codec's `catch_unwind` (4.1). Any other
  panic flushes the log, shows a Windows message box "VoiceBro stopped because of an
  internal error" with the message, the location and the log folder, and calls
  `std::process::abort()` once it is closed, so outside the codec a release build still
  ends at the first panic as it did with `panic = "abort"` (exit code `0xC0000409`). A
  debug build started with `VOICEBRO_TEST_PANIC=task` panics in a background task 3 s
  after the setup, to check the hook in the real app.
- **Links open in the default browser.** Every link in the window has
  `target="_blank"`, so following it in any way (a click, a modifier or middle click,
  Enter, the context menu) is a WebView2 new-window request. The main window's builder
  has one `on_new_window` handler (`hand_link_to_browser`): an `http` or `https` URL goes
  to the default browser through `rundll32 url.dll,FileProtocolHandler` and is logged,
  any other address opens nothing and is logged, and the request is always denied, so the
  window never shows a web page or opens a window of its own. The page has no click
  handler for links. That is the one rule the user asked for on 2026-11-12: no list of
  allowed pages, no second guard.
- **Global hotkey** through `tauri-plugin-global-shortcut`: registered at start-up and again
  when a saved settings document changes `hotkey.mute`, the previous shortcut unregistered
  first; an empty value registers nothing; toggle only, on the key-down edge; the outcome
  goes to `Runtime::set_hotkey_status` and the status shows `{shortcut, registered, error}`.
- `tauri-plugin-log` in every build: the app at Info, every other crate at Warn,
  songbird's receive task at Error, local time, 1 MB per file, one old file kept, under the
  data directory, with the file filter of section 7 (detail lines only in a debug build or
  with the verbose log). A second target (`TargetKind::Dispatch`, `log_ring.rs`) gets
  every record that passes the levels, keeps the last 500 lines and emits each as
  `app-log`; the
  `log_history` command returns the ring, so the window's log pane shows the whole app log
  (what the recognizer heard, every phrase decision) including what was logged before the
  window loaded. The pane no longer lists `AppEvent::Log` separately.
- `build.rs` embeds the Common-Controls v6 manifest into every linked target and stamps the
  build (version plus short git hash plus date) for the UI.
- **The main window is built in `setup`** (`build_main_window`, label `main`) with
  `WebviewWindowBuilder::data_directory(<data dir>\webview-data)`; `tauri.conf.json` lists no
  window, because its `dataDirectory` is relative to `%APPDATA%`.
- UI: static HTML, CSS and JS with `withGlobalTauri: true`, no framework or bundler. The
  window has a tab bar with **Console,
  Codex chat, Status,
  Performance, System, Settings, Config** (the Whisper mode has no tab of its own since
  change `whisper-config-layout`: its fields are on Config and Settings), cards, and a
  status bar (Discord state in colour,
  guild count, voice channel count, uptime, a memory slot while memory measurement is on,
  the build stamp). Switching tabs
  keeps each tab's content and scroll position. Four themes (Dark, Darcula, Light,
  IntelliJ) as CSS variables on `html[data-theme]` and a font size, both from the `ui`
  settings section, applied at once and at load.
  - Console is a read-only text area with the app log
    (`log_history` at load, then each `app-log` record; at most 2 000 lines in the view),
    search that filters the lines and shows the match count, Clear (the view only), Copy
    (the shown lines), Copy Selection and Auto-scroll (off keeps the view where the user
    scrolled).
  - Status renders from one `status` snapshot (`renderStatus`): the connected guilds on the
    left (`DiscordStatus.guilds`, read from the serenity cache), and fact tiles on the right:
    Discord state, bot account, voice channel, people in it, assistant engine, assistant
    mode, the gates (session state, assistant ready, last phrase, held frames), wake word,
    in the Whisper mode only Whisper (state and model), Whisper device (in use, with the
    fallback reason) and Local voice, then mute, Codex account, call, Codex thread, mute
    hotkey, token stored, build; then connection errors
    and the counters behind a details toggle. Events trigger a refresh; a 5 s timer backs
    it up. Buttons: Mute, Connect, Disconnect, Sign in to Codex, Check Codex, Refresh.
    When the status has no Codex executable, the Codex area shows the install offer
    (`codex_install.rs`): `codex_install_info` says whether winget is on the PC (its App
    Execution Alias in `%LOCALAPPDATA%\Microsoft\WindowsApps`, found with
    `symlink_metadata`, or `winget.exe` on the PATH), the package, the source and the
    sizes, and the install state (idle, running, finished with winget's exit code, failed
    to start). `codex_install` starts `cmd.exe /V:ON /C "winget install --id OpenAI.Codex
    --exact --source winget & … & pause & exit !rc!"` in a new console window the user
    sees, accepts no agreement for them, refuses a second start while one runs, and
    emits an `app-event` of kind `codex_install` with the final state; the finder then
    sees winget's link on the next status read. Without winget the notice gives the npm
    and the manual ways instead of the button. With Codex found and no account, the area
    names the next step, Sign in to Codex.
  - Performance: a window selector (30 s, 60 s, 5 min), Refresh, a health dot and a
    quality label, two canvas charts drawn with the page's own `drawChart` (no library), and stat cards, all behind the "Performance monitoring"
    switch in the tab's toolbar, which is the `diagnostics.performance` setting (off by
    default; the Diagnostics card of the Settings tab holds a second control of the same
    setting). While it is on the window samples the status once a second (the same poll
    refreshes the status values) and keeps five minutes; while it is off the window reads
    the status every 5 s and on events, keeps no samples, and the tab shows the switch and
    a note on what the monitoring costs. Turning it off drops the kept samples. Quality: "idle"
    without a connected call; "stable" when, over the chosen window, late plus concealed
    packets stay under 1 % of those received and no input frame was dropped; otherwise
    "degraded" naming the counter (dropped input frames, concealed packets, late packets).
    "Audio timing" plots each measured answer (first speech to playback, last speech to
    answer); "Media packets" plots packets per second sent to and received from the call.
    The cards are numbers from the status: Discord ticks and frames, the call's counters,
    frames the assistant missed, wake chunks dropped, both levels and clip counts, the
    number of answers measured and the last answer's four durations.
  - System shows `system_status` (version, build stamp, process id, uptime, runtime,
    platform with the Windows build, architecture, CPU cores), the memory (the "Native
    host" card: a bar and the private working set of the Rust process, WebView2 and Codex,
    "not running" while Codex is stopped, and the total, which the status bar shows too;
    while `diagnostics.memory` is off the card says the measurement is off and where to
    turn it on, the status bar has no memory item, and the window does not call
    `memory_status`) and the files (data folder, `settings.json`, the Codex CLI path).
  - Settings holds the window's own settings (change `config-tab-and-codex-lists`): the
    cards Appearance and Diagnostics (Performance monitoring, Memory measurement and
    Verbose log, `diagnostics.verbose_log`, and the
    "Opus codec (experiment)" list, `diagnostics.opus_codec`, with a line under it,
    `#opus-codec-in-use`, that `renderStatus` fills from `VoiceChannelStatus.opus_codec` and
    `CallStatus.opus_codec`: "In use: Discord <label>; call <label>", or none while the bot
    is in no channel; it is not a form field), then Whisper (how Whisper runs on this PC,
    change `whisper-config-layout`; below). The bot's cards (Access, Audio, Assistant, Local
    voice, Codex with the "Save chat history" switch, `assistant.save_chat_history`, and
    Hotkey) are on the Config tab, below "Discord and Codex", and behave as described here: the form code
    finds every control by `data-path`, whatever its tab. There is
    one control per field with its JSON path (`data-path`) and kind (string, number, bool,
    list, percent), and under every control a help line saying what the setting affects,
    when a change applies (at once, from the next call, from the next session, at the next
    connection) and the default. Each control saves itself on `change` with one
    `save_settings` patch (text when leaving the field or on Enter, sliders on release,
    selects and switches at once). A save changes no geometry: the field's name takes the
    success colour for two seconds and the feedback line of the control's tab
    (`.form-feedback[data-autosave]`: `#settings-feedback` on Settings,
    `#config-autosave-feedback` at the bottom of Config; none on Performance) reads
    "Saved: <field>" for the same time, then its `data-autosave` text again; "Open models
    folder" and "Open assets folder" write their result there for six seconds. The line
    under the field holds the validation message only, which
    stays, with the rejected value, until the field is corrected. **Switches**: every
    `data-kind="bool"` control is a native checkbox with `role="switch"` and class `switch`,
    drawn by `styles.css` as a 46 × 26 px track (`appearance: none`, the knob a `::before`,
    the accent colour when on, a 2 px focus outline), followed by its "On"/"Off" text
    (`#<id>-label`, written by `percentLabel`) inside the field's `<label>`, so the switch,
    its text and the field's name all toggle it and Space works as on any checkbox. The
    Earcons field (`label.beside-slider`) moves its switch down by a slider's value line so
    it lines up with the Earcon volume slider next to it. Two controls may share a
    `data-path` (the Performance tab's switch and the
    Diagnostics card): a save from one fills the other at once. Lists: the wake model comes
    from `wake_models` (the catalog with its download state, then the local models; the
    wake engine has no control since change `whisper-config-layout`: Vosk is its only value,
    and the grey Recognition field shows it), each model labelled "<language> — <name> · downloaded | to download, N MB |
    models folder" and listed again when a download ends. The two phrase fields carry
    `data-phrases` (activation, deactivation) instead of a fixed path: they read and save
    `wake.phrases.<selected model>.<list>`, built as a key array because model ids contain
    dots, show the selected model's name in their labels, and are filled again from the
    stored settings when another model is saved. Under the wake model control, the
    download notice renders from `status.wake.state`: `needs_download` (with or without
    phrases) lists each file with its size and site, the download-by-hand part and a
    Download button (`wake_download {model}`), `downloading` shows the progress,
    `installing` shows "Checking and unpacking the <label>…", and an error after a
    download shows the message with the part and the button again; next to it "Open
    models folder" (`open_models_folder`). The download-by-hand part (`renderManual`, the
    same in the notice and the dialog) lists each missing file's archive name (`file`)
    with a link to its `page`, an "Open assets folder" button (`open_assets_folder`) and a
    line on putting the `.zip` files into that folder and pressing Download. It is hidden
    while the state is `downloading` or `installing`, and after a failure it is filled
    again from `wake_requirements`, so it lists only what is still missing. Choosing a wake
    model saves it at once, whatever it misses (change `models-in-settings`, D3); the notice
    then lists its files with Download. While the install dialog is open the list cannot
    change (a change from a script or a keyboard path goes back to the stored model). The Codex model, reasoning effort and
    voice are selects filled from `capabilities` when the engine has lists, read in this
    run or saved from the last read (`codex-lists.json`, 4.5), and text inputs with a
    datalist only while none was ever read; the effort list follows the chosen model. The Codex card's
    "Answer handoff" list (`codex.handoff_mode`) offers Conversation items (recommended),
    Commentary, Tagged channels and Built-in (thinking); the hint under it says what the
    chosen mode does and how it behaved in the 2026-11-10 tests. Phrases are one
    per line or comma-separated and are shown trimmed and lower-cased after save; the words
    the model does not know are listed under the field. The Audio group has sliders of kind
    `percent` (shown 0 to 400 %, stored as the multiplier): "What the assistant hears",
    "How loud the assistant speaks" and the tone volume, saved on release without a Save
    button, with the earcon switch, and under each gain the last second's peak and the clip
    count from `Status.audio`.
  - **Install dialog** (`<dialog id="wake-install">`, opened with `showModal()`): one modal
    dialog for the wake word's missing files, armed by `start()` once the model list and
    the settings are loaded. Since change `models-in-settings` (D4) it opens only for a
    `/join` refused because files are missing: the core logs "join to guild <id> refused:
    <reply>", the `log` event carries the line, and a line holding "The wake word needs
    downloads" makes the window ask `wake_requirements` for the stored model and open the
    dialog with the reason `join` (in `data-reason`). Never at the window's start (the
    user's decision of 2026-11-12), at a switch of the mode or the activation, or for a
    choice (the user's decision of 2026-11-20): a choice saves at once and its notice lists
    the files. It names the model and each missing file with its size
    and site, the total, and the download-by-hand part. Download calls `wake_download`
    ("Starting…", not "the download", since files from the folder are not downloaded) and
    shows the progress in the dialog (one button, Hide): "Downloading the <label>: N %"
    and "Checking and unpacking the <label>…". It closes when the state leaves
    `downloading` or `installing` for `ready`, `no_phrases` or `loading`, and shows an
    error with Try again and Close. Not now, Hide, Close and Escape store nothing: the
    state stays `needs_download`, so only another refused join opens it again, and the
    card's notice stays. Its note says to run `/join` again once the files are here: the
    bot does not join by itself after the download.
    `apps/desktop/ui-dev/check-wake-dialog.mjs` runs all of this against the mock.
    **The Whisper subject** (changes `whisper-mode` and `whisper-config-layout`): the same
    dialog, with `data-subject="whisper"` and the title "The Whisper mode is not
    installed", lists every file the selected Whisper model, device and voice need
    (`whisper_requirements {model, device, voice}`: Whisper's files, then the voice's, each
    file once), always with the total, and the download-by-hand part. It opens only after
    a `/join` refused for them in the Whisper mode (`join`: the refusal's log line, holding
    "The Whisper mode needs downloads", seen within 5 s), and waits while the Claude
    subscription dialog is open. Never because the window started, the mode was switched or
    a Whisper model, device or voice was picked (change `models-in-settings`, D4): each
    choice saves at once in both modes and its card shows the notice. Download calls both
    `whisper_download {model}` and `tts_download {voice}`, which give each manager its
    consent, and the progress follows `status.whisper` and `status.tts`.
  - **Codec failure notice** (`<dialog id="codec-failure">`, change
    `opus-codec-experiment`): opened with `showModal()` by `AppEvent::CodecFailure`
    (`showCodecFailure`), in the install dialog's style with the error colour as its border.
    Title "The bot left the voice channel"; one item per failure, "<codec> failed while
    <activity>: “<message>”. The bot left <channel> in <server>."; then the hint "Run /join
    to bring the bot back, or choose libopus in Settings → Diagnostics → Opus codec. The
    log has the details." A failure while the dialog is open adds an item to it. "Open log
    folder" calls `open_log_folder` and keeps the dialog open; OK and Escape close it and
    empty the list. The mock's `__mock.codecFailure(where)` sends the event.
  - **Form state is kept apart from the status.** `main.js` holds the stored settings
    (from `get_settings`, refreshed on `settings_changed` and returned by each
    `save_settings`) and a state per field: clean, dirty (unsaved), saving or error
    (rejected). Status refreshes (events and the 5 s timer) never write a form field; a
    field is written from the stored settings only when it is clean, not focused and, for
    a list, not open (a press or focus marks a select busy until a choice or blur). Lists
    are built when their source changes: the wake catalog once, the Codex lists when
    `capabilities` differ from the last ones; a rebuild that would hit an open list waits
    for it to close and keeps the selection.
  - Config holds the bot's settings. Its first card, "Discord and Codex", has the Discord
    token (write-only, with help on getting one, a stored badge and
    Clear stored token), the guild id (with help on copying it) and the Codex executable,
    an "Auto-start enabled" badge, Save changes (one settings patch for the edited fields
    plus `set_secret` when a token was typed) and Reset (back to the stored values); its
    fields (`data-config-path`) are the only ones that wait for Save changes. Below it come
    the Access, Audio, Assistant, Codex (the engine card) and Hotkey cards, which save each change at once, then the
    tab's feedback line and one line with the path of `settings.json` (`#config-path`),
    outside every card. The tab never shows the contents of the settings file.
  - **Access** (Config, after "Discord and Codex"; changes `owner-only-access` and
    `voice-consent`, 4.11 and 4.13): the list "Who the assistant hears" (`#access-mode`,
    `discord.access`: Everyone, People who agreed (/optin), Only my Discord account) and the
    field "Privacy policy URL" (`#privacy-url`, `discord.privacy_url`; a value the settings
    reject is not saved and the field shows the error), both saved on change; then the link
    part and the list "People who agreed", which are status and not form. `renderAccessNotes`
    redraws two warnings from the stored settings and the link: `owner` with nothing linked
    (the assistant hears nobody), `agreed` with no URL (`/optin` is refused). `renderConsent`
    redraws the list from `Status.consent` (the `consent` event refreshes the status): one
    row per record with the name, "agreed <local time>" and Remove (`consent_remove`, which
    returns the records left), "Nobody yet" when empty, and the unreadable reason. `renderOwner`
    redraws it from `Status.owner` on every status (the `owner` event refreshes the status).
    It shows the owner as "<name> (<id>)" or "not linked" with `link_ended` as the reason;
    while a code waits, `/link <code>` and a countdown from `expires_at_ms` (redrawn every
    second, its own timer stopped when no code waits) with Cancel; otherwise Link (or "Link
    another account") and, when linked, Unlink. A code that ended without a link shows
    `code_ended` as a sentence ("The code was cancelled."); a code past its time shows "The
    code expired." Link, Cancel and Unlink call `owner_link_start` (Codex's
    account read as Check, then a code; its error shows under the buttons),
    `owner_link_cancel` and `owner_unlink`, disable the three buttons meanwhile and refresh
    the status.
  - **Collapsible cards** (Settings and Config): each of the ten cards has `data-card`
    (`appearance`, `diagnostics`, `whisper`, `local-voice` on Settings; `discord`, `access`,
    `audio`, `assistant`, `codex`, `hotkey` on Config; the ids that change
    `whisper-mode`'s removed cards may have stored are kept and ignored), its title inside a `<button class=
    "card-toggle" aria-expanded aria-controls>` with a CSS chevron, and its body in
    `.card-body`, which `hidden` hides; "Discord and Codex" keeps its badge in the title
    row. A click, Enter or Space toggles the card at once and saves the whole list with one
    `save_settings` patch `{ ui: { collapsed_cards } }`: the ids this window does not know
    first, as stored, then its own collapsed ids. It is not a form field (no saved mark,
    no feedback line). `fillForm` applies the stored list on load and on every settings
    change, except while a toggle's save is in flight, so quick toggles do not undo each
    other. Fields keep their values and unsaved edits while their card is folded.
  - **Post answers to Discord** (Config → Codex, after "Save chat history"; change
    `post-assistant-answers`): a select `#post-answers` (`assistant.post_answers`) with Off,
    The voice channel's chat, The channel of /join and A chosen channel; its help line says
    that the agent's final answers are posted there and names View Channel, Send Messages
    and Embed Links. Below it, only while A chosen channel is shown, "Answers channel"
    (`#answers-channel`, no `data-path`): the text channels from `discord_text_channels`
    (4.2), one `<optgroup>` per server, "#<name>", and "(can't post)" after a channel
    whose `can_post` is `false`. Saving: Off and the two chats save `post_answers` at once
    through the normal path and keep `answers_channel`; A chosen channel saves both fields
    in one patch (`saveAnswerPosting`), at once when a channel is stored, else only when one
    is picked; until then the select stays dirty, nothing is saved, and the line "Pick a
    channel: nothing is saved until you do." shows. Picking a channel saves both. While the
    command returns no servers (not connected), the list holds the stored id ("Channel
    <id>") and the note says the names appear once the bot is connected; a stored id the
    connected bot does not know is listed as "<id> (not in the bot's servers)". The list is
    read when the Config tab opens and whenever `renderStatus` sees the Discord connection
    state change; a rebuild keeps the stored (or the in-flight) choice and waits while the
    list is open. A refused post shows on the Status tab as the `answers` connection error.
  - **Codex lists** (Config → Codex, first row): the "Refresh the lists at start" switch
    (`codex.refresh_lists_at_start`, read by the desktop shell at the next start) and the
    "Lists" field: a **Refresh lists** button and `#codex-lists-read`, "Read <YYYY-MM-DD
    HH:MM>" in local time from `Capabilities.lists_read_at_ms`, or "Never read". The button
    calls `codex_check` (the same command as Check Codex: account, voices and models, saved
    to `codex-lists.json`), then `capabilities`; meanwhile it is disabled and the line reads
    "Reading…"; a failure leaves the lists and shows "Not read: <error>" in the error
    colour. `loadCapabilities` writes the read time on every load (the start read and a
    join publish an `assistant` event, which loads them) and rebuilds the lists only when
    the models or voices changed, keeping the stored choice.
  - **The Whisper mode's fields** (change `whisper-mode`, laid out by change
    `whisper-config-layout`, designs D3 to D5, and moved by change `models-in-settings`,
    D1 and D2). There is no tab of their own: the bot's behaviour is on Config, the models,
    the voice and how Whisper runs on this PC are on Settings. Every field saves each change
    through the normal path, with its help line and validation message; a model, device or
    voice that misses files saves at once too, in both modes, and no dialog opens.
    - *Config → Assistant* follows the mode and swaps rows, not data: each row writes only
      its own path, and the other mode's rows are hidden, not disabled.
      - "Assistant mode" (`assistant.mode`: Realtime voice or Whisper) is the first field;
        its help line says what runs on the PC and what leaves it in each mode, that
        `codex.model` and `codex.reasoning_effort` set how fast each answer comes in the
        Whisper mode, and that Whisper and the voice are set in Settings.
      - In the Whisper mode, under the mode, the note `#whisper-missing`
        (`renderWhisperMissing`, from `status.whisper.missing` and `status.tts.missing`,
        never the form) names what is not on this PC ("Whisper's files for the model
        <name>", "the voice <name>"), says that `/join` is refused until then, and holds
        "Open Settings": it shows the Settings tab, unfolds the first card with missing
        files (Whisper, else Local voice; through `toggleCard`, so the stored list follows)
        and scrolls its notice into view. It is hidden in the realtime mode and once nothing
        is missing.
      - "Recognition" is a grey read-only field, "Vosk" or "Whisper", which follows the
        mode. It replaces the wake engine select.
      - The realtime mode shows the wake model row (`wake.model`) with its notice and the
        selected Vosk model's phrase fields, as above.
      - The Whisper mode shows, in the same place, a second phrase pair bound to
        `whisper.phrases` (commas or line breaks; a list never set shows "bro" or "peace
        out"). The help line says which set the fields hold.
      - Activation (`session.mode`) and the auto-stop stay below.
    - *Settings → Whisper* (`whisper`, after Diagnostics): an "On" or "Off" label that
      follows `assistant.mode` and says to switch it in Config → Assistant; the Whisper
      model (`whisper.model`, from `whisper_models`, grouped as Multilingual, English only,
      Russian-tuned and Your models, each with its size, note and download state); Device
      (CPU or NVIDIA GPU (CUDA)), CPU threads and Language (`whisper_languages`: Automatic,
      the ten wake catalog languages, then the rest of Whisper's list); one notice
      (`#whisper-download`) listing every file the selected model misses for the selected
      device (runtime, model, VAD model) with the total and Download
      (`whisper_download {model}`), in both modes; the in-use line from `status.whisper`
      and `status.tts` (what is loaded, the device or "GPU requested, CPU used: <reason>",
      the last answer's four timings); and "Open Whisper models folder"
      (`open_whisper_models_folder`).
    - *Settings → Local voice* (`local-voice`, after Whisper) works in both modes. A note on
      top follows the mode: in the realtime mode the local voice is not used, because the
      Codex call speaks with its own voice, and it can be downloaded now; in the Whisper mode
      this voice reads Codex's answers into the channel. Then Voice (`tts.voice`, from
      `tts_voices`, grouped by engine and language with the size and "downloaded", "to
      download" or "built into Windows", and a line with the data license and the
      phonemizer), Speed (`tts.speed`, a slider of kind `factor` shown as "1.25×", saved on
      release), Preview (`tts_preview {voice, speed}` returns WAV bytes, played in an
      `<audio>` element in the window only) and the voice's notice with Download
      (`tts_download {voice}`).

    The notices render from `status.whisper.missing` and `status.tts.missing`, which the
    core fills in both modes, and the progress from their states. Each Download fetches
    only what its notice lists; a Download in the realtime mode installs the files and
    loads nothing. The Codex card's Voice and Answer handoff help lines say "realtime mode
    only".
  - Codex chat shows the chat log entries (pulled at start,
    then one `chat` event per entry, at most 600 rows), one row each with the time, a
    coloured source label and the text. Labels: the speaker's name for a user transcript,
    `GPT` (the voice answer), `Context → GPT`, `Task → Codex`, `Codex`, `Codex thinking`,
    `Codex tool`, `Call`, `Error`; the voice answer and delegated tasks are highlighted.
    Filters for names and notes, agent work and call events, Auto-scroll, Copy (one
    `[time] label: text` line per shown row) and Open log folder.
  - Commands: `get_settings`, `save_settings`, `set_secret`, `clear_secret`, `status`,
    `app_info`, `set_muted`, `toggle_mute`, `discord_connect`, `discord_disconnect`,
    `codex_sign_in`, `codex_check`, `codex_install_info`, `codex_install`,
    `codex_models`, `owner_link_start`, `owner_link_cancel`, `owner_unlink`, `capabilities`,
    `discord_text_channels`, `wake_models`, `wake_download`, `wake_requirements`,
    `whisper_models`, `tts_voices`, `whisper_requirements`, `whisper_download`,
    `tts_download`, `tts_preview`, `whisper_languages`, `open_whisper_models_folder`,
    `chat_log`, `log_history`, `system_status`, `memory_status`, `open_log_folder`,
    `open_models_folder`, `open_assets_folder`. `whisper_download {model}` fetches
    Whisper's files and `tts_download {voice}` the voice's, in either mode.
    `tts_preview` runs on a blocking thread: it loads a voice of its own for the sample.
- Checking the window without a person: start the app with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`; the page is then
  reachable through the Chrome DevTools protocol (evaluate expressions, take screenshots).
- Checking the UI without the app: `apps/desktop/ui-dev/` (outside `frontendDist`, never
  shipped) holds `mock.js`, a stand-in for every command the page calls and its events
  (an in-memory settings document with the app's validation messages for the timeout,
  phrase overlap, gains, theme and font size; status events every second;
  `window.__mock` with the calls made, the stored document, a capabilities change,
  `failNextCheck` and `nextCheckAddsModel`; `?nolists` starts with no Codex lists and
  `?collapsed=<ids>` with those cards stored as collapsed; `discord_text_channels` answers
  two servers with "rules" marked can't post, `?disconnected` starts with the bot not
  connected and `setConnected(on)` changes it, `?post_answers=<value>&answers_channel=<id>`
  starts with that answer posting stored, `?owner=linked` with Alex linked,
  `?access=agreed|owner` (or the older `?owner_only`) with that access mode,
  `?privacy=<url>` with that policy URL and `?consents=<name,name>` with those people
  recorded; `completeLink(name)`, `wrongCode()` and `expireCode()` act as Discord and the
  clock on a waiting code, `owner()` returns the link state, `optin(name)` and `optout(id)`
  act as a person's "I agree" and `/optout`, `consentUnreadable()` as an unreadable records
  file, and `consent()` returns the records; for the
  Whisper mode the catalogs as the app lists them, the `missing` files in the statuses of
  both modes, `tts_download`, `?whisper` (the Whisper mode with its files on disk),
  `?whisper_missing` (nothing downloaded) and `?gpu_fallback` (Whisper loaded in a channel,
  the GPU asked for and the CPU used, one measured answer), a refused `join()` with files
  missing, and `failNextWhisperDownload`, `completeWhisperDownload`, `placeWhisperFiles`,
  `setCuda` and `whisperTurn`; a `join()` in the realtime mode with the wake word is refused
  too while the Vosk files are missing, with the core's reply in its log line),
  `make.mjs`, which writes `ui-dev/index.html` from `ui/index.html` with the mock loaded
  first, and `serve.mjs`, which serves it on `http://127.0.0.1:5174/` for browsers that do
  not run scripts from `file://`. Four headless checks drive the mock in Edge and stop at
  the first failure: `check-wake-dialog.mjs` (the install dialog at a refused join, the
  wake model's choice and notice),
  `check-config-tab.mjs` (which cards each tab holds, the two ways of saving on Config,
  the collapsible cards of both tabs, the switches' size, label and keyboard, the Codex lists and
  Refresh lists, and Post answers to Discord with its channel list),
  `check-whisper-config.mjs` (the Whisper mode's fields on Config and Settings: the rows of
  each mode, the Settings → Whisper and Local voice cards, the notices and Download in both
  modes, the Config note with Open Settings, and that only a refused join opens the
  install dialog's Whisper subject) and `check-engine-config.mjs` (the engines).
- Window lifecycle: destroying the main window exits the app, which stops `codex.exe`.

### 4.9 Probe binary (`apps/probe`)

A headless dev binary over `voicebro-core`: Codex handshake and account check, WebRTC call
with a WAV, Vosk corpus runs, Credential Manager round trip, timer jitter, memory
measurement, and for the Whisper mode `whisper`, `whisper-corpus` and `tts` (4.12). Not
shipped. It exists so that every external API is probed live before code
is built on it.

### 4.10 Distribution (`apps/desktop/tauri.conf.json`, `scripts/`)

- **Installer:** Tauri's NSIS template with `installMode: currentUser` (no elevation,
  `%LOCALAPPDATA%\VoiceBro` by default, the folder selectable; per-machine is not offered
  because Program Files is not writable for the app), English and Russian by the system
  language without a selector, a `VoiceBro` Start menu folder, the desktop shortcut on the
  last page, no license page. `LICENSE` (as `LICENSE.txt`) and
  `THIRD-PARTY-NOTICES.txt` are installed next to `voicebro.exe` as resources.
- **Icon:** `bundle.icon` names `icons/icon.ico`, and that one file is the program's icon
  resource (embedded by `tauri-build` as group `#32512`), the default window icon (on
  Windows Tauri 2.12 compiles no image into the binary: `generate_context!` calls
  `default_window_icon_from_app_icon_resource`, which loads `#32512` at run time at the
  system's large icon size; the main window sets none of its own) and, through
  `bundle.windows.nsis.installerIcon` and `uninstallerIcon` (without them NSIS uses its own
  default icon), the installer's and uninstaller's icon; the shortcuts show the program's.
  Tauri hands the window icon to tao, which sets only `ICON_SMALL`: the title bar shows it,
  and the taskbar button, with no `ICON_BIG`, falls back to that icon or the program's
  resource. The file holds 16, 24, 32, 48, 64 and 256 px, PNG-compressed, generated from
  the kept source `icons/app-icon.png` (1024 px) by `cargo tauri icon icons\app-icon.png
  -o <scratch folder>` run in `apps/desktop`; only `icon.ico` of that output is kept, the
  macOS, iOS, Android and Store images are not used. The source's fully transparent pixels
  hold the edge colour instead of black, so a scaler without premultiplied alpha draws no
  dark rim. The image is the project author's (made with ChatGPT) and ships under the
  project's license, with no third-party notice. The Windows shell caches an executable's
  icon by its path, and an update writes the new `voicebro.exe` over the old one, so
  `NSIS_HOOK_POSTINSTALL` sends `SHChangeNotify` twice after every install:
  `SHCNE_UPDATEITEM` for `$INSTDIR\voicebro.exe` and `SHCNE_ASSOCCHANGED`, both flushed.
  Without them Explorer, the shortcuts and the taskbar keep the icon of the earlier build.
- **Update and uninstall:** an install over an older one replaces only the bundled files,
  so the data files (`owner-link.bin` and `consents.bin` among them) stay.
  `apps/desktop/installer-hooks.nsh` defines `NSIS_HOOK_POSTINSTALL` (the installer
  language for the uninstaller, the icon notice above) and `NSIS_HOOK_POSTUNINSTALL`:
  only when "Delete the application data" is checked and not in update mode, it removes
  `logs`, `assets` (the Whisper and voice downloads too), `models`, `whisper-models`,
  `codex-workspace`, `webview-data`, `settings.json`, `owner-link.bin`, `consents.bin` (with
  their `.tmp` files), runs
  `cmdkey /delete:VoiceBro/discord_token`, and removes the then empty install folder.
  Tauri's own deletion of `%APPDATA%` / `%LOCALAPPDATA%\com.voicebro.desktop` stays.
- **Release script** `scripts\build-release.ps1`: checks that `tauri.conf.json` and
  `apps/desktop/Cargo.toml` agree on the version; without `-Trial` it refuses unless on
  `main`, with a clean tree and the tag `v<version>` on `HEAD`. It writes the notices,
  runs `cargo tauri build`, and puts into `target\release\dist\` (`dist-trial\` with
  `-trial` names) the installer, `VoiceBro_<version>_x64-portable.zip` (`voicebro.exe`,
  `README.md`, `docs\user-guide.md` with `docs\images\*.png`, `LICENSE`,
  `THIRD-PARTY-NOTICES.txt`) and `SHA256SUMS.txt`.
- **The documentation:** `README.md` is the landing page (features, requirements,
  download, a five-step quick start, building from source, license); `docs/user-guide.md`
  is the full guide. Their screenshots in `docs/images/` are taken from the command mock
  opened with `?sample` (a sample session, a release-like build stamp), so no real server,
  token or conversation shows.
- **Third-party notices** `scripts\third-party-notices.ps1`: the crates of `cargo tree -p
  voicebro-desktop -e normal --target x86_64-pc-windows-msvc` (the metadata's resolve
  graph is not used: it unifies the whole workspace's features, dev dependencies included),
  each with version, license, source and the `LICENSE*`, `COPYING*`, `NOTICE*` files from
  its package folder. Five numbered sections: (1) those crates; (2) native libraries that
  crates build or link in under licenses of their own: Opus 1.5 from `libopus_sys` (text
  `opus\COPYING` inside the crate) and the WebView2 loader from `webview2-com-sys` (WebView2
  SDK 1.0.3800.47; text kept in `scripts\notices\`) (opus-rs, a port of the Opus code,
  ships the Opus text as its own `COPYING` and is listed with it in section 1), plus a line
  that `ring`'s own `LICENSE-*` files cover its BoringSSL code; (3) the installer: NSIS 3.13 and Tauri's
  `nsis_tauri_utils` 0.5.3 with texts kept in `scripts\notices\`, and the WebView2 Runtime
  installer it downloads when the runtime is missing; (4) what is downloaded after the OK:
  Vosk and its catalog models, and the Whisper mode's files (change `whisper-mode`):
  whisper.cpp 1.9.5 (MIT), NVIDIA's CUDA 12.4 runtime under the CUDA EULA's redistributable
  terms (only with the GPU runtime), each Whisper model with its license, Silero VAD 6.2.0
  (MIT), sherpa-onnx 2.0.1 (Apache-2.0) with ONNX Runtime 1.28.2 (MIT), Supertonic 3 under
  BigScience OpenRAIL-M with its use restrictions, piper-phonemize 1.5.9 (GPL-3.0, with
  espeak-ng inside; not shipped, a separate download loaded at run time, its source
  linked), each Piper voice with its data license, and Kokoro English (Apache-2.0); the
  texts of whisper.cpp, Silero VAD, ONNX Runtime and Supertonic 3 are kept in
  `scripts\notices\`. The script's lists of Whisper models and Piper voices must name
  exactly the entries of `stt\catalog.rs` and `tts\catalog.rs`: an entry on one side only
  stops it. (5) the Codex CLI, needed but not shipped. None of the Whisper mode's files is
  in the installer or the zip. Each native entry is
  pinned to the crate version it was checked for: another version, or a missing kept text,
  stops the script with an error and no file is written. A Tauri CLI update can change the
  NSIS and plug-in versions; check them in `tauri-bundler`'s `nsis/mod.rs` when it does.
- Code signing is not set up; `bundle.windows.signCommand` or a certificate thumbprint can
  be added later without changing the rest.

### 4.11 Owner link (`owner.rs`)

Change `owner-only-access`: an optional mode in which only the voice of one linked Discord
account reaches the wake word and the assistant, so the subscription answers only its
holder. The commands (`/join`, `/leave`, `/status`, the Mute button) and the answers stay
open to everyone.

- **State.** One `OwnerState` (shared `Arc`, held by the runtime and the gate host):
  - the link (`OwnerLink`: Discord user id and name, the assistant id, the assistant
    account's fingerprint, the time);
  - at most one waiting code;
  - why the last code ended (`code_ended`) and why there is no link (`link_ended`);
  - `linked: AtomicU64`, the owner's id whatever the mode (`linked_id()`), and the older
    `filter: AtomicU64`, both lock-free, for the access filter (4.13).

  The owner-only mode is the `owner` value of `discord.access`; `AccessState::set_mode`
  passes it on at start and on every save of it. Every change publishes `AppEvent::Owner`
  and then calls the listener the access state registered (`set_listener`), still under
  the owner's lock, so the listener reads only the two atomics.
- **The code.** Link in the window: the desktop shell has Codex read the account (as Check),
  then `Runtime::start_link` issues a code bound to that account; not signed in is
  refused.
  - The code is 8 Crockford base32 symbols from the OS random source, shown as `XXXX-XXXX`.
  - Typed codes are upper-cased, spaces and dashes dropped, `O` read as `0`, `I` and `L` as
    `1`, then compared in constant time.
  - It lives 10 minutes; 5 wrong codes in total, from anyone, end it; Link again replaces
    it; Cancel ends it.
  - `/link` hands the sender's id (from the interaction) to `VoiceHost::complete_link`,
    and the outcome is the ephemeral reply.
  - The code reaches only the window, through the status and the event: `PendingCode`'s
    `Debug` prints `<hidden>`, and the log lines say only that a code was issued, a wrong
    code came (n of 5), or an account was linked.
- **The file.** `owner-link.bin` in the data folder: the link's JSON (`"version": 1`)
  encrypted with `CryptProtectData` for the current user (`CRYPTPROTECT_UI_FORBIDDEN`, the
  entropy `VoiceBro owner link v1`; the calls are in `protect.rs`, shared with 4.13),
  written through `owner-link.bin.tmp` and a rename, read once at start. A file that does
  not decrypt, parse or match the version is no link (`link_ended`: "the link file could
  not be read", one warning). Unlink deletes it. Off Windows the link is kept in memory
  only.
- **The assistant account.** The fingerprint is SHA-256 of `<account type>:<email trimmed,
  lower-cased>`; the email is never stored. The runtime compares every signed-in
  `AppEvent::Assistant` status (Check, Link, the read at start) and, synchronously, the
  account read by each join preparation. Another account removes the link
  (`link_ended`: "the Codex account changed"); signing out keeps it. The join goes on in
  either case: only the voice is limited.
- **Per engine** (change `claude-and-opencode-engines`, design D9). `signed_in_account`
  takes the engine and the fingerprint from the selected engine's status: Codex's ChatGPT
  account, Claude Code's sign-in (`claude.ai` or `api_key` with the email of
  `claude auth status`), and for OpenCode, which has no account, an empty fingerprint
  whether or not anything is set up, so Link works without one. `issue_code_for(engine,
  account)` binds the code to both and the link records the engine in `assistant`.
  `check_account_of(engine, fingerprint)` compares only a link of the same engine: another
  account removes it with "the <Codex | Claude | OpenCode> account changed"; a link of
  another engine is kept, and the new engine and account are written into it (logged
  "owner link: kept for <engine>; its account is recorded from now on"). A Claude Code
  without a sign-in refuses Link with "Claude Code is not signed in". `owner-link.bin` keeps
  its fields, so a 1.2.0 file reads as a Codex link.

### 4.12 Speech to text and the local voice (`stt/`, `tts/`)

Change `whisper-mode`: the parts of the Whisper mode (`assistant.mode = whisper`).
whisper.cpp turns the people's speech into text on this PC and the gate spots the phrases
in it (4.3), Codex answers the questions as text turns (4.5), and a local voice speaks the
answer. Nothing of it runs in the realtime mode. The model and the voice load at a `/join`
in this mode and unload 0.5 s after the last leave (4.7). Every file is pinned by SHA-256,
never ships with the app, and is downloaded through the shared asset store (4.4) only after
the user agrees in the window (4.8).

**Speech to text (`stt/`)**

```rust
/// A loaded speech-to-text model. One thread owns it.
pub trait SpeechToText: Send {
    fn transcribe(&mut self, audio: &[f32], options: &DecodeOptions) -> Result<RawTranscript, SttError>; // 16 kHz mono, -1..1
}
pub struct DecodeOptions { pub language: String, pub threads: u32, pub prompt: Option<String> }

/// Loads a model: whisper.cpp, or a fake in tests. `Loaded` says which device it runs on.
pub trait Engine: Send + Sync {
    fn load(&self, request: &LoadRequest) -> Result<Loaded, SttError>;
}

/// Receives the transcripts: the registry, which routes them to the guild's gate.
pub trait TranscriptSink: Send + Sync {
    fn on_transcript(&self, transcript: Transcript);
}
```

- **The runtime** (`whisper/ffi.rs`, `whisper/mod.rs`, design D8): whisper.cpp v1.9.5, from
  the nightly tag `b5454` (the same commit). Two pinned zips:
  - CPU, 8.9 MB: kept are `whisper.dll`, `ggml.dll`, `ggml-base.dll` and the
    `ggml-cpu-*.dll` variants;
  - NVIDIA GPU, CUDA 12.4, 685 MB: the same plus `ggml-cuda.dll`, `cudart64_12.dll`,
    `cublas64_12.dll` and `cublasLt64_12.dll`, about 1.15 GB unpacked. It needs no
    `nvrtc*.dll`.
- **Loading.** `WhisperRuntime::load` loads `ggml-base.dll`, `ggml.dll` and `whisper.dll` by
  full path (`LOAD_WITH_ALTERED_SEARCH_PATH`) and refuses a `whisper_version()` other than
  `1.9.5`. It sends whisper.cpp's and ggml's log into the app log (`whisper_log_set`). It
  then loads each `ggml-cpu-*` variant of the folder, asks its own `ggml_backend_score`,
  and registers the best one with `ggml_backend_load` (alderlake on the test PC); the
  runtime loads in about 0.01 s. The CUDA backend is added only when the GPU device is
  chosen (`enable_cuda`): the three CUDA libraries are loaded first by full path, so ggml's
  plain `LoadLibraryW` of `ggml-cuda.dll` binds to them, and on the CPU device no CUDA
  context exists, even from the GPU folder. No process-wide `SetDllDirectoryW`: it would
  also change where `onnxruntime.dll` is looked for.
- **Once per process.** The runtime is loaded on first use from the GPU folder when it is
  on disk (it holds the CPU set too), else from the CPU folder, and stays loaded: ggml's
  backend registry is global, and CUDA's libraries are not made to be unloaded. Only the
  model is freed after the last leave; after a model on the GPU the CUDA context is reset
  too (`WhisperRuntime::reset_cuda`: `cudaSetDevice(0)` and `cudaDeviceReset` of the
  loaded `cudart64_12.dll`; change `whisper-config-layout`, design D9), so its GPU memory
  goes back, and the next join creates a new one. When the CPU runtime was loaded in this run and the
  GPU runtime is downloaded later, the GPU needs a restart of the app, and the fallback
  reason says so.
- **Struct mirrors.** `whisper_context_params`, `whisper_full_params` and
  `whisper_vad_params` are `#[repr(C)]` mirrors of the v1.9.5 header, passed by value. The
  pinned hash and the version check make them valid. The gated test
  `whisper_runtime_when_enabled` (`VOICEBRO_WHISPER_DIR`) compares the library's greedy and
  context defaults with the header's, field by field, and transcribes two corpus WAVs.
  Raw pointers stay in `ffi.rs`; `Context` frees the model on drop.
- **One context and its own state.** `Context::full` calls `whisper_full` on the context's
  default state, because whisper.cpp applies its VAD only there (`whisper_full_with_state`
  skips it). Only the transcriber thread touches the context; the header says it is not
  thread-safe.
- **The device** (`choose_device`, design D11). `whisper.device = gpu` loads with `use_gpu`
  and flash attention when ggml registered a GPU device after `enable_cuda`. Otherwise the
  model loads on the CPU, with the reason: "no CUDA device", "CUDA did not load: …", "the
  GPU runtime is not on disk", or "the CPU runtime was loaded before the GPU runtime was
  downloaded; restart VoiceBro to use the GPU". The log line is "Whisper device: gpu
  requested, cpu used: <reason>", and the status carries the reason.
- **Decoding** (`decode_params`, design D5): greedy (`best_of` 1), temperature 0 with
  whisper.cpp's fallback steps, `no_context`, nothing printed, `suppress_nst`, the
  no-speech threshold 0.6, the language (`auto` or the code), and as `initial_prompt` the
  phrases in effect joined with commas, so they are spelled as the user typed them. The
  VAD is on, with `ggml-silero-v6.2.0.bin` and whisper.cpp's VAD defaults. `n_threads` is
  `whisper.threads`, or for 0 `auto_threads()`: half the logical processors, at most 16
  (12 on the test PC, the fastest of 4, 6, 8 and 12 for every model).
- **The catalog** (`catalog.rs`, design D9): thirteen models, each a single pinned `.bin`
  (`AssetKind::File`) in `assets\<id>\`:
  - multilingual: tiny, base and small (q8_0), medium (q5_0), large-v3-turbo (q5_0, q8_0
    and full precision) and large-v3 (q5_0);
  - English only: base.en and small.en (q8_0), distil-large-v3.5;
  - Russian-tuned, from community uploaders: large-v3-turbo Russian (MIT) and large-v3
    Podlodka (q5_0, Apache-2.0).

  The ggerganov models are pinned to one Hugging Face commit, the others to their own
  repositories' commits. Each entry has a group, a one-line note and a license. The Silero
  VAD model (0.9 MB, MIT) comes with every setup. The default is `whisper-base-q8_0` on the
  CPU. `LANGUAGES` lists the 100 language codes whisper.cpp knows.
- **The user's own models** (`local.rs`): `whisper-models\` in the data folder. Every
  `.bin` file there is the model `local:<file name>`, listed once at start
  (`scan_local_models`, which creates the folder); other files are skipped with a log line,
  and a name with a separator, a colon or `..` is no id. The app never writes there.
- **The cutter** (`cutter.rs`, thread `voicebro-whisper-cut`, designs D2 and D3). Per guild
  and SSRC it downsamples each 48 kHz stereo frame to 16 kHz mono with `StereoDownsampler`,
  appends it, and counts the voiced frames (peak ≥ 700). On each `Tick`, a speaker absent
  for 35 ticks (700 ms) ends the utterance:
  - fewer than 12 voiced frames (240 ms) is noise, dropped and counted;
  - otherwise 300 ms of silence is added, and the clip is padded to 1 s, since whisper.cpp
    returns nothing for shorter clips.

  The `Utterance` carries the instant of its last frame, the speech end of the latency
  line. At 28 s an utterance is cut, and the rest starts a new one. Silence is counted in
  receive ticks, so a late tick or a busy transcriber never ends an utterance early.
- **The transcriber** (`transcriber.rs`, thread `voicebro-whisper`). A queue of 8
  utterances; a ninth drops the oldest waiting, counted and logged. One at a time it runs
  the model in the manager's slot, then `clean`:
  - segments above the no-speech threshold, and segments that are only a note in brackets
    ("[BLANK_AUDIO]", "(music)", "*…*"), are dropped;
  - nothing left is noise;
  - one word four or more times in a row is a hallucination when the run is also faster
    than five words a second of the utterance's speech (`MAX_REPEATS_PER_SECOND`; change
    `whisper-config-layout`, design D7), so a person's "bro, bro, bro, bro" is kept. A
    drop is counted and logged with the repeats and the length, without the text.

  The rest goes to the sink as a `Transcript`: the SSRC, the text, the language used or
  detected, the instants, the audio length, the model and the device. What Whisper heard
  is a detail line.
- **Phrases** (`phrases.rs`, design D4). `decide(text, mode, active, phrases)` compares in
  the matching form of 4.4 (`phrase_occurrences` on the utterance's words, so punctuation,
  case and "ё" do not matter) and returns `Activate {phrase, question}` (the words after
  the phrase as Whisper wrote them, if any), `Deactivate {phrase}`, `Question` or `Ignore`
  (4.3). `whisper.phrases` is one pair of lists for every language, apart from the wake
  models' phrases; a list never set means the activation "bro" or the deactivation "peace
  out", whatever `whisper.language` is (change `whisper-config-layout`, design D2).
  Whisper writes any word, so there is no vocabulary check.
- **`WhisperManager`** (`manager.rs`, designs D10, D12 and D15 of change `whisper-mode`, D1
  of change `whisper-config-layout`):
  - *`configure(settings)`*, at start and on the `mode` and `whisper` parts. In both modes
    it checks, in the calling thread and without the network, which files the selected
    model needs on the selected device: the GPU runtime for `gpu`, for `cpu` the GPU runtime
    when it is on disk and else the CPU runtime, the model and the VAD model. It publishes
    them as `missing`. In the realtime mode it unloads and the state is `Off`. In the
    Whisper mode, without the user's consent for that model and device, the state is
    `NeedsDownload {items}`, and nothing is requested from the network.
  - *The consent* (`download(model)`) is accepted in either mode whenever the selected model
    misses files on the selected device (or after a failed download), and refused for any
    other model. It is held in memory only. A thread of its own downloads (`Downloading`,
    `Installing`) and empties `missing`. In the realtime mode the state then returns to
    `Off` and nothing loads; in the Whisper mode it is `Idle`, or the model loads when the
    bot is in a channel.
  - *A model or device change* while loaded reloads in the background and swaps the model
    in its slot; the old one serves until then. Language, threads and phrases apply to the
    next utterance through the shared `DecodeOptions`. A newer `configure` makes an older
    background job give way (a generation counter).
  - *`prepare_join`* loads now, or refuses with `JOIN_NEEDS_DOWNLOADS` while files are
    missing. *`release`* unloads 0.5 s later unless a join came first (a presence counter).
    The unload closes the pipeline first, which stops the utterance being transcribed,
    then frees the model; after a model on the GPU it resets the CUDA context with the busy
    lock held, so no load runs meanwhile. A switch to the realtime mode resets it only when
    the lock is free.
  - *It implements `WakeFeed`*: frames and ticks go to the cutter's channel only while a
    model is loaded, and a full channel counts a dropped frame. `clear_guild` drops what is
    cut or queued for a guild and stops its utterance being transcribed.
- **`Status.whisper`** (`WhisperStatus`, `AppEvent::Whisper`): the state (`off`,
  `needs_download`, `downloading`, `installing`, `idle`, `loading`, `ready`, `error`), the
  model, the device requested and in use, the fallback reason, the backend ("CUDA0: NVIDIA
  GeForce RTX 5090 Laptop GPU", "CPU ggml-cpu-alderlake, 12 threads"), the language, the
  load time, the phrases in effect, `missing` (the files the selected model and device
  still need, in either mode: the window's notices and Download buttons) and the counters:
  queued, transcribed, dropped late, dropped as noise, dropped as hallucination, the last
  transcription time and audio length.

**The local voice (`tts/`)**

```rust
/// A loaded voice. One thread owns it.
pub trait Synth: Send {
    fn sample_rate(&self) -> u32;
    /// Mono audio in -1..1; `None` when `stop` turned true during the synthesis.
    fn synthesize(&mut self, text: &str, language: Option<&str>, speed: f32, stop: &dyn Fn() -> bool)
        -> Result<Option<Vec<f32>>, TtsError>;
}

/// Loads a voice by id: sherpa-onnx and the Windows voices, or a fake in tests.
pub trait Loader: Send + Sync {
    fn load(&self, voice: &str, assets_dir: &Path) -> Result<Box<dyn Synth>, TtsError>;
}
```

- **sherpa-onnx** (`sherpa.rs`, design D9b): v2.0.1, the CPU build. Its 7.9 MB archive keeps
  `sherpa-onnx-c-api.dll` and its `onnxruntime.dll` (1.28.2).
  - `onnxruntime.dll` is loaded first, by full path, so the C API binds to it and not to
    Windows 11's own older `System32\onnxruntime.dll`; the log names the path. The library
    loads once per process and stays.
  - The config structs are mirrored by hand from the v2.0.1 `c-api.h`; a test checks their
    sizes and offsets.
  - Only `SherpaOnnxOfflineTtsGenerateWithConfig` is used. Its progress callback returns 0
    when the piece's epoch ended, which stops the synthesis.
  - The provider is `cpu`, with threads per engine: Supertonic 2, Piper 4, Kokoro 8, the
    fastest measured. The silence scale is 0.2.
  - Supertonic 3 reads raw text with `extra = {"lang": …}`: the question's language when
    Supertonic knows it, else `na`, which lets the model infer it. It runs 5 steps. Piper
    and Kokoro get the phoneme codepoints of the text.
- **piper-phonemize** (`phonemize.rs`): v1.5.9, GPL-3.0, a separate 0.4 MB download that only
  Piper and Kokoro need, loaded at run time.
  - `PiperPhonemizeInitialize` gets the first voice's `espeak-ng-data` once per process (every
    voice archive carries the same data) and returns espeak's sample rate, 22050, on
    success.
  - `PiperPhonemizeText` gives the codepoints per sentence for the voice's espeak voice:
    from the Piper voice's `.onnx.json`, `en-us` for Kokoro.
  - espeak-ng keeps global state, so there is one call at a time in the process.
- **The Windows voices** (`windows.rs`): `Windows.Media.SpeechSynthesis.SpeechSynthesizer`
  through the `windows` crate 0.62, already in the lock file through Tauri. `AllVoices`
  gives the list at run time, as `windows:<token>` (the last part of the voice's registry
  id). A synthesis writes a WAV stream at the speaking rate of `tts.speed`, read by walking
  its RIFF chunks; another rate than 16 kHz is resampled to it. No download.
- **The catalog** (`catalog.rs`): the engine and the phonemizer as assets, and the voice
  packs, each with its data license:
  - Supertonic 3: one 129 MB archive, ten voices (F1–F5, M1–M5) in 31 languages, 44.1 kHz,
    BigScience OpenRAIL-M;
  - seventeen Piper voices in the ten wake catalog languages, 67–116 MB each, 22.05 kHz;
    voices whose data is non-commercial or AGPL are left out;
  - Kokoro English: 320 MB, eleven voices, 24 kHz, Apache-2.0.

  Ids are `supertonic3:F1` … `supertonic3:M5`, `piper:<voice>`, `kokoro-en:<speaker>` and
  `windows:<token>`. `assets_for(voice)` is the engine, the phonemizer for Piper and
  Kokoro, and the pack. The default is `supertonic3:F1`: it needs no GPL component and
  speaks every wake catalog language.
- **What is read** (`text.rs`, design D7). A `SentenceSplitter` per turn cuts the streamed
  text into pieces:
  - at `.`, `!`, `?` or `…` before whitespace, or at a line break;
  - the first piece of each agent message also at a comma, so the first audio comes sooner;
  - a piece under 20 characters waits for the next one; a piece over 300 is cut at its last
    comma, semicolon or space;
  - `finish` gives the rest when the message completes.

  `speakable` drops code blocks and table rows (the chat keeps them), reads `[text](url)`
  as the text and a bare URL as its host, and removes heading, list, quote and emphasis
  markers and emoji; an underscore inside a word stays.
- **`SpeechPlayer`** (`player.rs`): the guild's output pulls one 20 ms frame from it per read,
  as from the call's jitter buffer.
  - It holds the synthesized 48 kHz audio, mono duplicated to stereo, and plays it in order.
  - `voice_ahead` is true while it holds a frame with a peak of 256 or more, so sending
    toward Discord starts before the voice plays (4.1).
  - It reports the first played frame of each turn (`on_first_played`, the last step of the
    latency line) and answers `own_latency`.
  - `flush` raises its epoch and drops what it holds; audio of an older epoch is refused.
    The output's own flush and mute call `reset`.
- **`TtsManager`** (`manager.rs`):
  - *`configure`* works like `WhisperManager`'s, for the voice's files (the engine, the
    phonemizer, the pack; nothing for a Windows voice): `missing` in both modes, its own
    consent per voice, and `download(voice)` (command `tts_download`) accepted in either
    mode while the selected voice misses files; in the realtime mode it installs them and
    loads nothing. The speed applies at once.
  - *`prepare_join`* loads the voice and warms it up with one short synthesis; *`release`*
    unloads it 0.5 s after the last leave.
  - *One thread, `voicebro-tts`,* takes the queued pieces (epoch, text, language, player,
    turn) in order. It waits while the player holds more than 20 s, skips a piece whose
    epoch ended, synthesizes, resamples to 48 kHz (`Resampler`, 4.1) and pushes the audio.
  - *`preview(voice, speed)`* loads a voice of its own for the call and reads
    `preview_sentence`: a neutral sentence about the weather in London in the voice's
    language (English for Supertonic). It returns WAV bytes for the window and never
    reaches Discord.
- **`Status.tts`** (`TtsStatus`, `AppEvent::Tts`): the state (as Whisper's), the voice, the
  engine, the sample rate, the load time, the last answer's time from queueing its first
  piece to its audio, the pieces spoken and skipped, and `missing` (the selected voice's
  files not on disk, in either mode).

**Measured on the development PC** (Intel Core Ultra 9 275HX, 24 logical processors; RTX
5090 Laptop GPU, 24 GB; design.md holds the full numbers). Whisper encodes a 30 s window,
so a clip's time barely depends on its length:

| Whisper model and device | Per 2–10 s utterance |
|---|---|
| tiny (q8_0), CPU, 12 threads | 0.23–0.28 s |
| base (q8_0), CPU, 12 threads (loads in 0.08 s) | 0.54–0.56 s |
| small (q8_0), CPU, 12 threads | 1.7–1.8 s |
| medium (q5_0), CPU, 12 threads | 6.4–9.5 s |
| large-v3-turbo (q5_0), CPU, 12 threads | 11.1–11.3 s |
| large-v3-turbo (q5_0), GPU (loads in 0.44–0.55 s) | 115–140 ms |

On the GPU, the CUDA 12.4 build runs on Blackwell through the driver's JIT of its PTX: the
very first transcription after installing took 25 s, and the driver caches the result.
large-v3-turbo (q5_0) then holds about 826 MB of video memory (573 MB of weights plus the
KV and compute buffers, from ggml's log lines).

| Voice, CPU | First sentence | Real-time factor |
|---|---|---|
| Supertonic 3, 2 threads (loads in about 0.5 s) | about 0.47 s | 0.13 |
| Piper | 0.09–0.16 s | about 0.03 |
| Kokoro English, 8 threads | about 0.6 s | 0.19 |
| Windows | 5–20 ms per sentence | |

Supertonic was slower with more than 2 threads. Whisper large-v3-turbo read back 89–92 % of
the words of every voice's output, so Kokoro's espeak phonemes work under sherpa-onnx 2.0.

**Tests and probes.** Unit tests without models cover the cutter's rules, the phrase
decisions, the gate in the Whisper mode, the splitter and `speakable`, the resampler, the
player's flush and epochs, both managers with fake engines and a fake fetch, and the text
session against the fake Codex (4.5). The probes:

- `probe whisper <wav> [--model id] [--device cpu|gpu] [--language xx] [--threads n]
  [--prompt text]` prints the text, the language, the load time and the transcription
  time; `--bench` repeats the clip to 2, 5 and 10 s and prints the median of three runs
  each;
- `probe whisper-corpus [--languages en,ru] [--no-prompt]` runs every WAV of the wake corpus
  (`target\wake-corpus\<lang>\`) through Whisper and `decide` with that language's default
  phrases, then three noise-only clips the probe makes (silence, steady noise of peak 300,
  a game-like mix) with every language's phrases in the prompt, one line per file and a
  total; a noise clip that gives a phrase fails it;
- `probe tts <voice id> "<text>" <out.wav>` reads a sentence, and with `--bench` the fixed
  sentences about Bangkok and London with 2, 4 and 8 threads: load, first audio, real-time
  factor, WAVs in `target\tts-listen\`, and how much of each Whisper reads back.

They take their files from `--assets`, `VOICEBRO_WHISPER_DIR` or
`target\whisper-test\assets\` through the asset store.

**Not tested:** a real Discord channel with two or more people and a game running, the
Whisper mode's wake phrase in a channel, and the comparison of both modes' resource use.
These are the user's tests.

### 4.13 Access and consent (`access.rs`, `consent.rs`, `protect.rs`)

Change `voice-consent`: whose voice reaches the wake word and the assistant, chosen by the
operator in `discord.access`, and the records of the people who agreed with `/optin`. Every
mode is optional; the default `everyone` is the behavior before the change.

- **`VoiceFilter`.** `Everyone`, or `Only(HashSet<u64>)` of Discord user ids:
  - `everyone` gives `Everyone`;
  - `owner` gives the linked owner alone, or an empty set with no link;
  - `agreed` gives the linked owner (who needs no record) and every record.

  `passes(user)` lets a source with no known account through only `Everyone`.
- **`AccessState`** holds the mode, the owner state, the consent state and
  `RwLock<Arc<VoiceFilter>>`. `refresh` rebuilds the filter and swaps the `Arc`; the
  receive path clones it once per tick and holds no lock while mixing. `refresh` runs on
  `set_mode` and from the listeners of the owner and the consent state, so a mode change, a
  link, an unlink, an "I agree", an `/optout` and a Remove apply from the next frame. Its
  observers (the gate host's participant names) run after each refresh.
- **The records** (`ConsentRecord`): Discord user id, the name at the moment of agreeing,
  `agreed_at` (RFC 3339, UTC), `text_version` and the `assistant` engine. `consents.bin` in
  the data folder holds `{"version": 1, "records": [...]}` encrypted like the owner link
  with the entropy `VoiceBro consents v1`, written through `consents.bin.tmp` and a rename,
  and deleted when the last record goes. A file that does not decrypt, parse or match the
  version gives no records and one warning; `ConsentStatus.unreadable` keeps the reason
  until the next write. Off Windows the records are kept in memory only.
- **`ConsentState`** keeps the records behind a mutex and their ids in a separate
  `RwLock<Arc<HashSet<u64>>>` for the filter. `agree`, `withdraw` and `set_assistant`
  write the file, publish `AppEvent::Consent` and call the listener outside the lock. Log
  lines: "consent: <name> agreed (text v<n>)", "consent: <name> withdrew", "consent: <name>
  removed in the window", and at start or on an engine change "consent: N record(s)
  removed because the <consent text | assistant engine> changed".
- **The consent text.** `CONSENT_TEXT_VERSION` (1) and every published version's text in
  `CONSENT_TEXTS`, filled with the engine's provider ("OpenAI, through Codex") and the
  policy URL. It says that after the activation phrase the voice (voice mode) or the text
  of the speech made on the operator's PC (Whisper mode) goes to the provider, what is
  kept, that `/optout` withdraws at once and earlier speech is already sent, and where the
  policy is. A record of another version or engine is deleted at start and when
  `assistant.engine` changes; raising the version makes everyone run `/optin` again.
- **The recipient per engine** (change `claude-and-opencode-engines`, design D10).
  `consent::recipient_key(settings)` is the record's `assistant` value: `codex`, `claude` or
  `opencode:<provider>` (the provider part of `opencode.model`, `default` when it is
  empty), so a change of the engine or of OpenCode's provider ends the records
  (`apply` calls `set_assistant` for the `engine` and `opencode` parts).
  `provider_name(key)` fills the text: "OpenAI, through Codex" (the 1.2.0 wording, so the
  text and its version stay and no Codex record is lost), "Anthropic, through Claude Code",
  and for OpenCode the provider's company by a short table (an unknown id named as it is),
  "through OpenCode, and, for the web searches the assistant makes, OpenCode's web search
  service". `/optin`'s command description names no company.
- **The window.** `Status.consent` and `AppEvent::Consent` carry the records (id as a
  string, name, `agreed_at`) and the unreadable reason; `Runtime::consent_remove(id)`
  (the Tauri command `consent_remove`) deletes one record as `/optout` does and sends the
  person nothing.

## 5. Data flow

### Audio in

```text
Discord UDP ─► songbird (DAVE, Opus decode; with opus-rs only DAVE, and the
   VoiceTick handler decodes per SSRC) ─► VoiceTick: per-SSRC 48 k stereo
   ─► owner filter (4.11: everyone, nobody, or the owner's account only; the
      others counted as ignored_frames and not decoded with opus-rs)
   ─► mix (i32 sum, clamp) ─► RelayGate.process_tick(mixed, per_speaker)
        ├─► per speaker (frame, tick n), then Tick n ─► ModeFeed
        │      ├─► realtime mode, wake word: WakeFeed (bounded channel)
        │      │      ─► wake worker thread ─► 16 k mono ─► recognizer ─► PhraseSpotter ─► gate.on_phrase
        │      └─► Whisper mode, both session modes: WhisperManager (bounded channel, 256)
        │             ─► voicebro-whisper-cut: 16 k mono per speaker; ends after 35 silent ticks
        │             ─► utterance queue (8, drop oldest) ─► voicebro-whisper: whisper_full + VAD
        │             ─► noise guards ─► Transcript ─► RelayRegistry ─► gate.on_transcript
        │                    ─► activation (+ question) │ deactivation │ question ─► session.user_text
        └─► realtime mode, session active, not muted: Frame ─► AudioPorts.input (FIFO, 5 frames)
               ─► assistant (Codex: send thread, 20 ms clock, 2-frame prefill,
                  oldest frame or silence ─► downmix ─► Opus ─► RTP)
```

### Audio out

```text
Codex: remote track ─► RTP packets ─► JitterBuffer (by sequence number, target 3..8)
   ◄─ pulled as FrameProvider: decode, FEC, concealment, 20 ms frames
Whisper mode: item/agentMessage/delta ─► SentenceSplitter ─► speakable() ─► TTS queue (epoch)
   ─► voicebro-tts: synthesize ─► Resampler to 48 kHz ─► SpeechPlayer (at most 20 s ahead)
   ◄─ pulled as FrameProvider: 20 ms frames in order
stub assistant, tests ─► OutputHandle.push(Frame) ─► OutputQueue (jitter 3..8 frames)
   all ─► PcmSource::fill_frame (flush epoch, mute, gain, earcons)
      └─► sending wish (audible now or ahead; off after 500 ms with nothing audible)
          ─► watch ─► session task ─► songbird Driver::mute(!on)
   ─► OpusOutput: Opus with FEC (expected loss from Discord's reports) ◄─ songbird mixer thread
   ─► songbird passthrough, DAVE ─► Discord (while muted: five silence frames, then nothing)
Discord RTCP receiver reports on our SSRC ─► DeliveryShared::on_report ─► expected loss
```

### Control

```text
/join ─► VoiceHost::prepare_join ─► Codex: start app-server, account/read, open thread
   ─► refused: reply with the reason, no join │ ready: songbird join, gate, attach
phrase hit ─► gate: Waiting→Starting ─► assistant.call_context, session_wanted(true)
   ─► Codex: ensure thread, build initialItems, open RTC call ─► ready() = true
   ─► gate replays pre-roll from the end of the phrase, then live frames
deactivation / idle timeout ─► gate: output muted + flushed, falling earcon, session_wanted(false)
   ─► Codex: thread/realtime/stop, keep thread and history for the next call
the people's transcript (realtime/transcript/delta, role user; done) while the call is live
   ─► CodexSession joins the turn's pieces ─► AudioPorts.heard ─► RelayRegistry
   ─► gate.on_heard: a deactivation phrase (closed, or held 300 ms) ─► the same deactivation
mute (app button | hotkey | Discord panel) ─► Runtime::toggle_mute ─► RelayRegistry
   ─► every gate: session ends (session_wanted(false)), nothing sent, output flushed,
      mute tone ─► AppEvent::MuteChanged ─► UI, panel
last /leave ─► Codex: app-server stops 0.5 s later; conversations kept
voice delegates a task ─► Codex agent: turn/started, items, turn/completed (completed)
   ─► CodexSession: AgentAnswer {guild, task, last agent text} ─► AudioPorts.answers
   ─► runtime answer handler: current assistant.post_answers ─► off: nothing
      │ else DiscordRuntime::post_answer: resolve channel, format, split, post (no mentions)
      └─► posted: log, clear `answers` error │ refused: `answers` error, log │ skipped: log

Whisper mode:
/join ─► prepare_join: Codex as above, then Whisper and the voice load
   ─► files missing: "Not joined: The Whisper mode needs downloads: …" │ loaded: join, gate, CodexTextSession
activation phrase in Whisper's text ─► gate: Waiting→Active, rising tone, call_context
   ─► the words after the phrase, if any, as the first question
question ─► CodexTextSession: flush the voice, turn/interrupt the running turn (if any)
   (one speaker at a time on: turn/steer into the running turn, or the next turn; nothing
   interrupted)
   ─► turn/start {text, additionalContext} ─► deltas spoken ─► turn/completed (completed)
   ─► AgentAnswer {guild, task: question, last agent text} ─► AudioPorts.answers (as above)
deactivation phrase / idle timeout / mute ─► session_wanted(false): voice flushed, turn interrupted
assistant.mode changed ─► Vosk off or on, Whisper and the voice configured,
   every gate rebuilt in the new mode on the same output; the bot stays in the channel
assistant.engine changed (Whisper mode for Claude Code and OpenCode) ─► EngineSwitch selects
   it, consent records of the old recipient deleted, every gate rebuilt (old session
   detached from its own engine) ─► per guild: new engine's prepare ─► attach │ error, no session
last /leave ─► Whisper and the voice unload 0.5 s later
```

### Settings

```text
UI patch ─► command save_settings ─► Settings::validate ─► store ─► Runtime::apply(diff)
   ─► gates (mode, timeout, earcons, labels, deactivation phrases on a wake change), gains (shared atomics), wake engine (phrases, model),
      Codex (thread settings), Discord (restart on token change), hotkey (re-register);
      answer posting: nothing (read by the answer handler at the next answer);
      mode: assistant, wake engine, Whisper, voice, every gate switched at once;
      whisper: Whisper (model or device reloads in the background), gates (phrases);
      tts: the voice (speed at once, another voice reloads in the background)
```

## 6. Contracts and constants

Values that are known to work. Change them only with a measurement or a corpus run.

| Contract | Value |
|---|---|
| Discord frame | 48 kHz, 2 ch, 20 ms, 1920 `i16` |
| Wake input | 16 kHz mono, 320 samples per 20 ms |
| Queue toward assistant | 5 frames (~100 ms) FIFO, drop oldest; the Codex send thread prefills 2 frames |
| Opus toward the call | mono VOIP, complexity 10, 32 kbit/s, in-band FEC, 5 % expected loss |
| Call receive jitter buffer | at most 50 packets; decodes up to 100 ms ahead; concealment at most 5 frames in a row |
| Call playout (`playout.rs`) | depth 60..500 ms from the 95th percentile of the relative arrival delay (2 s window, 20 ms buckets, forgetting 0.9983 per packet) + 20 ms; a call starts from the guild's last learnt depth, the first from a 200 ms prior (30 % of the mass at 160–180 ms); pauses first (±20 ms, at most 25 stretches without a new packet, at least 60 ms of a pause kept); speech by one pitch period below half or above twice the depth, at most once per 60 ms, correlation ≥ 0.5; bridging at most 3 frames |
| Queue toward Discord | 8 frames (~160 ms), drop oldest; jitter target 3..8 frames (stub and tests; the call uses its own jitter buffer) |
| Opus toward Discord | 48 kHz stereo VOIP, 128 kbit/s, complexity 10, in-band FEC; expected loss from Discord's reports (envelope, forgetting 0.9983 per packet sent), default 10 %; songbird passes the packets through |
| Sending toward Discord | on while the assistant's frame played now or seen ahead has a peak ≥ 256, or an earcon plays or waits; off after 25 frames (500 ms) with nothing audible; an earcon from silence waits 2 frames (40 ms); applied with `songbird::Driver::mute`, never `Call::mute` |
| Gains | input and output 0.0..4.0, default 1.0; input applied at the gate's send, output in `PcmSource` before earcons |
| Speech threshold | peak ≥ 700 (measured on the unscaled channel audio) |
| Utterance gap | 700 ms |
| Pre-roll / replay cap | 250 frames (~5 s) / 300 frames (~6 s) |
| Lookback without timing | 1 s + 80 ms margin |
| Activation / deactivation partials | 15 in a row / 5 of the last 10; a final always counts |
| Partial lag | ~150 ms |
| Reactivation guard | 1.5 s after a deactivation from either source, kept by the gate |
| Trailing silence | after 15 receive ticks (300 ms) of a speaker's absence, one 20 ms chunk per absent tick, caught up at once, up to 75 (1.5 s); counted in receive ticks, never on the worker's clock |
| Bridged gap reported | 3 ticks or more, at most one line per speaker per 10 s |
| Recognizer lag reported | over 500 ms, at most one line per 10 s |
| Transcript hold | 300 ms for a deactivation phrase whose last word may go on; the voice is silenced meanwhile |
| Previous transcript turn | read with the next turn when it ended less than 2 s before |
| Speaker recognizer idle | dropped after 30 s |
| Wake queue | 256 chunks, drop and count |
| Idle timeout | default 60 s; 0 = off; 5..3600 |
| Earcon volume | 0.18 |
| Default mute hotkey | `Ctrl+Alt+M`; empty disables |
| Codex history cap | 100 items or 24 000 bytes |
| Realtime version | `v3` (`v1`/`v2` are rejected) |
| Whisper input | 16 kHz mono `f32`, from each speaker's 48 kHz stereo frames through `StereoDownsampler` |
| Whisper utterance end | 35 receive ticks (700 ms) without a frame from that speaker |
| Longest utterance | 28 s, then cut; the rest starts a new one |
| Shortest utterance transcribed | 12 voiced frames (240 ms, peak ≥ 700); fewer is noise |
| Utterance padding | 300 ms of silence after the speech, then at least 1 s in all |
| Toward the cutter / utterance queue | 256 messages, a full channel drops and counts / 8 utterances, the oldest waiting dropped and counted |
| Whisper decoding | greedy, temperature 0 with whisper.cpp's fallback, `no_context`, Silero VAD 6.2.0 with whisper.cpp's defaults, no-speech threshold 0.6; one word 4 or more times in a row and faster than 5 words a second of speech is a hallucination |
| Whisper CPU threads | `whisper.threads`; 0 is half the logical processors, at most 16 |
| Whisper and voice unload | 0.5 s after the last leave; the transcription in progress stops at the end of its encoder pass or decoder step; after a model on the GPU the CUDA context is reset; the runtime libraries stay loaded |
| Voice pieces | end at `. ! ? …` before whitespace or at a line break, the first piece of a message also at a comma; at least 20, at most 300 characters |
| Voice ahead of playback | at most 20 s |
| Voice threads | Supertonic 3: 2, 5 steps; Piper: 4; Kokoro: 8 |
| Voice resampling | polyphase windowed sinc, 32 taps per phase, Blackman, cut-off 0.92 of the lower Nyquist |

## 7. Observability

- `log` facade everywhere in the core; the desktop app installs `tauri-plugin-log`.
- **What the file keeps.** Levels: the global level is Warn; this app's targets
  (`voicebro`, `voicebro_core`, `voicebro_desktop`, `detail`) log at Info; songbird's
  receive task at Error. The window (the `app-log` ring) gets every record that passes
  the levels. The file target has a filter (`keep_in_file`): in a release build with the
  verbose log off it keeps this app's event lines and third-party errors, and leaves out
  the **detail lines** and the receive task's per-packet errors; a debug build, or the
  verbose log (`diagnostics.verbose_log`, the `VERBOSE_LOG` flag the shell sets at start
  and on every settings change), writes everything. Detail lines are logged with
  `detail!("<area>", …)`, target `detail::<area>`: what the recognizer heard
  (`detail::wake`), what Whisper heard and dropped (`detail::whisper`), the per-turn latency (`detail::latency`), the call's buffer, track and
  end-of-call counters (`detail::rtc`), the delivery lines (`detail::delivery`), the
  speaker-to-SSRC mapping (`detail::receive`), the copy of each chat entry
  (`detail::chat`) and the text of each posted answer (`detail::answers`). The file rotates at 1 MB and keeps one archived file, about 2 MB.
- Log the value that was actually used, never the one that was requested and replaced.
- The wake worker logs what it heard; the UI shows the recognizer's results, not only the
  assistant's transcript.
- **The phrase decisions and the wake engine's health** (change `voice-recognition-rework`,
  design D5), at info level in every build and without recognized text (the text is only
  in the `detail::wake` line `[wake] <g>/<ssrc> heard: …`), built by pure functions in
  `wake/worker.rs` and `gate.rs`:

  ```text
  [wake] <g>/<ssrc>: activation|deactivation «<phrase>» <outcome>, <t> s after the phrase ended (<r> s recognizing, <q> s queued)
  [wake] <g>/<ssrc>: deactivation «<phrase>» was in <n> partial results but was not confirmed
  [wake] <g>/<ssrc>: <n> gaps in the audio bridged in the last 10 s, longest <ms> ms
  [wake] the recognizer runs <ms> ms behind the audio
  gate <g>: deactivation phrase «<phrase>» from the wake engine|the call's transcript; the session ended, waiting for the activation phrase
  ```

  The outcome is `applied`, `ignored (the session is already in that state)`, `ignored
  right after a deactivation` or `ignored (not listening: muted, always mode or no voice
  channel)`. "Recognizing" is the spotter's stream time from the phrase's end to the
  result, "queued" the time from the gate's feed to the worker; the gate gets their sum as
  `since_phrase_end`. The gaps line comes once 10 s passed since a speaker's first bridged
  gap; the lag line when a message is processed more than 500 ms after the gate fed it;
  each at most once per 10 s.
- **The Whisper mode** (change `whisper-mode`, design D15), at info level in every build and
  without what people said:

  ```text
  Whisper runtime: whisper.cpp 1.9.5 from <folder>, CPU backend <ggml-cpu variant>
  Whisper model <id> loaded on <cpu|gpu> in <ms> ms (<backend>)
  Whisper device: gpu requested, cpu used: <reason>
  Whisper model <id> unloaded
  Whisper: the GPU's memory was given back (CUDA context reset)
  Whisper: the CUDA context was not reset: <error>   (warning)
  [whisper:<guild>] stopped the transcription in progress after <ms> ms: the bot left or Whisper unloads
  [whisper:<guild>] the transcriber is 8 utterances behind; dropped the oldest (<s> s, <n> so far)
  [whisper:<guild>] dropped a repeated-word transcription as a hallucination: one word <repeats> times in <s> s (<n> so far)
  [whisper] <ggml's buffer-size and device lines>
  voice engine: sherpa-onnx from <folder>, onnxruntime <path>
  phonemizer: piper-phonemize from <folder>, espeak-ng data <folder>
  voice <id> loaded in <ms> ms, warmed up in <ms> ms (<rate> Hz)
  voice <id> unloaded
  gate <g>: activation phrase «<phrase>» in Whisper's text; the assistant is listening
  gate <g>: deactivation phrase «<phrase>» from Whisper's text; the session ended, waiting for the activation phrase
  guild <g>: the assistant switched to the <realtime|whisper> mode
  ```

  whisper.cpp's and ggml's own log goes through `whisper_log_set`: its errors and warnings
  as warnings, the lines with sizes and devices ("CUDA0 model buffer size = … MiB", the KV
  and compute buffers) at info, so the log shows what Whisper holds on the GPU; the rest at
  debug. The app does not measure GPU memory itself. The private working set (Memory,
  below) includes the in-process engines. What Whisper heard is a detail line, `[whisper:<g>]
  ssrc <n> heard in <ms> ms (<s> s, <language>): <text>`, and so are the utterances dropped
  as noise or as a hallucination.
- Connection errors (Codex process, RTC call, Discord) surface in the status snapshot, not
  only in the log. The source `answers` (change `post-assistant-answers`) is set when
  Discord refuses an answer's post, or the chosen channel is gone, and cleared by the next
  post.
- **The agent's answers** (4.2, 4.7), prefixed with the guild:

  ```text
  [answers:<guild>] posted the Codex answer to #<channel> in <server> (<n> messages, <chars> characters)
  [answers:<guild>] the Codex answer was not posted: <reason>
  [answers:<guild>] cannot post to #<channel> in <server>: <Discord's reason>[; the bot lacks <permissions> here]
  ```

  The text itself is a detail line (`detail::answers`), in the file only with the verbose
  log.
- **Panics** are logged by the desktop shell's hook (4.8) with thread, location and message,
  in every build. A panic of opus-rs is also logged by the runtime when the bot leaves
  ("<path>: <codec> panicked: …", 4.7) and shown in the window's codec failure notice;
  any other panic ends the app after a message box that names the log folder.
- Counters: packets in/out, drops per queue, supplied silence, underruns, wake drops. They
  are part of the status snapshot.
- **The leg toward Discord**, per voice channel (`DeliveryCounters`): the voice server and
  region, packets encoded and passed through, encode errors, the expected loss and its
  source, Discord's reports on our stream with the last loss and jitter. Logged as:

  ```text
  [delivery:<guild>] voice server <host> (region <region>)
  [delivery:<guild>] voice server <host> (region <region>): <n> packets encoded, <n> passed through, <n> encode errors; expected loss <p> % (measured|default); <n> reports, last loss <p> %, jitter <ms> ms
  ```
- **Turn latency** (`latency.rs`): one `LatencyTracker` per guild, owned by the gate. The gate reports every
  frame with speech (peak ≥ 700, before the gain) it sends to the assistant; `PcmSource`
  reports every played frame of the assistant's audio with a signal (peak ≥ 256), with the
  network arrival of its packet from the jitter buffer (`FrameProvider::arrival`). A turn
  starts after more than 350 ms without speech; the first answer audio within 15 s of its
  last speech is its response; speech while the answer plays (its audio less than 1 s ago)
  is reported once as an interruption; answer audio with no turn waiting is autonomous.
  The log lines, prefixed with the guild:

  ```text
  [latency:<guild>] turn <n> (response|response after interruption): first Discord speech -> ChatGPT audio <ms> ms; last Discord speech -> ChatGPT audio <ms> ms; ChatGPT audio -> Discord relay handoff <ms> ms; first speech -> Discord relay handoff <ms> ms
  [latency:<guild>] turn <n>: Discord speech interrupted ChatGPT output
  [latency:<guild>] ChatGPT initiated autonomous speech
  [latency:<guild>] autonomous ChatGPT speech: ChatGPT audio -> Discord relay handoff <ms> ms
  ```

  "ChatGPT audio -> Discord relay handoff" is the time the answer waited in the jitter
  buffer. The last 50 measurements per guild, with their local time, are
  `Status.latency`.

  In the Whisper mode the output reports nothing (`own_latency`, 4.1). The text session
  measures each answer itself (`WhisperTiming`) and logs one line when its first voice
  frame plays; the report, of kind `whisper` with its four timings, joins `Status.latency`:

  ```text
  [latency:<guild>] whisper turn <n>: speech end -> text <ms> ms (<s> s of audio, <model>, <device>); text -> Codex first words <ms> ms; first words -> voice <ms> ms; speech end -> voice <ms> ms
  ```

  "Speech end" is when the gate handed over the utterance's last frame, so "speech end ->
  text" holds the 700 ms that end the utterance, the wait in the queue and the
  transcription. "Codex first words" is the turn's first `item/agentMessage/delta`; "voice"
  is the first played frame of the turn's voice. An interrupted turn is not measured.
- **Memory** is the private working set, the number Task Manager shows as "Memory", and
  only that is compared or reported. `apps/desktop/src/memory.rs` walks the process tree
  from the app's id with a ToolHelp snapshot (the app, its `msedgewebview2.exe`
  descendants, its `codex*` child and everything under it) and reads each process with
  `GetProcessMemoryInfo` into `PROCESS_MEMORY_COUNTERS_EX2` (`PrivateWorkingSetSize`)
  every 3 s on its own thread (`memory::Reader`), which runs only while the
  `diagnostics.memory` setting is on: the shell starts it at start-up when the setting
  says so and starts or stops it on every settings change, logging "memory reading
  started" / "stopped". The `memory_status` command returns the latest reading, `None`
  while the reader is not running. Of the `diagnostics` section the core reads only
  `opus_codec`: `Runtime::apply` logs the switches, keeps the codec in the gate host for the
  next join and passes it to the assistant's config for the next call ("Opus codec <label>
  from the next join and the next call"); a running join or call keeps its own.
  `scripts\measure.ps1` reads the same metric from the `Working Set - Private` counter for
  release builds; private bytes (the commit charge) are printed only for diagnosis.
- The status snapshot holds measured values only. A quantity that is not measured is
  omitted, never replaced by a label.

## 8. Conventions

- Errors: `thiserror` enums per module in the core; `String` only at the Tauri boundary.
- No panics on the audio paths; a bad frame is dropped and counted.
- Bounded queues everywhere, with an explicit overflow policy (drop oldest) and a counter.
- Locks are never held across an `await` or across FFI calls into Vosk. The Whisper model
  and the voice sit in a mutex that only their own thread holds during a call; a reload or
  an unload takes it between calls.
- FFI (Vosk, whisper.cpp and ggml, sherpa-onnx, piper-phonemize) is wrapped in safe types
  with `Drop`; raw pointers never leave `wake/vosk/`, `stt/whisper/ffi.rs`, `tts/sherpa.rs`
  and `tts/phonemize.rs`.
- Release builds optimize for speed (`opt-level = 3`, thin LTO, one codegen unit, symbols
  stripped), not for size: the hot paths are FIR filters, mixing and Opus, and the binary
  is small next to the model and the Codex process anyway. They unwind on panic
  (`panic = "unwind"`, since change `opus-codec-experiment`): the Opus codec module catches
  a panic of opus-rs (4.1), and the desktop shell's panic hook ends the process on any
  other panic, as `panic = "abort"` did before (4.8).
- Unit tests sit next to the code. Tests that need assets or a live account are gated by
  environment variables and skipped otherwise.
- Files are edited with the editor tool; `.gitattributes` fixes line endings.
- Every build handed over has a new version or a visible build stamp.

## 9. Extension points

| To add | Touch |
|---|---|
| A wake engine (sherpa-onnx, a KWS) | `wake/<engine>/` implementing `WakeEngine`, `LoadedModel`, `SpeechRecognizer`; catalog entries; nothing in `PhraseSpotter` or the gate |
| A model or language | A `ModelEntry` in the catalog (URL, SHA-256, license) |
| A Whisper model | A `WhisperModelEntry` in `stt/catalog.rs` (a single-file `Asset` with URL, SHA-256 and size, the group, the note, the license) and its line in `scripts\third-party-notices.ps1` |
| A voice | A `VoiceEntry` in `tts/catalog.rs`, with its `VoicePack` (URL, SHA-256, data license, sample rate, whether it needs the phonemizer); a Piper voice also in `scripts\third-party-notices.ps1` |
| A speech-to-text engine | `stt::manager::Engine` loading a `SpeechToText`; nothing in the cutter, the transcriber, the phrases or the gate |
| A voice engine | A `Synth` returned by `tts::manager::Loader`, its files as assets; nothing in the splitter, the player or the text session |
| An assistant or an auth path (API key, another provider) | `assistant/<engine>/` implementing `Assistant` and `AssistantSession` over `AudioPorts` (with `user_text` and `working` for the Whisper mode's questions in words), sending its agent's final answers as `AgentAnswer` through `AudioPorts.answers` to have them posted; nothing in the gate or Discord |
| A setting | One field in `Settings`, its validation, its `apply` branch, its UI control |
| A UI | Anything that can call the commands and subscribe to the event; the core is unchanged |

## 10. Code origin

All of VoiceBro's own code is written by the project's author; no one else's code is copied
into it (the libraries it is built with are dependencies, listed in the README). Every module
(the Codex assistant, the wake engine, speech to text and the local voice, the session gate,
the audio core, the Discord adapter, the member panel, the settings, the desktop shell and
the UI) follows this document.
