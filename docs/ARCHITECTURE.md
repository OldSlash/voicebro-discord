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
| WebRTC call: ICE/DTLS/SRTP, Opus encode/decode | The webrtc crate's runtime tasks | Decoded output is pushed into the output queue without blocking. |
| JSON-RPC to `codex app-server` | tokio task reading stdout line by line | Requests have timeouts; server-initiated requests are answered automatically. |
| Global hotkey | The desktop app's main thread (Tauri plugin) | Only toggles the app-wide mute through `Runtime`. |

### Event bus

The core publishes `AppEvent` values on a `tokio::sync::broadcast` channel. The desktop app
subscribes and forwards them to the webview as one Tauri event; the UI also pulls a full
`Status` snapshot on demand. The core never calls into Tauri.

```rust
pub enum AppEvent {
    Log { line: String },              // human-readable line for the UI log pane
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

Serialized with a `kind` tag in `snake_case`, so the UI switches on one string.

## 4. Components

### 4.1 Audio core (`audio/`)

Owns the formats, queues and conversions shared by everything else.

- **Formats.** Discord side: 48 kHz, 2 channels, 20 ms frames (960 frames, 1920 `i16`).
  Wake engines: 16 kHz mono (320 samples per 20 ms). WebRTC: Opus at 48 kHz, 20 ms, mono
  payload.
- **`Frame`**: a fixed-size 48 kHz stereo packet. Everything between Discord and the gate and
  between the gate and the assistant is a `Frame`.
- **Mixing** several speakers: sum into `i32`, then clamp to `i16`. Never saturate before
  averaging.
- **Resamplers** (`dsp.rs`): fixed-ratio windowed-sinc FIR, 48 k stereo → 16 k mono, and the
  24 k paths from the first version (kept and tested, unused by the MVP). Deterministic, no
  external resampler.
- **`OutputQueue` / `PcmSource`**: the assistant's audio toward Discord. A small adaptive
  jitter buffer (3 to 8 frames), drop-oldest when full, a flush epoch (so a cut answer never
  plays after unmute), an output mute that still lets earcons through, and counters
  (supplied silence, drops, underruns). `PcmSource` implements songbird's `MediaSource`
  through `RawAdapter`.
- **Source lifetime.** One output track per join. An empty queue (session over, nothing to
  say) produces silence, so a temporary underrun never ends the track. Only leaving the
  channel closes the source: `OutputHandle::close()` makes the next `read` return end of
  stream, and the track ends with it. Nothing else may end it.
- **Earcons**: short tones generated in Rust (rising on activation, falling on session end),
  mixed into the output even while muted, at volume 0.18.
- **Queue budget**: about 80 ms toward the assistant, about 160 ms toward Discord. Deeper
  buffering is voice latency and is not added without a measurement.

### 4.2 Discord adapter (`discord/`)

serenity 0.12 (gateway, HTTP, cache limited to guilds, voice states and members) and
songbird 0.6 with `receive`. DAVE is handled by songbird.

- Slash commands `/join`, `/leave`, `/status`. `/join` joins the caller's voice channel.
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
  then username; printable, trimmed, at most 32 chars). It outlives the call. If a speaking
  SSRC is unknown and the channel has exactly one non-bot member, that member is the speaker.
- **Participants** are kept current from `voice_state_update` and pushed to the gate.
- The receive handler decodes per-speaker PCM at 48 kHz stereo (never mono or 24 kHz: songbird
  0.6's packet-loss recovery for those modes is unproven), mixes it into reusable buffers,
  and calls `gate.process_tick(mixed, per_speaker)`. Participant identity is preserved up
  to the gate and the wake engine; the assistant hears the mix.
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
  the Discord panel button. All three call `RelayRegistry::toggle_muted`. Mute is two gates:
  input becomes silence (the stream is never cut, so the call is not renegotiated) and
  output is discarded with a flush. Mute overrides phrases.
- **Pre-roll** of about 5 s of frames. On activation the replay starts at the end of the
  phrase (recognizer timing, or a 1 s lookback plus 80 ms margin), holds while the
  assistant is not `ready()`, and catches up by skipping silent frames. Replay is capped at
  about 6 s.
- **Idle timeout** ends the session after N seconds without input; the assistant's own audio
  counts as activity, so a long answer is never cut.
- In wake-word mode the output is muted whenever no session is active, so a deactivation is
  silent at once, and it flushes the answer in progress.
- Activation counts only while waiting, deactivation only while active. Repeating the
  activation phrase during a session does nothing.
- The gate tells the assistant `session_wanted`, `call_context(caller, participants)` before a
  session, and `speakers_changed(names)` when the set of talking people changes.

Speech detection for these decisions is a peak threshold (700) per 20 ms frame.

### 4.4 Wake-word engine (`wake/`)

Three layers, so the engine can change without touching the phrase logic:

```rust
/// A streaming speech recognizer for one speaker. Vosk today; sherpa-onnx or a KWS later.
pub trait SpeechRecognizer: Send {
    fn accept(&mut self, pcm_16k_mono: &[i16]) -> Option<Recognition>; // partial or final
    fn flush(&mut self) -> Option<Recognition>;
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
    fn load(&self, dir: &AssetDir, model: &ModelEntry) -> Result<Box<dyn LoadedModel>, WakeError>;
}

pub struct Recognition { pub text: String, pub is_final: bool, pub words: Vec<Word> }
```

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
- **Asset store**: downloads to a temp name, verifies the hash, extracts with the system
  `tar.exe`, writes a marker file, cleans up partial files at start. Progress goes to the
  event bus.
- Vosk is loaded at runtime through `libloading` (`LOAD_WITH_ALTERED_SEARCH_PATH`), so the
  app starts without the DLL. Partial word timestamps are not requested (they delay
  partials by seconds). Grammar-restricted recognition is not used (it maps ordinary speech
  onto the wake words).
- The worker logs every final result, every phrase decision, and dropped audio. Missing
  vocabulary (phrase words the model does not know) is part of `WakeStatus`.

### 4.5 Assistant (`assistant/`)

```rust
pub trait Assistant: Send + Sync {
    fn id(&self) -> &'static str;                                 // "codex"
    fn attach(&self, guild: GuildId, ports: AudioPorts) -> Result<Arc<dyn AssistantSession>, AssistantError>;
    fn detach(&self, guild: GuildId);
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
`OutputQueue` from 4.1) and can `flush` it.

**Codex implementation** (`assistant/codex/`), ChatGPT sign-in only:

- `app_server.rs`: spawns `codex app-server --disable shell_tool`, JSON-RPC over stdio with
  `experimentalApi: true`, request timeouts, a generation counter (thread ids die with the
  process), and automatic decline of every approval, tool call and user-input request.
  Account: `account/read`, `account/login/start` and `account/login/completed`. Models:
  `model/list` with their reasoning efforts. Voices: `thread/realtime/listVoices`.
- `thread.rs`: one ephemeral, read-only thread per guild (`sandbox: read-only`, approvals
  never granted, `developerInstructions`, optional `model` and
  `config.model_reasoning_effort`). A changed model or effort opens a new thread and remaps
  the guild's session; history carries over through `initialItems`. A rejected model falls
  back to the account defaults.
- `rtc.rs`: the realtime call over WebRTC inside the process (`webrtc` 0.21 over its sans-io
  core `rtc`, decided by the live call probe on 2026-11-09): one Opus audio track (registered by hand: 48 kHz,
  2 channels in SDP, `minptime=10;useinbandfec=1`), one data channel named `oai-events`
  created before the offer, a non-trickle offer (ICE gathering waited for, at most 2 s),
  `thread/realtime/start` with `transport: {type: webrtc, sdp}` and `version: "v3"`, the
  answer from the `thread/realtime/sdp` notification. Incoming Discord frames are downmixed
  and Opus-encoded into `TrackLocalStaticSample` samples; the remote track (its `on_track`
  fires with the first RTP packet) is decoded to 48 kHz and pushed to the output queue. The
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
- `chat_log.rs`: transcripts, call context, call start and end, errors, delegated agent work
  (`turn/*`, `item/completed`), unwrapped from `<realtime_delegation>`. Kept in memory
  (bounded), pushed as `AppEvent::Chat`, appended to a daily file.

The thread's `model` and reasoning effort affect only work the voice model delegates to
the Codex agent; the voice itself is the realtime model. The UI says so next to the setting.

### 4.6 Settings (`settings/`)

One schema, one place to validate, one place to apply.

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub version: u32,                      // schema version, migrated on load
    pub discord: DiscordSettings,          // auto_connect
    pub session: SessionSettings,          // mode: always | wake_word; idle_timeout_seconds (0 = off, else 5..=3600); earcons, earcon_volume
    pub wake: WakeSettings,                // engine id, model id (catalog), activation_phrases, deactivation_phrases
    pub assistant: AssistantSettings,      // engine id, speaker_labels: start | changes | off
    pub codex: CodexSettings,              // executable (empty = auto-detect), model, reasoning_effort, voice, instructions (no default)
    pub hotkey: HotkeySettings,            // mute: shortcut string, empty disables it
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
  (Vosk library and models) and `codex-workspace\` (the empty, read-only cwd for Codex
  threads). Nothing is written under the user profile.

### 4.7 Composition root (`runtime.rs`)

`Runtime` owns the registry of gates, the Discord adapter, the wake engine, the assistant,
the settings store and the event bus. It is the only place that knows every subsystem.
Start-up: load settings, read secrets, start Discord if a token is set and auto-connect is
on, load the wake engine if the mode is wake-word, start the assistant's process when the
bot connects. Shutdown: stop the assistant (and its child process), leave every call, stop
the wake worker. `Runtime::toggle_mute` is the single entry for all three mute sources.

### 4.8 Desktop shell (`apps/desktop`)

- Tauri commands are thin: validate input, call `Runtime`, return serializable snapshots.
  Errors cross the boundary as `String`.
- One event bridge task forwards `AppEvent` to the webview.
- **Global hotkey** through `tauri-plugin-global-shortcut`: registered at start-up and again
  when settings are saved, the previous shortcut unregistered first; toggle only; the status
  shows `{shortcut, registered, error}`.
- `tauri-plugin-log` in every build: Info level, chatty crates at Warn, local time, 4 MB per
  file, two old files kept, under the data directory.
- `build.rs` embeds the Common-Controls v6 manifest into every linked target and stamps the
  build (version plus short git hash plus date) for the UI.
- UI: static HTML, CSS and JS with `withGlobalTauri: true`. Tabs: Status, Settings, Chat. The
  status shows the engine, the assistant state (waiting / listening / muted), the wake
  engine state and missing vocabulary, the Codex account, the hotkey, the build stamp, and
  buttons Mute GPT, Sign in to Codex, Check Codex.
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
        └─► if session active and not muted: Frame ─► AudioPorts.input ─► assistant
                                                     (Codex: downmix, Opus, RTP)
```

### Audio out

```text
assistant (Codex: remote track ─► Opus decode 48 k) ─► OutputHandle.push(Frame)
   ─► OutputQueue (jitter 3..8 frames, flush epoch, mute, earcons)
   ─► PcmSource::read ◄─ songbird mixer thread ─► Opus, DAVE ─► Discord
```

### Control

```text
phrase hit ─► gate: Waiting→Starting ─► assistant.call_context, session_wanted(true)
   ─► Codex: ensure thread, build initialItems, open RTC call ─► ready() = true
   ─► gate replays pre-roll from the end of the phrase, then live frames
deactivation / idle timeout ─► gate: output muted + flushed, session_wanted(false)
   ─► Codex: thread/realtime/stop, keep thread and history for the next call
mute (app button | hotkey | Discord panel) ─► Runtime::toggle_mute ─► RelayRegistry
   ─► every gate: input silence, output flush ─► AppEvent::MuteChanged ─► UI, panel
```

### Settings

```text
UI patch ─► command save_settings ─► Settings::validate ─► store ─► Runtime::apply(diff)
   ─► gates (mode, timeout, earcons, labels), wake engine (phrases, model),
      Codex (thread settings), Discord (restart on token change), hotkey (re-register)
```

## 6. Contracts and constants

Values that worked in the first version. Change them only with a measurement or a corpus run.

| Contract | Value |
|---|---|
| Discord frame | 48 kHz, 2 ch, 20 ms, 1920 `i16` |
| Wake input | 16 kHz mono, 320 samples per 20 ms |
| Queue toward assistant | 4 frames (~80 ms), drop oldest |
| Queue toward Discord | 8 frames (~160 ms), drop oldest; jitter target 3..8 frames |
| Speech threshold | peak ≥ 700 |
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
- The status snapshot holds measured values only. A quantity that is not measured (memory,
  latency) is omitted, never replaced by a label.

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
