# VoiceBro architecture

How the project is built: crates, components, their boundaries and contracts. This file
describes the target shape of the code, not the order in which it is written and not the
history of the first version. Every agent working here reads it before touching code and
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
PC the user is gaming on. The assistant is ChatGPT voice through the Codex CLI app-server with
the user's ChatGPT sign-in; no API key is involved.

Fixed constraints that shape the architecture:

| Constraint | Consequence |
|---|---|
| Rust, Tauri 2, Windows first | One native process plus WebView2 for the settings UI. The core never depends on the UI. |
| Low CPU and memory while idle | Heavy things (speech model, assistant process, call transport) are loaded only while their feature is on, and unloaded when it is off. |
| Steady 20 ms audio | Pacing loops run on dedicated OS threads with `std::thread::sleep`. Tokio timers are never used for pacing. |
| Users install nothing but the app | No Node, Python or build tools at runtime. Native libraries and models are downloaded on first use with pinned SHA-256. |
| Replaceable engines | Wake-word recognition and the assistant sit behind traits. Only one implementation of each exists today (Vosk, Codex app-server). |
| Diagnosable in the field | A file log in every build, including release. The recognizer's own results are logged. The status shows measured values only. |
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
        audio/                 frame types, queues, resamplers, earcons, output source
        discord/               serenity + songbird adapter, slash commands, member panel
        gate.rs                session gate: mute, wake session state machine, pre-roll
        wake/                  wake-word engine trait, phrase spotter, model catalog, Vosk
        assistant/             assistant trait and the Codex implementation
        settings/              schema, file store, secret store
        events.rs              AppEvent and the broadcast bus
        runtime.rs             composition root: owns every subsystem, applies settings
  apps/
    desktop/                   the Tauri app (what `src-tauri` is in a plain Tauri project)
      Cargo.toml, build.rs, windows-app-manifest.xml, tauri.conf.json, capabilities/
      src/                     commands, event bridge to the webview, global hotkey, lifecycle
      ui/                      static HTML/CSS/JS, no bundler, `withGlobalTauri: true`
    probe/                     headless dev binary: live probes and self-tests, not shipped
  docs/
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

The realtime call (WebRTC) lives inside the Rust process.

### Threads and tasks

| Work | Runs on | Rule |
|---|---|---|
| Discord gateway, songbird driver, HTTP, downloads | Tauri's tokio runtime | Ordinary async code. |
| Receive path: `VoiceTick` → mix → gate → assistant input, wake feed | Songbird's event task (tokio) | Must return within a fraction of 20 ms: no blocking calls, no locks held across awaits, no allocation after warm-up. |
| Discord output: songbird's mixer pulls `PcmSource::read` | Songbird's mixer thread | A read never blocks for more than a few ms; on underrun it returns silence and counts it. |
| Wake worker: recognizers, trailing silence, phrase detectors | One dedicated OS thread per engine, 20 ms tick | Fed through a bounded channel; a full queue drops and counts. |
| WebRTC call: ICE/DTLS/SRTP, RTP in and out | The webrtc crate's runtime tasks | Incoming RTP packets are only inserted into the call's jitter buffer; nothing is decoded here. |
| WebRTC send: pacing and Opus encode | One dedicated OS thread per call (`voicebro-rtc-send`), 20 ms tick | Sends exactly one packet per tick: the oldest waiting frame or silence. |
| WebRTC receive: Opus decode, FEC, concealment | Songbird's mixer thread, through `PcmSource::read` | One decode per 20 ms read, pulled from the jitter buffer. |
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
    Assistant(AssistantStatus),
    Chat(ChatEntry),                   // one assistant chat-log entry
    Assets(AssetProgress),             // downloads: label, percent, error
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
  payload.
- **`Frame`**: a fixed-size 48 kHz stereo packet. Everything between Discord and the gate and
  between the gate and the assistant is a `Frame`.
- **Mixing** several speakers: the average, as in the first version. Sum into `i32`, divide
  by the number of speakers in the tick, then clamp to `i16` once; one speaker passes
  unchanged. Never saturate before averaging.
- **Resamplers** (`dsp.rs`): fixed-ratio windowed-sinc FIR, 48 k stereo → 16 k mono, and the
  24 k paths from the first version (kept and tested, unused by the MVP). Deterministic, no
  external resampler.
- **`OutputQueue` / `PcmSource`**: the assistant's audio toward Discord. A small adaptive
  jitter buffer (3 to 8 frames), drop-oldest when full, a flush epoch (so a cut answer never
  plays after unmute), an output mute that still lets earcons through, and counters
  (supplied silence, drops, underruns). `PcmSource` implements songbird's `MediaSource`
  through `RawAdapter`. A producer with its own clock-free buffer (the Codex call's jitter
  buffer) attaches as a `FrameProvider`: while attached, `PcmSource` pulls one frame from it
  per read before the queue, flush and mute changes reset it, and a muted read still pulls
  and discards so the provider's playout keeps time. `OutputHandle::push` stays for the stub
  assistant and the tests.
- **Source lifetime.** One output track per join. An empty queue (session over, nothing to
  say) produces silence, so a temporary underrun never ends the track. Only leaving the
  channel closes the source: `OutputHandle::close()` makes the next `read` return end of
  stream, and the track ends with it. Nothing else may end it.
- **Earcons**: the first version's four tones generated in Rust, mixed into the output even
  while muted, at volume 0.18 by default: session start 523.25, 659.25 and 783.99 Hz (70, 70,
  110 ms), session end 659.25 then 392 Hz (80, 130 ms), mute 660 then 440 Hz (90, 110 ms),
  unmute 440 then 660 Hz (90, 110 ms), each note with a 6 ms fade at both ends.
- **Gains** (`gain.rs`): `audio.input_gain` and `audio.output_gain`, 0.0 to 4.0, default
  1.0, as in the first version. `AudioGains` holds one `GainStage` per direction (the gain
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
- Slash commands `/join`, `/leave`, `/status`, registered globally on `ready` and, when
  `discord.guild_id` is set, also in that server (where they appear at once); each
  registration's result is logged.
- `/join` first awaits `VoiceHost::prepare_join(guild)`: the assistant starts its process
  and checks its sign-in. An error is the reply ("Not joined: …") and the bot stays out of
  the channel; only then does the join below run.
- **Member panel** (`panel.rs`): an embed in the channel the command came from, with the
  engine, the assistant's state (waiting for the wake word / listening / muted) and a
  "Mute GPT" button. The button toggles the app-wide mute through `Runtime`; the panel is
  refreshed from `AppEvent::Gate` and `AppEvent::MuteChanged`.
- **Join order is fixed:** get or create the `Call`, register the `SpeakingStateUpdate` and
  `VoiceTick` handlers, then `join`. Discord announces a user's SSRC only the first time
  they speak.
- **Leave** uses `Songbird::remove`, not `leave`, so the `Call`, its handlers and tracks are
  gone, and closes the output source. Rejoin builds a new gate. Join and leave are
  serialized per guild across the whole awaited operation.
- **Speaker directory** per guild maps SSRC → display name (nickname, then display name,
  then username; printable, trimmed, at most 32 chars). It outlives the call. The first
  time a source with no name is heard, and only then, the receive handler checks the
  channel's participants: when exactly one non-bot member is there, that member is taken as
  the speaker and logged "assigned as the only person in the channel"; a real mapping from a
  later speaking event replaces it.
- **Participants** are kept current from `voice_state_update` and pushed to the gate.
- The receive handler decodes per-speaker PCM at 48 kHz stereo (never mono or 24 kHz: songbird
  0.6's packet-loss recovery for those modes is unproven), mixes it into reusable buffers,
  and calls the app's `VoiceHost::on_tick(guild, mixed, per_speaker)`. `VoiceHost` is the
  one trait the adapter needs from the rest of the app (ticks, speaker names, participants,
  join and leave, the assistant state for the panel, mute); the session gate implements it.
  Participant identity is preserved up to the gate and the wake engine; the assistant hears
  the mix.
- Output is one `Track` from `RawAdapter<PcmSource>` (f32 PCM), attached with `play_only`.
  The crate that owns the track enables symphonia's `pcm` feature; without it songbird
  cannot decode the raw stream and the track fails with "no compatible track found".
- Changing the token restarts the client; a running client never keeps stale credentials.

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
  `RelayRegistry::set_muted` into every gate. Mute ends the session, as in the first
  version: the session is no longer wanted (the call closes, nothing is spent while muted),
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
- Activation counts only while waiting, deactivation only while active. Repeating the
  activation phrase during a session does nothing.
- The gate tells the assistant `session_wanted`, `call_context(caller, participants)` before a
  session, and `speakers_changed(names)` when the set of talking people changes (the
  latter only with `speaker_labels: changes`; `start` sends names in the call context only).
- The gate is the receive path's tick sink (`VoiceHost::on_tick` finds the guild's gate in
  the registry) and the wake worker's `PhraseSink` (the registry routes `on_phrase` to the
  guild's gate). In wake-word mode, when not muted, the gate forwards each speaker's frame
  to the `WakeFeed`.
- `GateStatus` per guild: mode, muted, active, listening for the wake word, session wanted,
  assistant ready, last wake phrase, frames held for replay, and counters (input frames the
  assistant did not take, replay frames dropped, replay timeouts). Every transition is
  published as `AppEvent::Gate`.

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

/// An engine: how to fetch its assets and load a model from the catalog.
pub trait WakeEngine: Send + Sync {
    fn id(&self) -> &'static str;                 // "vosk"
    fn assets(&self, model: &ModelEntry) -> Vec<Asset>;  // library + model, pinned SHA-256
    fn load(&self, assets_dir: &Path, model: &ModelEntry) -> Result<Box<dyn LoadedModel>, WakeError>;
}

pub struct Recognition { pub text: String, pub is_final: bool, pub words: Vec<Word> }

/// Receives confirmed phrases (the session gate). Returns whether the phrase changed anything.
pub trait PhraseSink: Send + Sync {
    fn on_phrase(&self, guild: u64, kind: PhraseKind, phrase: &str, ssrc: u32, since_phrase_end: Duration) -> bool;
}

/// Receives each speaker's 48 kHz stereo frames from the receive path.
pub trait WakeFeed: Send + Sync {
    fn feed(&self, guild: u64, ssrc: u32, frame: &[i16]);
}
```

- **`WakeManager`** owns the engines, the assets directory and the worker. `configure(settings)`
  is called at start and whenever the `session` or `wake` section changes: it stops the
  engine outside wake-word mode (`Off`), stops it when both phrase lists are empty
  (`NoPhrases`), sends new phrases to the running worker when only the phrases changed, and
  otherwise downloads, loads and starts on a thread of its own (`Downloading {label,
  percent}` → `Loading` → `Ready`, or `Error {message}`). Every transition is published as
  `AppEvent::Wake(WakeStatus)` and visible in `Status.wake`. The manager implements
  `WakeFeed`; `feed` drops when the worker's queue is full and counts the drop.

- **`PhraseSpotter`** is engine-independent. One recognizer per speaker, one detector per
  phrase kind (activation, deactivation), whole-word matching on open-vocabulary text. A
  phrase counts in a final result, or in 15 partials in a row for activation and 5 of the
  last 10 for deactivation. Each occurrence is reported once per utterance; the recognizer is
  never reset after a hit. An activation after a deactivation in the same utterance is
  ignored, and activations are ignored for 1.5 s after any deactivation.
- **Trailing silence.** Discord stops sending audio when a user stops talking. After 60 ms of
  quiet the worker feeds up to 1.5 s of silence on its 20 ms tick so phrases at the end of a
  sentence are finalized.
- A recognizer exists only while its speaker is talking and is dropped after 30 s idle. The
  whole engine is loaded only in wake-word mode.
- **Model catalog** (`catalog.rs`): vetted entries with engine, language, display name,
  download URLs, SHA-256 and license. The settings refer to a catalog id. The catalog
  carries no phrases: the activation and deactivation phrases are the user's, live in the
  settings file and start empty; the wake engine shows "no phrases configured" until the
  user enters them.
  Today: `vosk-model-small-ru-0.22` plus the Vosk 0.3.45 Windows library.
- **Asset store** (`assets.rs`): downloads with the system `curl.exe` to a `.part` name,
  verifies the SHA-256 (a mismatch deletes the file and extracts nothing), renames, extracts
  with the system `tar.exe`, checks the marker file, and cleans up `.part` files at start.
  Progress goes to the event bus as `AppEvent::Assets {label, percent, error}` in steps of
  at least 20 %. No HTTP crate.
- Vosk is loaded at runtime through `libloading` (`LOAD_WITH_ALTERED_SEARCH_PATH`), so the
  app starts without the DLL. Partial word timestamps are not requested (they delay
  partials by seconds). Grammar-restricted recognition is not used (it maps ordinary speech
  onto the wake words).
- The worker logs every final result, every phrase decision, and dropped audio (once per
  10 s, as seconds of skipped speech). Phrases with a word the model does not know are not
  spotted; those words are listed in `WakeStatus.unknown_words` and in the "wake word ready"
  log line.
- Corpus tests: `wake::vosk::tests::vosk_corpus_when_assets_are_provided` runs only when
  `VOICEBRO_VOSK_DIR` points at an assets directory, with `VOICEBRO_WAKE_PHRASES`,
  `VOICEBRO_WAKE_WAV`, `VOICEBRO_WAKE_EXPECT`, `VOICEBRO_WAKE_SEQUENCES` and
  `VOICEBRO_WAKE_NOISE` describing the samples (16 kHz mono WAV). Without the variables the
  test passes without doing anything, so the suite stays runnable on any machine.

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
}

/// Audio in and out of the assistant, in 48 kHz stereo frames.
pub struct AudioPorts { pub input: FrameReceiver, pub output: OutputHandle }
```

`AudioPorts` replace the first version's local WebSocket transport: the gate pushes gated
frames into `input`, the assistant pushes decoded answer audio into `output` (the
`OutputQueue` from 4.1) and can `flush` it. `Runtime::on_join` builds the ports and the gate,
calls `attach`, and gives the session to the gate; `on_leave` calls `detach`.

**Stub implementation** (`assistant/mod.rs`, `StubAssistant`): becomes ready a configurable
time after a session is wanted, drains its input on a thread, answers nothing and records
every call. It is what the tests use and what the runtime falls back to when no real
assistant is given, so the app never panics on an unknown engine setting.

**Codex implementation** (`assistant/codex/`), ChatGPT sign-in only:

- `app_server.rs`: spawns `codex app-server --disable shell_tool`, JSON-RPC over stdio with
  `experimentalApi: true`, request timeouts, a generation counter (thread ids die with the
  process), and automatic decline of every approval, tool call and user-input request.
  Account: `account/read`, `account/login/start` and `account/login/completed`. Models:
  `model/list` with their reasoning efforts. Voices: `thread/realtime/listVoices`.
- `thread.rs`: one ephemeral, read-only thread per guild (`sandbox: read-only`,
  `approvalPolicy: "untrusted"` as in the first version's working code, so the agent's
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
  Codex's built-in routing and what the app and the first version sent before. With the
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
  (complexity 10, 32 kbit/s, in-band FEC, 5 % expected loss), or encoded silence when the
  FIFO is empty. After the call connects and after every underrun it waits for 2 queued
  frames (40 ms) before sending speech again (`pacing.rs`, a pure function with its own
  tests), so a late songbird tick does not starve the next send tick.
  *Receive:* the remote track (its `on_track` fires with the first RTP packet) only inserts
  packets into the call's jitter buffer (`jitter.rs`), keyed by extended sequence number.
  The buffer is attached to the output as its `FrameProvider` and is pulled by songbird's
  mixer thread: it starts playing at its target depth (3 frames, growing by one up to 8
  after a late packet or a playout underrun, relaxing after 750 stable frames), decodes the
  next packet, slices 10 or 40 ms packets into 20 ms frames, decodes a missing packet from
  the next one's FEC or conceals it (at most 5 frames in a row), and plays silence without
  concealment for a pause with contiguous sequence numbers. Late packets are dropped.
  *Counters* (`CallAudioCounters`): frames sent, input drops, input underruns, packets
  received, late, concealed, concealed by FEC, playout underruns, output drops, decode
  errors. They are part of `CallStatus.audio` while a call runs (the last call's after it)
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
  `<realtime_delegation>`. Kept in memory (bounded at 600), pushed as `AppEvent::Chat`,
  appended to `logs\codex-chat-YYYY-MM-DD.log`.
- `session.rs`: the state machine idle → starting → live → stopping. `session_wanted(true)`
  checks the sign-in, ensures the thread, opens the WebRTC call, sends
  `thread/realtime/start` with the history (retried once without it), applies the answer
  from `thread/realtime/sdp`, waits for the peer to connect and for
  `thread/realtime/started`. `ready()` is live and connected. A `closed` notification or a
  lost transport restarts the call while wanted, at most three times per session, with a
  backoff of 2 s times the attempt; the fourth failure sets the connection error. A stop
  closes the peer, sends `thread/realtime/stop` and flushes the output; a restart waits for
  the stop. Barge-in is left to the realtime server (it stops its own output when the user
  speaks); the first version's client-side flush is not ported.
- `CodexAssistant` (`codex/mod.rs`) owns the process, the account, the voices and models,
  the conversations per guild and the chat log. **Nothing of Codex runs before `/join`.**
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
- Tests: `testdata/fake-codex` is a Node stand-in for the app-server (handshake, account,
  voices, models, `thread/start`; `fake_codex_signed_out.cmd` reports no account). The
  tests that use it check nothing where Node is not installed.
  `VOICEBRO_CODEX_SMOKE=1 cargo test -p voicebro-core real_codex_starts` runs the real
  start at a preparation and the stop after the last leave.
- Live check: `VOICEBRO_CODEX_CALL_WAV=<q1.wav> [VOICEBRO_CODEX_EXPECT1="a|b"
  VOICEBRO_CODEX_CALL_WAV2=<q2.wav> VOICEBRO_CODEX_EXPECT2="a|b"
  VOICEBRO_CODEX_CALL_WAV3=<q3.wav>] cargo test -p voicebro-core live_call_when_enabled --
  --nocapture` runs a real call through the engine without Discord: the question frames
  are fed on a jittery tick (20 ms ± 7 ms), the output is read every 20 ms like songbird
  does, and the test checks the canary, audio back, zero input drops, continuity and prints
  the call's counters and the chat log (a delegated question shows the agent's web search).
  `VOICEBRO_CODEX_SMOKE=1` runs the handshake only. `VOICEBRO_RTC_LOOPBACK=1 cargo test -p
  voicebro-core loopback_call_loses_nothing` sends a 10 s tone between two peers in one
  process through the same send path and jitter buffer and checks zero drops, zero late,
  zero concealed frames and the correlation both ways.

The thread's `model` and reasoning effort affect only work the voice model delegates to
the Codex agent; the voice itself is the realtime model. The UI says so next to the setting.

### 4.6 Settings (`settings/`)

One schema, one place to validate, one place to apply.

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub version: u32,                      // schema version, migrated on load
    pub discord: DiscordSettings,          // guild_id (an older auto_connect is ignored and dropped at the next save)
    pub session: SessionSettings,          // mode: always | wake_word; idle_timeout_seconds (0 = off, else 5..=3600); earcons, earcon_volume
    pub audio: AudioSettings,              // input_gain, output_gain: 0.0..=4.0, default 1.0 (a file without the section gets the defaults)
    pub wake: WakeSettings,                // engine id, model id (catalog), activation_phrases, deactivation_phrases
    pub assistant: AssistantSettings,      // engine id, speaker_labels: start | changes | off
    pub codex: CodexSettings,              // executable (empty = auto-detect), model, reasoning_effort, voice (empty = server default, the default), instructions (no default), handoff_mode: items (default) | commentary | bem_tags | thinking
    pub hotkey: HotkeySettings,            // mute: shortcut string, empty disables it
    pub ui: UiSettings,                    // theme: dark | darcula | light | intellij (default dark); font_size: 11..=20 (default 13)
    pub extra: Map<String, Value>,         // flattened: unknown fields from a newer version, kept as they are
}
```

Every string in the project (UI, log, notes to the model, code, docs) is English. The wake
phrases are user data in the settings file, in whatever language the user's model
recognizes; the code ships none.

- Stored as JSON at `settings.json` next to the executable. Written to a temp file and
  renamed over the old one. Unknown fields are kept; missing fields take defaults.
- `Settings::validate()` is the only validator (phrase lists must not overlap, timeout
  range, catalog id exists, shortcut parses). `Runtime::apply(settings)` diffs against the
  current settings and reconfigures only the subsystems whose section changed.
- Adding a setting means: a field with a default, its validation, its `apply` effect, and its
  UI control. Nothing else.
- **Secrets** go through `SecretStore` (Windows Credential Manager via `windows-sys`, entry
  `VoiceBro/discord_token`). The UI writes a secret with one command and only ever reads back
  `is_set: bool`. Secrets never appear in `Settings`, in the status snapshot, in events or in
  the log. Codex's own ChatGPT sign-in is stored by Codex itself (`~/.codex`), not by the app.
- The app is portable: its data directory is the folder the executable runs from
  (`VOICEBRO_DATA_DIR` overrides it for tests). It holds `settings.json`, `logs\`, `assets\`
  (Vosk library and models), `codex-workspace\` (the empty, read-only cwd for Codex
  threads) and `webview-data\` (the window's WebView2 data, about 11 MB). Nothing is written
  under the user profile; builds before change `ui-layout-port` left a
  `%LOCALAPPDATA%\com.voicebro.desktop` folder (WebView2 data) that the user may delete.

### 4.7 Composition root (`runtime.rs`)

`Runtime` owns the registry of gates, the Discord adapter, the wake engine, the assistant,
the settings store and the event bus. It is the only place that knows every subsystem.
Start-up: load settings, read secrets, start Discord whenever a token is set, load the wake
engine if the mode is wake-word. The assistant's process is not started: it starts at the
first `/join` (`GateHost::prepare_join` calls `Assistant::prepare`). Shutdown: stop the assistant (and its child process), leave every call, stop
the wake worker. `Runtime::toggle_mute` is the single entry for all three mute sources.

### 4.8 Desktop shell (`apps/desktop`)

- Tauri commands are thin: validate input, call `Runtime`, return serializable snapshots.
  Errors cross the boundary as `String`.
- One event bridge task forwards `AppEvent` to the webview.
- **Global hotkey** through `tauri-plugin-global-shortcut`: registered at start-up and again
  when a saved settings document changes `hotkey.mute`, the previous shortcut unregistered
  first; an empty value registers nothing; toggle only, on the key-down edge; the outcome
  goes to `Runtime::set_hotkey_status` and the status shows `{shortcut, registered, error}`.
- `tauri-plugin-log` in every build: Info level, chatty crates at Warn, songbird's receive
  task at Error, local time, 4 MB per file, two old files kept, under the data directory.
  A second target (`TargetKind::Dispatch`, `log_ring.rs`) gets the same records with the
  same filters and format, keeps the last 500 lines and emits each as `app-log`; the
  `log_history` command returns the ring, so the window's log pane shows the whole app log
  (what the recognizer heard, every phrase decision) including what was logged before the
  window loaded. The pane no longer lists `AppEvent::Log` separately.
- `build.rs` embeds the Common-Controls v6 manifest into every linked target and stamps the
  build (version plus short git hash plus date) for the UI.
- **The main window is built in `setup`** (`build_main_window`, label `main`) with
  `WebviewWindowBuilder::data_directory(<data dir>\webview-data)`; `tauri.conf.json` lists no
  window, because its `dataDirectory` is relative to `%APPDATA%`.
- UI: static HTML, CSS and JS with `withGlobalTauri: true`, no framework or bundler. The
  window has the first version's look, written new for this app: a tab bar with **Console,
  Codex chat, Status,
  Performance, System, Settings, Config**, cards, and a status bar (Discord state in colour,
  guild count, voice channel count, uptime, a memory slot, the build stamp). Switching tabs
  keeps each tab's content and scroll position. Four themes (Dark, Darcula, Light,
  IntelliJ) as CSS variables on `html[data-theme]` and a font size, both from the `ui`
  settings section, applied at once and at load.
  - Console, as in the first version, is a read-only text area with the app log
    (`log_history` at load, then each `app-log` record; at most 2 000 lines in the view),
    search that filters the lines and shows the match count, Clear (the view only), Copy
    (the shown lines), Copy Selection and Auto-scroll (off keeps the view where the user
    scrolled).
  - Status renders from one `status` snapshot (`renderStatus`): the connected guilds on the
    left (`DiscordStatus.guilds`, read from the serenity cache), and fact tiles on the right:
    Discord state, bot account, voice channel, people in it, assistant engine, the gates
    (session state, assistant ready, last phrase, held frames), wake word, mute, Codex
    account, call, Codex thread, mute hotkey, token stored, build; then connection errors
    and the counters behind a details toggle. Events trigger a refresh; a 5 s timer backs
    it up. Buttons: Mute, Connect, Disconnect, Sign in to Codex, Check Codex, Refresh.
  - Performance, as in the first version: a window selector (30 s, 60 s, 5 min), Refresh,
    a health dot and a quality label, two canvas charts drawn with the first version's
    `drawChart` (no library), and stat cards. The window samples the status once a second
    (the same poll refreshes the status values) and keeps five minutes. Quality: "idle"
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
    "not running" while Codex is stopped, and the total, which the status bar shows too)
    and the files (data folder, `settings.json`, the Codex CLI path).
  - Settings has the cards Appearance, Audio, Assistant, Codex and Hotkey, one control per
    field with its JSON path (`data-path`) and kind (string, number, bool, list, percent).
    Each control saves itself on `change` with one `save_settings` patch (text when leaving
    the field or on Enter, sliders on release, selects and switches at once); the line
    under the field shows "Saving…", "Saved" for two seconds, or the validation message,
    which stays, with the rejected value, until the field is corrected. Lists: the wake engine
    and model come from `wake_models` (the catalog); the Codex model, reasoning effort and
    voice are selects filled from `capabilities` when the engine has them and text inputs
    with a datalist otherwise; the effort list follows the chosen model. The Codex card's
    "Answer handoff" list (`codex.handoff_mode`) offers Conversation items (recommended),
    Commentary, Tagged channels and Built-in (thinking); the hint under it says what the
    chosen mode does and how it behaved in the 2026-11-10 tests. Phrases are one
    per line or comma-separated and are shown trimmed and lower-cased after save; the words
    the model does not know are listed under the field. The Audio group has sliders of kind
    `percent` (shown 0 to 400 %, stored as the multiplier): "What the assistant hears",
    "How loud the assistant speaks" and the tone volume, saved on release without a Save
    button, with the earcon switch, and under each gain the last second's peak and the clip
    count from `Status.audio`.
  - **Form state is kept apart from the status.** `main.js` holds the stored settings
    (from `get_settings`, refreshed on `settings_changed` and returned by each
    `save_settings`) and a state per field: clean, dirty (unsaved), saving or error
    (rejected). Status refreshes (events and the 5 s timer) never write a form field; a
    field is written from the stored settings only when it is clean, not focused and, for
    a list, not open (a press or focus marks a select busy until a choice or blur). Lists
    are built when their source changes: the wake catalog once, the Codex lists when
    `capabilities` differ from the last ones; a rebuild that would hit an open list waits
    for it to close and keeps the selection.
  - Config holds the Discord token (write-only, with help on getting one, a stored badge and
    Clear stored token), the guild id (with help on copying it) and the Codex executable,
    an "Auto-start enabled" badge, Save changes (one settings patch for the edited fields
    plus `set_secret` when a token was typed) and Reset (back to the stored values), and the
    stored `settings.json` read-only with its path. The token is never in that view.
  - Codex chat is the first version's chat view: the chat log entries (pulled at start,
    then one `chat` event per entry, at most 600 rows), one row each with the time, a
    coloured source label and the text. Labels: the speaker's name for a user transcript,
    `GPT` (the voice answer), `Context → GPT`, `Task → Codex`, `Codex`, `Codex thinking`,
    `Codex tool`, `Call`, `Error`; the voice answer and delegated tasks are highlighted.
    Filters for names and notes, agent work and call events, Auto-scroll, Copy (one
    `[time] label: text` line per shown row) and Open log folder.
  - Commands: `get_settings`, `save_settings`, `set_secret`, `clear_secret`, `status`,
    `app_info`, `system_status`, `set_muted`, `toggle_mute`, `discord_connect`,
    `discord_disconnect`, `codex_sign_in`, `codex_check`, `codex_models`, `capabilities`,
    `wake_models`, `chat_log`, `log_history`, `open_log_folder`.
- Checking the window without a person: start the app with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`; the page is then
  reachable through the Chrome DevTools protocol (evaluate expressions, take screenshots).
- Checking the UI without the app: `apps/desktop/ui-dev/` (outside `frontendDist`, never
  shipped) holds `mock.js`, a stand-in for every command the page calls and its events
  (an in-memory settings document with the app's validation messages for the timeout,
  phrase overlap, gains, theme and font size; status events every second;
  `window.__mock` with the calls made, the stored document and a capabilities change),
  `make.mjs`, which writes `ui-dev/index.html` from `ui/index.html` with the mock loaded
  first, and `serve.mjs`, which serves it on `http://127.0.0.1:5174/` for browsers that do
  not run scripts from `file://`.
- Window lifecycle: destroying the main window exits the app, which stops `codex.exe`.

### 4.9 Probe binary (`apps/probe`)

A headless dev binary over `voicebro-core`: Codex handshake and account check, WebRTC call
with a WAV, Vosk corpus runs, Credential Manager round trip, timer jitter, memory
measurement. Not shipped. It exists so that every external API is probed live before code
is built on it.

## 5. Data flow

### Audio in

```text
Discord UDP ─► songbird (DAVE, Opus decode) ─► VoiceTick: per-SSRC 48 k stereo
   ─► mix (i32 sum, clamp) ─► RelayGate.process_tick(mixed, per_speaker)
        ├─► per speaker ─► WakeFeed (bounded channel) ─► wake worker thread
        │                     ─► 16 k mono ─► recognizer ─► PhraseSpotter ─► gate.on_phrase
        └─► if session active and not muted: Frame ─► AudioPorts.input (FIFO, 5 frames)
               ─► assistant (Codex: send thread, 20 ms clock, 2-frame prefill,
                  oldest frame or silence ─► downmix ─► Opus ─► RTP)
```

### Audio out

```text
Codex: remote track ─► RTP packets ─► JitterBuffer (by sequence number, target 3..8)
   ◄─ pulled as FrameProvider: decode, FEC, concealment, 20 ms frames
stub assistant, tests ─► OutputHandle.push(Frame) ─► OutputQueue (jitter 3..8 frames)
   both ─► PcmSource::read (flush epoch, mute, earcons) ◄─ songbird mixer thread
   ─► Opus, DAVE ─► Discord
```

### Control

```text
/join ─► VoiceHost::prepare_join ─► Codex: start app-server, account/read, open thread
   ─► refused: reply with the reason, no join │ ready: songbird join, gate, attach
phrase hit ─► gate: Waiting→Starting ─► assistant.call_context, session_wanted(true)
   ─► Codex: ensure thread, build initialItems, open RTC call ─► ready() = true
   ─► gate replays pre-roll from the end of the phrase, then live frames
deactivation / idle timeout ─► gate: output muted + flushed, session_wanted(false)
   ─► Codex: thread/realtime/stop, keep thread and history for the next call
mute (app button | hotkey | Discord panel) ─► Runtime::toggle_mute ─► RelayRegistry
   ─► every gate: session ends (session_wanted(false)), nothing sent, output flushed,
      mute tone ─► AppEvent::MuteChanged ─► UI, panel
last /leave ─► Codex: app-server stops 0.5 s later; conversations kept
```

### Settings

```text
UI patch ─► command save_settings ─► Settings::validate ─► store ─► Runtime::apply(diff)
   ─► gates (mode, timeout, earcons, labels), gains (shared atomics), wake engine (phrases, model),
      Codex (thread settings), Discord (restart on token change), hotkey (re-register)
```

## 6. Contracts and constants

Values that worked in the first version. Change them only with a measurement or a corpus run.

| Contract | Value |
|---|---|
| Discord frame | 48 kHz, 2 ch, 20 ms, 1920 `i16` |
| Wake input | 16 kHz mono, 320 samples per 20 ms |
| Queue toward assistant | 5 frames (~100 ms) FIFO, drop oldest; the Codex send thread prefills 2 frames |
| Opus toward the call | mono VOIP, complexity 10, 32 kbit/s, in-band FEC, 5 % expected loss |
| Call receive jitter buffer | target 3..8 frames, relaxes after 750 stable frames; at most 25 packets; concealment at most 5 frames in a row |
| Queue toward Discord | 8 frames (~160 ms), drop oldest; jitter target 3..8 frames (stub and tests; the call uses its own jitter buffer) |
| Gains | input and output 0.0..4.0, default 1.0; input applied at the gate's send, output in `PcmSource` before earcons |
| Speech threshold | peak ≥ 700 (measured on the unscaled channel audio) |
| Utterance gap | 700 ms |
| Pre-roll / replay cap | 250 frames (~5 s) / 300 frames (~6 s) |
| Lookback without timing | 1 s + 80 ms margin |
| Activation / deactivation partials | 15 in a row / 5 of the last 10; a final always counts |
| Partial lag | ~150 ms |
| Reactivation guard | 1.5 s after a deactivation |
| Trailing silence | after 60 ms quiet, up to 1.5 s on a 20 ms tick |
| Speaker recognizer idle | dropped after 30 s |
| Wake queue | 256 chunks, drop and count |
| Idle timeout | default 60 s; 0 = off; 5..3600 |
| Earcon volume | 0.18 |
| Default mute hotkey | `Ctrl+Alt+M`; empty disables |
| Codex history cap | 100 items or 24 000 bytes |
| Realtime version | `v3` (`v1`/`v2` are rejected) |

## 7. Observability

- `log` facade everywhere in the core; the desktop app installs `tauri-plugin-log`.
- Log the value that was actually used, never the one that was requested and replaced.
- The wake worker logs what it heard; the UI shows the recognizer's results, not only the
  assistant's transcript.
- Connection errors (Codex process, RTC call, Discord) surface in the status snapshot, not
  only in the log.
- Counters: packets in/out, drops per queue, supplied silence, underruns, wake drops. They
  are part of the status snapshot.
- **Turn latency** (`latency.rs`, ported from the first version's tracker with its
  constants): one `LatencyTracker` per guild, owned by the gate. The gate reports every
  frame with speech (peak ≥ 700, before the gain) it sends to the assistant; `PcmSource`
  reports every played frame of the assistant's audio with a signal (peak ≥ 256), with the
  network arrival of its packet from the jitter buffer (`FrameProvider::arrival`). A turn
  starts after more than 350 ms without speech; the first answer audio within 15 s of its
  last speech is its response; speech while the answer plays (its audio less than 1 s ago)
  is reported once as an interruption; answer audio with no turn waiting is autonomous.
  The log lines, as in the first version, prefixed with the guild:

  ```text
  [latency:<guild>] turn <n> (response|response after interruption): first Discord speech -> ChatGPT audio <ms> ms; last Discord speech -> ChatGPT audio <ms> ms; ChatGPT audio -> Discord relay handoff <ms> ms; first speech -> Discord relay handoff <ms> ms
  [latency:<guild>] turn <n>: Discord speech interrupted ChatGPT output
  [latency:<guild>] ChatGPT initiated autonomous speech
  [latency:<guild>] autonomous ChatGPT speech: ChatGPT audio -> Discord relay handoff <ms> ms
  ```

  "ChatGPT audio -> Discord relay handoff" is the time the answer waited in the jitter
  buffer. The last 50 measurements per guild, with their local time, are
  `Status.latency`.
- **Memory** is the private working set, the number Task Manager shows as "Memory", and
  only that is compared or reported. `apps/desktop/src/memory.rs` walks the process tree
  from the app's id with a ToolHelp snapshot (the app, its `msedgewebview2.exe`
  descendants, its `codex*` child and everything under it) and reads each process with
  `GetProcessMemoryInfo` into `PROCESS_MEMORY_COUNTERS_EX2` (`PrivateWorkingSetSize`)
  every 3 s on its own thread; the `memory_status` command returns the latest reading.
  `scripts\measure.ps1` reads the same metric from the `Working Set - Private` counter for
  release builds; private bytes (the commit charge) are printed only for diagnosis.
- The status snapshot holds measured values only. A quantity that is not measured is
  omitted, never replaced by a label.

## 8. Conventions

- Errors: `thiserror` enums per module in the core; `String` only at the Tauri boundary.
- No panics on the audio paths; a bad frame is dropped and counted.
- Bounded queues everywhere, with an explicit overflow policy (drop oldest) and a counter.
- Locks are never held across an `await` or across FFI calls into Vosk.
- FFI (Vosk) is wrapped in safe types with `Drop`; raw pointers never leave `wake/vosk/`.
- Release builds optimize for speed (`opt-level = 3`, thin LTO, one codegen unit, symbols
  stripped, `panic = "abort"`), not for size: the hot paths are FIR filters, mixing and
  Opus, and the binary is small next to the model and the Codex process anyway.
- Unit tests sit next to the code. Tests that need assets or a live account are gated by
  environment variables and skipped otherwise.
- Files are edited with the editor tool; `.gitattributes` fixes line endings.
- Every build handed over has a new version or a visible build stamp.

## 9. Extension points

| To add | Touch |
|---|---|
| A wake engine (sherpa-onnx, a KWS) | `wake/<engine>/` implementing `WakeEngine`, `LoadedModel`, `SpeechRecognizer`; catalog entries; nothing in `PhraseSpotter` or the gate |
| A model or language | A `ModelEntry` in the catalog (URL, SHA-256, license) |
| An assistant or an auth path (API key, another provider) | `assistant/<engine>/` implementing `Assistant` and `AssistantSession` over `AudioPorts`; nothing in the gate or Discord |
| A setting | One field in `Settings`, its validation, its `apply` branch, its UI control |
| A UI | Anything that can call the commands and subscribe to the event; the core is unchanged |

## 10. Provenance

Modules carried over from the first version (`gptvoice-discord`, branch
`feature/codex-voice-assistant`, written by this project's author): `codex.rs` without its
websocket pump and its old call transport, `wake.rs`, `gate.rs`, `dsp.rs`, and the manifest
embedding in `build.rs`. They are adapted to this architecture: the Tauri `AppHandle` and `Emitter` become the event bus,
`tauri::async_runtime::spawn` becomes a tokio handle, and the local WebSocket media
transport becomes `AudioPorts`.

Everything else (Discord adapter, member panel, audio core, settings, desktop shell, UI) is
written new from this document.
