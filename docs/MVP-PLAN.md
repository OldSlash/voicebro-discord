# VoiceBro MVP: implementation plan

The plan for the first milestone. It follows [ARCHITECTURE.md](ARCHITECTURE.md); where the two
disagree, the architecture wins and this file is fixed. Written 2026-11-09, before any code
existed; the probe results below are the first things that were verified.

Sources: `REWRITE-SPEC.md`, `REWRITE-NOTES.md` and `STACK-DECISION.md` in
`D:\SideProjects\gptvoice-discord`, the research summary of 2026-11-09, and the source of
`gptvoice-discord` at commit `38c29b6` on `feature/codex-voice-assistant`.

## 1. What the MVP is

**The MVP is the feature set of the first version** (`gptvoice-discord`, branch
`feature/codex-voice-assistant`), rebuilt on the new architecture, with the API-key path left
out. A user installs the app, pastes a Discord bot token, signs in to Codex with a ChatGPT
account, says `/join` in Discord, and talks to the assistant with a wake phrase. The session
ends on the deactivation phrase or after a configurable idle time. Mute works from the app, a hotkey and the
Discord member panel.

### In scope

| # | Feature | Notes |
|---|---|---|
| M1 | Discord bot: `/join`, `/leave`, `/status`, member panel | Joins the caller's voice channel. Speaker names, participant list. The panel shows the assistant's state and has the "Mute GPT" button. |
| M2 | Token and settings stored locally and securely | Token in the Windows Credential Manager; settings in one JSON schema. |
| M3 | Audio both ways as specified | 48 kHz stereo 20 ms frames, tiny queues, mixing in `i32`, output jitter buffer, mute with flush, earcons. |
| M4 | Mute from three places | App button, global hotkey (default `Ctrl+Alt+M`, configurable, empty disables), Discord panel button. One app-wide state. Mute overrides the wake word. |
| M5 | Wake word with Vosk behind an engine trait | Open vocabulary, per-speaker recognizers, separate detectors, trailing silence, reactivation guard. The engine and the model are settings (one catalog entry today). Assets downloaded on first use. |
| M6 | Codex app-server assistant over WebRTC inside Rust, ChatGPT sign-in | Read-only ephemeral thread per guild, approvals declined. Conversation continues across calls for the app's lifetime. Speaker labels `start` / `changes` / `off`. |
| M7 | Settings | Mode (always / wake word), idle timeout, phrases, wake engine and model, Codex executable, model, reasoning effort, voice, instructions, speaker labels, earcons, hotkey. Saved on change, applied at once. |
| M8 | UI: Status, Settings, Codex chat tabs | Status with wake vocabulary check, Codex account, hotkey state, Sign in / Check Codex / Mute buttons. Chat tab with filters, auto-scroll, copy, open log folder. |
| M9 | Diagnostics | File log in every build; Codex chat log in daily files; the recognizer's results logged and shown; counters in the status; build stamp in the UI. |

### Not in the MVP

- **The API-key sign-in and the websocket realtime path** (`appendAudio` /
  `outputAudio/delta`, the 24 kHz pump). Only ChatGPT sign-in over WebRTC.
- Other assistant providers (Claude or anything else) and other auth paths.
- Tray mode (destroying the webview while the core runs).
- Downloading the standalone `codex` app-server binary; the MVP uses an installed Codex CLI.
- Other wake engines (sherpa-onnx, KWS), other languages, Whisper, cloud ASR.
- Persistent conversation history across app restarts, per-speaker AI context.
- macOS, Linux.

## 2. What is reused from gptvoice-discord

All four modules were written in the first version by this project's author and can be
copied. Each imports things that will not exist here; the "adaptation" column lists them.

| v1 file | Lines | Keep | Adaptation |
|---|---|---|---|
| `src-tauri/src/dsp.rs` | 267 | All of it, as `audio/dsp.rs`, including the unused 24 kHz paths and their tests | None. Already has no Tauri dependency. |
| `src-tauri/src/gate.rs` | 962 | `RelayGate`, `RelayRegistry`, `GateSettings`, `GateStatus`, `PhraseKind`, `ActivationMode`, `SpeakerNamesMode`, `SessionListener`, `WakeFeed`, all tests | Replace `BrowserMediaTransport` with `AudioPorts` (input `FrameSender`, output `OutputHandle` with `flush`, `set_muted`, `last_signal`, `close`). Replace `Earcon` import with `audio::earcon`. Replace `AppHandle`/`Emitter` (3 uses) with the event bus. `GateSettings::from_config` reads the new `Settings`. |
| `src-tauri/src/wake.rs` | 1419 | Download pins (`VOSK_LIBRARY`, `VOSK_MODEL`), `VoskApi`/`Model`/`Recognizer` FFI wrappers, `Speaker`, `Detector`, `PhraseDetectors`, `parse_recognition`, `find_occurrences`, `usable_phrases`, the worker loop, all tests | Split into the three layers of the architecture: FFI and `Model`/`Recognizer` → `wake/vosk/`; `Detector`, `PhraseDetectors`, occurrence matching → `wake/spotter.rs`; downloads → `wake/assets.rs` and `wake/catalog.rs`; the worker → `wake/worker.rs` over the traits. Replace `AppHandle` (8 uses) with the event bus. `PhraseSet::from_config` reads the new `Settings`. |
| `src-tauri/src/codex.rs` | 3102 | `AppServer` (JSON-RPC client), `parse_models`, `find_codex`, `codex_version`, `AccountInfo`, `ThreadSettings`, `thread_start_params`, `open_thread`, `Conversation`, `CallContext`, `call_instructions`, `call_names_note`, `speaker_note`, chat log, `CodexSession` state machine, `delegated_task_text`, `agent_item_line`, the tests that do not need the websocket | Drop the old call transport (`open_rtc_window`, the `codex_rtc_*` commands, `BrowserMediaTransport`; 43 Tauri references), `run_pump`, `Realtime::websocket` start params, `audio_params`, `decode_audio_chunk`, the `Output` pacing for websocket data, and the websocket self-tests. The WebRTC call becomes `rtc.rs` in Rust. `tauri::async_runtime::spawn` becomes a stored tokio `Handle`. `emit`/`chat` publish `AppEvent`. Self-test entry points move to `apps/probe`. |
| `src-tauri/build.rs`, `windows-app-manifest.xml` | | As is, plus a build stamp | None. |

Not copied, by decision: `audio.rs`, `discord.rs`, `config.rs`, `lib.rs`, `main.js`,
`index.html`, `styles.css`. Their behaviour is written anew from the spec: the member panel
(embed, buttons, custom ids, state), the speaker directory, the hotkey registration, the
config model as one schema, the UI.

## 3. Dependencies

### Toolchain (installed)

Rust 1.99.1 MSVC, MSVC Build Tools 2022 with Windows SDK, CMake (the VS Build Tools copy,
not on PATH), Tauri CLI 2.12.4, Codex CLI 0.167.0 (`npm install -g @openai/codex`, fine for
development; users need their own install).

### Crates

Latest stable versions on crates.io on 2026-11-09. Pin the minor in `Cargo.toml`; the lock file
pins the rest.

| Crate | Version | Why |
|---|---|---|
| `tauri`, `tauri-build` | 2.12 | Desktop shell |
| `tauri-plugin-log` | 2.10 | File log in every build |
| `tauri-plugin-global-shortcut` | 2.4 | Mute hotkey |
| `tokio` | 1.54 | Async runtime (`rt-multi-thread`, `sync`, `process`, `io-util`, `time`, `fs`, `macros`) |
| `serenity` | 0.12.5 | Discord gateway/HTTP, features `builder cache client gateway http model rustls_backend`, no defaults |
| `songbird` | 0.6.1 | Voice, features `serenity rustls driver gateway tungstenite receive`, no defaults |
| `opus2` | 0.4 | Opus encode/decode for the WebRTC call. The same crate and version songbird 0.6.1 uses, so libopus is built once. |
| `webrtc`, `rtc` | 0.21.1 | In-process WebRTC, decided by the live call probe on 2026-11-09. `rtc` is the sans-io core; the `webrtc` crate drives it on tokio (`PeerConnectionBuilder`, `TrackLocalStaticSample`, event handler trait). |
| `symphonia-core`, `symphonia` (`pcm`) | 0.5 | `MediaSource` for `RawAdapter` |
| `serde`, `serde_json` | 1 | Settings, JSON-RPC |
| `thiserror` | 2 | Core error types |
| `log` | 0.4 | Logging facade |
| `reqwest` | 0.13, `rustls-tls`, no defaults | Asset downloads |
| `sha2` | 0.11 | Asset hashes |
| `libloading` | 0.9 | Vosk at runtime |
| `windows-sys` | 0.61 | Credential Manager (`Win32_Security_Credentials`), process flags, `LOAD_WITH_ALTERED_SEARCH_PATH` |
| `chrono` | 0.4, `clock std` only | Daily chat-log file names |

Not needed any more: `tokio-tungstenite` and `base64` (websocket path), `futures-util`
unless the webrtc crate's API asks for it.

Check at skeleton time that `rust-version` in the manifests matches what the lock file needs
(v1 needed 1.89 for `twilight-*` and `tokio-websockets`).

### Release profile

`opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`.
The first version used `opt-level = "z"` for size; here the hot paths (FIR resamplers, mixing,
Opus) matter more than a few megabytes next to a 45 MB model and a 350 MB Codex binary. The
real-channel phase compares CPU on the receive path between `3` and `z` once, and the result
is written here.

### External assets (downloaded at first use, pinned)

| Asset | Source | SHA-256 |
|---|---|---|
| `vosk-win64-0.3.45.zip` (14.9 MB) | `github.com/alphacep/vosk-api/releases/download/v0.3.45/` | `f1dcc9cca460630f81ea8f71794f69c80bed6556d2a4e6237b5785e1d2dff34b` |
| `vosk-model-small-ru-0.22.zip` (46.2 MB) | `alphacephei.com/vosk/models/` | `961d5ff98a17f4aa6de69864d0aa71fa5bac682301d2b5d17a3f24c5c99a46d4` |

Both were re-downloaded on 2026-11-09 and matched.

## 4. Phases and dependencies

```text
probes ─────┬──────────────────────────────────────────────────────────┐
            ▼                                                          │
skeleton ───┬─► Discord adapter ──┐                                    │
            ├─► audio core ───────┼─► session gate ─► Codex assistant ◄┘ (needs the call probe)
            └─► wake engine ──────┘          │              │
                                             ▼              ▼
                                        UI, hotkey, chat tab
                                             │
                                             ▼
                                real-channel test and measurements
```

The Discord adapter, the audio core and the wake engine are independent once the skeleton
is in. The gate needs the audio core and the wake engine and is integrated through the
Discord adapter. The Codex assistant needs the audio core and the call probe. The UI needs
everything that has a setting. The real-channel test is last and is the only thing that
makes a feature "verified".

Rough sizes: probes 2–3 days, skeleton 1 day, Discord adapter 3 days (panel included), audio
core 2 days, wake engine 2 days (mostly a port), session gate 1–2 days (port), Codex
assistant 4–6 days (the RTC transport is new), UI 2–3 days, real-channel test 2 days plus the
fixes it finds. These are estimates for planning the order, not commitments.

### OpenSpec changes

The phases are delivered as seven OpenSpec changes in `openspec/changes/`, one branch each
(see `AGENTS.md`, Git flow). All seven exist with proposal, specs, design and tasks; the
first two are implemented.

| Change | Phases | State |
|---|---|---|
| `probe-external-apis` | probes | implemented; the game and reboot checks are the user's |
| `workspace-skeleton` | skeleton | implemented; the window UI not exercised by the agent |
| `discord-voice-adapter` | Discord adapter + audio core | planned |
| `wake-word-engine` | wake engine | planned |
| `session-gate-and-mute` | session gate + mute from three sources, hotkey | planned |
| `codex-assistant` | Codex assistant | planned |
| `settings-ui-and-acceptance` | full UI + real-channel acceptance + measurements + v0.2.0 | planned |

### Probes (before the design is final)

Each probe is a subcommand of `apps/probe` with a written pass criterion. A failed probe
changes the plan, not the report.

| Probe | Question | How | Pass | If it fails |
|---|---|---|---|---|
| `codex-schema`, `codex-smoke` | Does Codex 0.167.0 still expose `thread/realtime/*` with `transport: webrtc`, `version: v3`? | `codex app-server generate-json-schema --experimental --out <dir>`; diff the realtime types against v1's assumptions (`initialItems`, `voiceSessionId`, `realtimeSessionId`, `thread/realtime/sdp`, the approval-policy values); then a live handshake | Fields present with the expected names | Adjust `rtc.rs` and `conversation.rs` to the schema before porting |
| `rtc-loopback`, `rtc-call` | Can `webrtc` 0.21 complete the call? | Two peers in one process first; then a real call: `PeerConnectionBuilder`, Opus audio track, data channel `oai-events`, wait for ICE gathering, offer → `thread/realtime/start`, set answer, send a 48 kHz WAV as Opus, receive the remote track, decode, write a WAV | DTLS connected, remote audio decoded, answer audible in the WAV | Try `str0m` 0.25 with the same flow; if that fails too, the plan stops here and the decision goes back to the user. |
| canary (inside `rtc-call`) | Does the canary instruction in `initialItems` still reach the model? | Last developer item: "start every reply with the English word Banana" | The reply starts with the canary word | Find the channel that works before building continuity on it |
| `opus-roundtrip` | Does `opus2` 0.4 expose an encoder and decoder usable for 48 kHz 20 ms frames? | Encode/decode round trip | Decoded signal within tolerance | Use `audiopus` only if songbird's Opus crate cannot encode; note the second libopus build |
| `credential` | Credential Manager round trip | `CredWriteW`/`CredReadW`/`CredDeleteW` through `windows-sys` for a `VoiceBro/test` generic credential | Value round-trips; entry visible in Control Panel; deleted cleanly; survives a reboot | Fall back to DPAPI (`CryptProtectData`) blobs in the data directory |
| `vosk-assets` | Vosk assets still match the pins | Download both, hash, load `libvosk.dll` with `libloading` 0.9 | Hashes match, `vosk_model_new` succeeds | Update the pins; if the model URL moved, the mirror decision goes to the user |
| `discord-lifecycle` | Does `Songbird::remove` after `join` free handlers and tracks, and does closing the source end the track? | Join, register a `VoiceTick` handler with a counter, attach a silent source, close it, `remove`, rejoin, check the old counter stops and the track reported `End` | Old handler silent after rejoin; one `TrackEvent::End` | Keep handler handles and call `remove_all_global_events`; stop the track explicitly |
| `receive-timing` | Receive path timing | Log the interval between `VoiceTick`s and the handler's own duration over a minute with two speakers | p99 handler time ≪ 20 ms | Move the mix and encode off the event task into a channel-fed thread |
| `hotkey` | Global shortcut while a game has focus | Register `Ctrl+Alt+M` with `tauri-plugin-global-shortcut` in a bare Tauri app, press it with a full-screen game in front | The handler fires | Raw `RegisterHotKey` through `windows-sys` |

#### Probe results

Filled in as the probes run. Each entry: date, verdict, what was measured, what was not
tested.

- **Codex schema and smoke** (2026-11-09) **PASS.** Codex CLI 0.167.0,
  `codex app-server generate-json-schema --experimental` dumps 423 v2 types. Confirmed as
  used by v1: `thread/realtime/start` `{threadId, outputModality (required: "audio"),
  transport {type: "webrtc", sdp}, version: "v3", voice, prompt, initialItems [{role, text}],
  voiceSessionId, realtimeSessionId, includeStartupContext}`; the answer arrives in the
  notification `thread/realtime/sdp {threadId, sdp}` (the response body is empty);
  `thread/realtime/started {threadId, realtimeSessionId, version}`;
  `thread/realtime/transcript/done {threadId, role, text}` and `transcript/delta`;
  `thread/realtime/closed {reason}`, `thread/realtime/error {message}`;
  `thread/realtime/stop {threadId}`; `thread/start` with `approvalPolicy` enum
  `untrusted | on-request | never`, `sandbox: read-only`, `ephemeral`,
  `developerInstructions`, `model`, `config`; `model/list` → `Model {id, model,
  displayName, supportedReasoningEfforts [{reasoningEffort, description}],
  defaultReasoningEffort, isDefault, hidden}`; voices `{defaultV1, defaultV2, v1, v2, v3}`
  with `juniper` present. Server requests to decline: `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`, `item/permissions/requestApproval` (new since v1),
  `item/tool/call`, `item/tool/requestUserInput`, `mcpServer/elicitation/request`, legacy
  `execCommandApproval`, `applyPatchApproval`, plus `currentTime/read`. New optional fields
  worth a later look: `realtimeStartInstructions` / `realtimeEndInstructions` (developer
  instructions for the backing Codex model), `delegationAckFiller`,
  `backendReasoningStatus`. Live (`probe codex-smoke`, same day): handshake with
  `experimentalApi`, `account/read` → signed in, type `chatgpt`; `model/list` → 8 models
  (`gpt-6.1-sol` default, efforts low…ultra); `listVoices` v1 = juniper, maple, spruce,
  ember, vale, breeze, arbor, sol, cove, `defaultV1` = cove, 25 v3 voices;
  `thread/start` with `approvalPolicy: never`, `sandbox: read-only` accepted. The native
  `codex.exe` is found in the npm vendor folder, so `kill_on_drop` kills the real process.
- **Opus round trip** (2026-11-09) **PASS.** `opus2` 0.4 (libopus built by CMake from the VS
  Build Tools copy): 100 mono 20 ms frames at 48 kHz, VOIP mode, complexity 5, in-band FEC
  on; 49 kbit/s; decoded tone correlates 0.999 with the input at a 21-sample lag (encoder
  lookahead 312 samples). One libopus build shared with songbird. Not tested: stereo, DTX,
  packet loss.
- **WebRTC loopback and call** (2026-11-09) **PASS, crate decided: `webrtc` 0.21.1.**
  `probe rtc-loopback`: two peer connections in one process, host candidates only, connected
  in 210 ms, the Opus tone arrives with correlation 1.000, the `oai-events` data channel
  opens and carries a message. `probe rtc-call`: offer 1200 bytes (2 host candidates, no
  STUN); Codex answered `thread/realtime/start` through `thread/realtime/sdp` after 2.3 s
  with 4 UDP and 2 TCP candidates; DTLS/ICE connected 750 ms after the answer; 461 packets,
  9.2 s of the assistant's voice decoded with 0 errors, written to a WAV;
  `thread/realtime/stop` closed the session with `reason: requested`. API notes for
  `rtc.rs`: `on_track` fires with the first RTP packet, not at negotiation; the Opus codec
  is registered by hand (`MIME_TYPE_OPUS`, 48 kHz, 2 channels, `minptime=10;useinbandfec=1`,
  payload type 111) and the negotiated payload type is read from the sender's parameters;
  samples go through `TrackLocalStaticSample::sample_writer`; the data channel is created
  before the offer. `str0m` was not tried. Not tested: TCP/TURN fallback, reconnection, more
  than one call per process, a stereo send track.
- **Canary** (2026-11-09) **PASS.** The last `initialItems` entry (role `developer`, the
  instruction to start every reply with the canary word) is followed: the assistant
  transcript starts with the canary word. Also observed: the model answered the greeting
  before the question had finished (the barge-in from v1, section 7 of REWRITE-SPEC, is
  reproduced); the `oai-events`
  channel delivers JSON events with millisecond timing, among them `session.started`,
  `input_transcript.added`, `turn.created`, `output_transcript.added` (33 events in one
  call), which the assistant implementation can use for speech and transcript timing
  instead of guessing. Not tested: `prompt` alone, `realtimeStartInstructions`,
  continuity across two calls (that is the `codex-assistant` change).
- **Credential Manager** (2026-11-09) **PASS.** `CredWriteW`/`CredReadW`/`CredDeleteW` through
  `windows-sys` 0.61: generic credential `VoiceBro/test`, `CRED_PERSIST_LOCAL_MACHINE`,
  UTF-16 blob of 178 bytes round-trips and is deleted. `ERROR_NOT_FOUND` (1168) maps to
  "no secret". Not tested yet: survival across a reboot (`probe credential --keep`, reboot,
  `probe credential --read-only`) and the Control Panel view; both are the user's step.
- **Vosk assets** (2026-11-09) **PASS.** Both pins still hold (hashes above). Downloaded with
  the system `curl.exe`, extracted with `tar.exe`, `libvosk.dll` loads with
  `LOAD_WITH_ALTERED_SEARCH_PATH` in 138 ms, `vosk_model_new` in 367 ms. The probe looks up
  words passed on its command line in the model's vocabulary; the v1 wake words were known,
  transliterations of "GPT" were not. Not tested: recognition itself (that is the
  `wake-word-engine` corpus).
- **Discord lifecycle** (2026-11-09) **PASS.** songbird 0.6.1 against the test guild: join
  in 2.7 s; a `VoiceTick` handler registered before `join` ticks every 20 ms (249 in 5 s);
  a silent `RawAdapter` source is read by the mixer (265 reads in 5 s) and closing it
  (`read` returns 0) ends the track: exactly one `TrackEvent::End` within 1.5 s;
  `Songbird::remove` stops the first handler for good (its count does not move after a
  rejoin) while a handler registered on the new `Call` ticks. Two facts for the Discord
  adapter: `DecodeMode::Decode` takes a `DecodeConfig` in 0.6.1 (`Channels::Stereo`,
  `SampleRate::Hz48000`), and decoding `RawAdapter` audio needs the application to enable
  symphonia's `pcm` feature, otherwise the track fails with "no compatible track found".
  The `davey` crate prints `[DAVE Binary] …` lines to stdout on join. Not tested: a track
  with real audio, several guilds.
- **Receive timing** (2026-11-09) **PASS with a gap.** 20 s in the channel with nobody
  speaking: 1000 ticks, tick interval p50 15.7 ms, p99 32.1 ms, max 37.4 ms (the Windows
  timer jitter of tokio-driven ticks, as expected); the handler's own work (mix into `i32`,
  clamp) p50 20 µs, p99 43 µs, max 132 µs, far under the 2 ms criterion. Not tested: ticks
  with one or two speakers decoding; that needs people in the channel.
- **Hotkey** (2026-11-09) **registration PASS, game test pending.** `probe-hotkey` registers
  `Ctrl+Alt+M` through `tauri-plugin-global-shortcut` and logs two synthetic presses sent
  with `SendKeys`. Not tested: a press while a full-screen game has focus; that is the user's
  step.
- **Credential reboot check** — not run yet (`probe credential --keep`, reboot, `--read-only`).

### Skeleton

Deliverable: `cargo tauri dev` opens a window that shows the build stamp; `cargo test` runs
in the core; the log file appears in `logs\` next to the executable.

1. Workspace `Cargo.toml` with `crates/voicebro-core`, `apps/desktop`, `apps/probe`; the
   release profile from section 3; `.gitattributes` (`* text=auto eol=lf`,
   `*.ps1 text eol=crlf`, `*.cmd text eol=crlf`, binaries marked).
2. `apps/desktop`: `tauri.conf.json` (product `VoiceBro`, identifier `com.voicebro.desktop`,
   `withGlobalTauri`, NSIS target, one window), `build.rs` with the manifest embedding from
   v1 plus `VOICEBRO_BUILD_STAMP` (version, short git hash, UTC date), `capabilities/default.json`
   with `core:default` and the global-shortcut permissions.
3. Logging: `tauri-plugin-log` in every build, targets file under the data dir and stdout in
   debug, Info with `serenity`, `songbird`, `tracing`, `hyper`, `h2`, `rustls`, `webrtc*` at
   Warn, 4 MB rotation, two kept, local time. The first log line is the build stamp.
4. `voicebro-core`: `settings/` (schema, `validate`, file store with temp-and-rename, `SecretStore`
   trait with the Credential Manager implementation and an in-memory one for tests),
   `events.rs`, an empty `Runtime` with start/stop and `toggle_mute`.
5. Commands: `get_settings`, `save_settings(patch)`, `set_secret(name, value)`,
   `clear_secret(name)`, `status`, `app_info` (version, build stamp, data dir), `set_muted`.
6. UI shell: three tabs, log pane fed by `AppEvent::Log`, build stamp in the footer.
7. `AGENTS.md`: fill the Commands section (build, test, clippy, fmt, single test, probe).

Tests: settings round trip, validation (overlapping phrases, timeout range, shortcut
syntax), migration from an older `version`, secret store in-memory implementation.

### Discord adapter

Deliverable: the bot joins and leaves on command, names speakers, posts the member panel,
and a loopback test shows per-speaker PCM arriving at a stub gate and a WAV playing into the
channel.

1. `discord/bot.rs`: `DiscordRuntime` with `start(token)`, `stop()`, `status()`. serenity
   client with `GUILDS | GUILD_VOICE_STATES`, songbird registered with
   `DecodeMode::Decode(DecodeConfig::new(Channels::Stereo, SampleRate::Hz48000))`. A token
   change stops and restarts the client.
2. `discord/commands.rs`: global application commands `/join`, `/leave`, `/status`,
   registered on `ready`. `/join` resolves the caller's voice channel from the cache.
3. `discord/speakers.rs`: `SpeakerDirectory` per guild (SSRC → name, outlives the call),
   `clean_display_name`, the one-non-bot-member fallback, participant list from
   `voice_state_update`.
4. `discord/receive.rs`: `VoiceTick` handler. For each `speaking` entry with decoded PCM,
   keep `(ssrc, &[i16])`; mix into a reusable `i32` scratch buffer, clamp into a reusable
   `Frame`; call `gate.process_tick`. No allocation per tick after warm-up. Counters: ticks,
   speakers per tick, empty ticks, malformed frames.
5. `discord/output.rs`: `Track` from `RawAdapter::new(PcmSource, 48_000, 2)` attached with
   `play_only`, created once per join; a `TrackEvent` logger for end and error; the source is
   closed on leave.
6. `discord/panel.rs`: the member panel. `/join` posts (or edits) an embed in the command's
   channel with the engine, the assistant state (waiting / listening / muted) and the
   "Mute GPT" button (custom id `voicebro:mute:<guild>`). Button interactions call
   `Runtime::toggle_mute`. The panel is edited on `AppEvent::Gate` and `AppEvent::MuteChanged`,
   rate-limited to one edit per second, and removed on `/leave`.
7. Join order: `get_or_insert` → `add_global_event(SpeakingStateUpdate)` →
   `add_global_event(VoiceTick)` → `join` → attach the output track → post the panel. Leave:
   stop the assistant session, close the source, `remove` the call, drop the gate, remove the
   panel. Join and leave are serialized per guild with an async mutex held across the whole
   operation.

Tests: name cleaning, SSRC directory behaviour, custom-id parsing, mixing (two samples of
30 000 give 32 767 after clamp, not 16 383), a loopback run gated by `VOICEBRO_DISCORD_TOKEN`
and `VOICEBRO_TEST_GUILD` that joins a channel, plays a WAV and records what it receives.

### Audio core

Deliverable: `audio/` with tests; `PcmSource` proven against songbird's reader in the Discord
adapter.

1. `audio/frame.rs`: `Frame` (1920 `i16`), `FRAME_SAMPLES`, `peak`, `is_silent`.
2. `audio/dsp.rs`: v1 `dsp.rs` unchanged.
3. `audio/queue.rs`: `FrameQueue` (bounded, drop-oldest, counters) used for the gate →
   assistant input (4 frames).
4. `audio/output.rs`: `OutputQueue` (8 frames; jitter target 3, grows to 8 after 3 underruns,
   relaxes after 750 stable frames; flush epoch; `set_muted`; earcon mixing at 0.18;
   `last_signal` timestamp for the gate's idle timeout; `close`) and `PcmSource` (`Read` +
   `Seek` + `MediaSource`, one 20 ms packet per read, silence on underrun, end of stream only
   after `close`, never waits more than a few ms).
5. `audio/earcon.rs`: `Earcon::{Activate, Deactivate}` sample generation from v1's design.
6. `AudioPorts`, `FrameSender`, `OutputHandle` as in the architecture.

Tests: jitter growth and relaxation, flush drops queued audio, mute passes earcons,
drop-oldest counters, `PcmSource` returns exactly one packet per read, an empty queue gives
silence and not EOF, `close` gives EOF.

### Wake engine

Deliverable: the Vosk engine behind the traits, assets downloaded on first use, the v1 test
corpus runs again.

1. `wake/mod.rs`: traits `WakeEngine`, `LoadedModel`, `SpeechRecognizer`, `Recognition`,
   `Word`, `PhraseEvent`, `WakeStatus`, `WakeError`.
2. `wake/catalog.rs`: `ModelEntry` and the one entry (`vosk-model-small-ru-0.22`, Russian,
   Apache-2.0), plus the library asset for the `vosk` engine. The catalog carries no phrases;
   the user's phrases live in the settings file and start empty.
3. `wake/assets.rs`: download to `<name>.part`, hash, extract with `tar.exe`
   (`CREATE_NO_WINDOW`), marker file, cleanup of `.part` at start, progress events every 20 %.
4. `wake/vosk/ffi.rs`, `wake/vosk/mod.rs`: v1 `VoskApi`, `Model`, `Recognizer` wrappers
   implementing the traits; `knows_word` via `vosk_model_find_word`; log level −1; no
   partial word timestamps.
5. `wake/spotter.rs`: v1 `Detector`, `PhraseDetectors`, `find_occurrences`,
   `parse_recognition` generalized to `Recognition`.
6. `wake/worker.rs`: the dedicated thread, `WakeMessage` channel (256), per-speaker state
   (recognizer, spotter, last audio, trailing silence feeder), 20 ms tick, 30 s speaker
   idle, 1.5 s reactivation guard, `gate.on_phrase` through the registry. Logs: final
   results, decisions, drops every 10 s.
7. `WakeManager`: loads the engine only in wake-word mode, reconfigures on phrase or model
   change, reports missing vocabulary in `WakeStatus`.

Tests: the v1 corpus tests gated by `VOICEBRO_VOSK_DIR`, `VOICEBRO_WAKE_WAV`,
`VOICEBRO_WAKE_EXPECT`, `VOICEBRO_WAKE_SEQUENCES`, `VOICEBRO_WAKE_NOISE`; spotter unit tests
for whole-word matching, streak counting, once-per-utterance, deactivation-then-activation.

### Session gate

Deliverable: `gate.rs` ported, wired between the Discord adapter, the wake engine and a stub
assistant; the stub proves the replay timing.

1. Port `RelayGate`, `RelayRegistry`, `GateSettings`, `GateStatus` over `AudioPorts`.
2. Replace `media.last_assistant_signal()` with `OutputHandle::last_signal()`, `media.send_*`
   with `FrameSender::send`, `Earcon` with `audio::earcon`.
3. `SessionListener` becomes `AssistantSession`; the gate holds `Weak<dyn AssistantSession>`.
4. Wire in `Runtime`: on join build the gate from current `Settings`, register it, preload
   the speaker directory, attach the assistant session; on leave detach and unregister.
   `Runtime::toggle_mute` → `RelayRegistry::toggle_muted` → `AppEvent::MuteChanged`.
5. A `StubAssistant` in tests that becomes `ready()` after a configurable delay and records
   the frames it receives, to assert: replay starts at the phrase end, holds until ready,
   skips silence, caps at 300 frames, idle timeout counts assistant audio, mute silences
   input and flushes output.

### Codex assistant

Deliverable: a wake phrase opens a real call, the question after the phrase is answered in
the channel, the deactivation phrase cuts the answer at once, the next activation continues
the conversation.

1. `assistant/codex/app_server.rs`: port `AppServer` (`find_codex`, spawn with
   `--disable shell_tool`, `kill_on_drop`, line-delimited JSON-RPC, 30 s request timeout,
   generation counter, auto-replies to `item/*/requestApproval`, `item/tool/call`,
   `item/tool/requestUserInput`). `initialize` with `clientInfo {name: "voicebro", title:
   "VoiceBro", version}` and `capabilities.experimentalApi: true`.
2. `assistant/codex/account.rs`: `account/read`, `account/login/start`,
   `account/login/completed`; `Capabilities` from `model/list` and
   `thread/realtime/listVoices` (v1 voices with a ChatGPT sign-in: juniper, maple, spruce,
   ember, vale, breeze, arbor, sol, cove; default `juniper`; `cedar` is not available).
3. `assistant/codex/thread.rs`: port `ThreadSettings`, `thread_start_params`, `open_thread`,
   `ensure_thread` with remap on model or effort change. `cwd` is the empty
   `codex-workspace` directory. `approvalPolicy` is `never` (confirmed by the schema and
   smoke probes).
4. `assistant/codex/conversation.rs`: port `Conversation`, `CallContext`,
   `call_instructions`, `call_names_note`, `speaker_note`, the `initialItems` builder with the
   hint, history cap, and the instructions as the last developer item; retry without history.
5. `assistant/codex/rtc.rs` (new, from the call probe): `RtcCall::open(sdp_exchange) -> RtcCall`
   with `send(frame)` (downmix to mono, Opus encode at 48 kHz, 20 ms, write RTP) and an output
   task (depacketize, Opus decode, `OutputHandle::push`). States `Connecting`, `Live`,
   `Closed` surfaced in `AssistantStatus`. The data channel is opened and its messages
   logged at Debug; nothing is sent on it.
6. `assistant/codex/session.rs`: port `CodexSession` (`Idle → Starting → Live → Stopping`,
   `wanted` flag, restarts capped at 3, stop completes before a restart, flush on
   `session_wanted(false)`), with `rtc.rs` as the only transport.
7. `assistant/codex/chat_log.rs`: port the chat log; daily file
   `logs\codex-chat-YYYY-MM-DD.log` next to the executable; `AppEvent::Chat`.
8. `CodexAssistant` implementing `Assistant`: one `AppServer` shared by all guilds, one
   session per guild, started when the bot connects, stopped at exit.

Tests: JSON-RPC framing and auto-decline, `parse_models`, `initialItems` ordering and cap,
`delegated_task_text` unwrapping; the smoke test gated by `VOICEBRO_CODEX_SMOKE=1`
(handshake, `account/read`, read-only thread); the probe binary runs
`q1.wav;q2.wav` through two calls with a restart in between and checks that q2 can refer to
q1 (the remember-a-number test).

### UI, hotkey and chat tab

Deliverable: every MVP setting is editable, saved on change and applied at once; the hotkey
toggles mute; the chat tab shows the Codex log.

1. Settings tab: Discord token (write-only field with "set"/"clear" and an `is_set` badge),
   auto-connect; mode (always / wake word), idle timeout; wake engine and model (from the
   catalog), activation and deactivation phrases; Codex executable, model and reasoning
   effort (from `model/list`, with the note that they affect delegated work only), voice,
   instructions; speaker labels; earcons and volume; mute hotkey.
2. Status tab: bot state and channel, assistant state (waiting / listening / muted), wake
   state (off / downloading N % / ready / error) and missing words, Codex account with
   "Sign in" and "Check", hotkey `{shortcut, registered, error}`, Mute GPT button, counters
   (drops, silence, underruns) behind a "details" toggle, build stamp. No field shows a
   value that was not measured.
3. Chat tab: entries with kind filters (names and notes, agent work, call events),
   auto-scroll, copy, open log folder.
4. Hotkey: `tauri-plugin-global-shortcut`, registered at start-up and on every settings
   save; the previous shortcut is unregistered first; empty disables; failures go to the
   status.
5. `Runtime::apply` diff: token → restart Discord; session or wake sections → gates and wake
   manager; codex section → thread settings for the next call; speaker labels → gates;
   hotkey → re-register.

### Real-channel test and measurements

Two or more people, a game running, the acceptance list in section 7. Each run is logged;
the report lists what passed, what failed, and what was not tested. Only after this does any
feature count as working.

Measurements, with the same script for every build (private bytes per process, repeated
three times):

| State | Target from STACK-DECISION §6 |
|---|---|
| Idle, always-on mode, window open | Rust process + WebView2 + codex idle |
| Idle, wake-word mode, window open, 2–4 speakers in the channel | the above + Vosk, expected 150–200 MB for the model |
| During a call | Rust + in-process WebRTC + codex during a call (never measured before) |
| Receive-path CPU with 2–4 speakers, `opt-level = 3` versus `z` | decides the release profile |

## 5. Technical details by subsystem

Deeper than the architecture: the concrete shapes to write, so that the port is mechanical.

### 5.1 Settings and secrets

```rust
pub struct Settings { pub version: u32, pub discord: DiscordSettings, pub session: SessionSettings,
                      pub wake: WakeSettings, pub assistant: AssistantSettings, pub codex: CodexSettings,
                      pub hotkey: HotkeySettings }
pub struct DiscordSettings  { pub auto_connect: bool }
pub struct SessionSettings  { pub mode: SessionMode, pub idle_timeout_seconds: u32, pub earcons: bool, pub earcon_volume: f32 }
pub struct WakeSettings     { pub engine: String, pub model: String, pub activation_phrases: Vec<String>, pub deactivation_phrases: Vec<String> }
pub struct AssistantSettings{ pub engine: String, pub speaker_labels: SpeakerLabels }
pub struct CodexSettings    { pub executable: String, pub model: String, pub reasoning_effort: String, pub voice: String, pub instructions: String }
pub struct HotkeySettings   { pub mute: String }   // "Ctrl+Alt+M"; empty disables

pub trait SecretStore: Send + Sync {
    fn get(&self, key: SecretKey) -> Result<Option<String>, SecretError>;
    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: SecretKey) -> Result<(), SecretError>;
}
pub enum SecretKey { DiscordToken }   // target name "VoiceBro/discord_token"
```

- Credential Manager: `CredWriteW` with `CRED_TYPE_GENERIC` and `CRED_PERSIST_LOCAL_MACHINE`
  (survives reboots, stays per user and is encrypted by the OS for that user;
  `CRED_PERSIST_SESSION` is lost at logoff and `CRED_PERSIST_ENTERPRISE` roams with a domain
  profile). The blob is UTF-16 as the Windows tools expect; the limit is 2560 bytes, far above
  a Discord token.
- No default instructions: `codex.instructions` starts empty and the user writes their own.
  Every string the app sends to the model or shows in the UI is English.
- `validate`: phrases trimmed, lower-cased, non-empty, no overlap between the two lists;
  timeout 0 or 5..=3600; `wake.model` in the catalog and its engine equals `wake.engine`;
  `earcon_volume` 0..=1; `hotkey.mute` empty or parseable by the shortcut plugin.
- Migration: `version` 1 is the first; a loader that fills missing fields from `Default`.

### 5.2 Discord receive and mixing

```rust
fn on_voice_tick(&self, tick: &VoiceTick) {
    self.scratch_i32.fill(0);
    self.speakers.clear();                      // Vec<(u32, &[i16])>, reused
    for (ssrc, data) in &tick.speaking {
        let Some(pcm) = data.decoded_voice.as_deref() else { continue };
        if pcm.len() != FRAME_SAMPLES { count_bad_frame(); continue; }
        for (acc, s) in self.scratch_i32.iter_mut().zip(pcm) { *acc += *s as i32; }
        self.speakers.push((*ssrc, pcm));
    }
    for (out, acc) in self.mixed.iter_mut().zip(&self.scratch_i32) { *out = (*acc).clamp(-32768, 32767) as i16; }
    gate.process_tick(&self.mixed, &self.speakers);
}
```

Songbird 0.6 decodes at 48 kHz stereo when asked with `DecodeConfig::new(Channels::Stereo,
SampleRate::Hz48000)`; this path never asks for mono or 24 kHz decoding (its missing-packet
recovery for those modes is an unresolved concern in 0.6.0).

### 5.3 Output source and track lifetime

`PcmSource::read` copies at most one 20 ms packet from the `OutputQueue`. If the queue is
below the jitter target it returns silence and counts an underrun. If output is muted it
returns silence plus any earcon samples. A flush sets the epoch; packets queued before it are
discarded on the next read. The wait is bounded by `PCM_READ_WAIT` (30 ms in v1; measure
whether 10 ms is enough with the mixer's own buffering before changing it).

Lifetime: one track per join. The end of a session, an empty queue or a closed assistant
never ends the track (that is what keeps the call free of renegotiation and lets earcons
play between sessions). `OutputHandle::close()` on leave makes the next `read` return `Ok(0)`,
songbird ends the track, and the `TrackEvent::End` is logged. This is the fix for the first
version's "closing the input never ends playback" and "handlers survive rejoin" findings
together with `Songbird::remove`.

### 5.4 Member panel

- Posted by `/join` in the channel of the command; the message id is kept per guild.
- Embed fields: engine (`Codex`), state (`waiting for the wake word` / `listening` /
  `muted`), channel,
  participants count.
- One button row: `Mute GPT` (style changes with the state), custom id
  `voicebro:mute:<guild_id>`. The interaction is acknowledged with a deferred update, then
  `Runtime::toggle_mute` runs and the panel is edited.
- Edits are coalesced: at most one edit per second per guild; the latest state wins.
- Removed on `/leave`; if the message is gone, the next `/join` posts a new one.
- Speaker identity needs no extra bot permission; the panel needs `Send Messages` and
  `Embed Links` in the command's channel.

### 5.5 WebRTC call (confirmed by the loopback and call probes on 2026-11-09)

1. Build a peer connection with Opus (48 kHz, 2 channels in SDP, `minptime=10;useinbandfec=1`,
   payload type 111) as the only audio codec; no ICE servers (the offer had none and the call
   worked; a STUN server is optional through configuration).
2. Add one local audio track and create the data channel `oai-events` before the offer, so
   the SDP carries both m-lines.
3. `create_offer`, `set_local_description`, wait for ICE gathering complete or 2 s.
4. `thread/realtime/start` `{ threadId, outputModality: "audio", transport: { type: "webrtc",
   sdp }, version: "v3", voice, prompt, initialItems, voiceSessionId?, realtimeSessionId?,
   includeStartupContext: false }`; read the answer SDP from the `thread/realtime/sdp`
   notification (the response body is empty).
5. `set_remote_description(answer)`. The answer carries UDP 3478 and TCP 443 candidates;
   UDP is what the crate's ICE agent uses.
6. Send: each gated `Frame` → downmix to mono `i16` → Opus encode (application VOIP, 20 ms,
   complexity 5, in-band FEC) → `TrackLocalStaticSample::sample_writer(ssrc,
   payload_type).write_sample` with a 20 ms duration; the payload type comes from the
   sender's negotiated parameters.
7. Receive: `on_track` (fires with the first RTP packet) → `poll` → `OnRtpPacket` → Opus
   decode to 48 kHz → duplicate to stereo → `OutputHandle::push`. Decoder PLC for lost
   packets.
8. Stop: `thread/realtime/stop`, close the peer connection, wait for the stop to complete
   before any restart.
9. The `oai-events` channel delivers the realtime server's JSON events (`session.started`,
   `input_transcript.added`, `turn.created`, `output_transcript.added`, with millisecond
   timing); they are logged at Debug and kept for speech timing. Nothing is sent on it in
   the MVP.

Opus passthrough (forwarding Discord's packets without decoding) is a later optimization
that only applies when exactly one person speaks; the MVP always decodes, mixes and encodes.

### 5.6 Codex JSON-RPC payloads (confirmed against the 0.167.0 schema)

```jsonc
// initialize
{ "clientInfo": { "name": "voicebro", "title": "VoiceBro", "version": "0.1.0" },
  "capabilities": { "experimentalApi": true } }
// thread/start
{ "cwd": "<data>/codex-workspace", "sandbox": "read-only", "approvalPolicy": "never",
  "ephemeral": true, "developerInstructions": "<instructions>",
  "model": "<optional>", "config": { "model_reasoning_effort": "<optional>" } }
// initialItems order (v3): [ developer: HISTORY_HINT ] + history[..cap] + [ developer: instructions + call context ]
// HISTORY_HINT: "Below is the beginning of this conversation before a pause. Continue it as one conversation and do not greet again."
// call context text: "In the channel: A, B. X has just addressed you." (omitted in always-on mode without a caller)
// `changes` mode mid-call note (role developer, only on speaker change):
//   "Service note, do not answer it and do not check anything: X is speaking now."
// auto-declined server requests: item/commandExecution/requestApproval, item/fileChange/requestApproval,
//   item/permissions/requestApproval, item/tool/call, item/tool/requestUserInput, mcpServer/elicitation/request,
//   execCommandApproval, applyPatchApproval; currentTime/read is answered with the clock
// notifications consumed: thread/realtime/sdp, thread/realtime/started, thread/realtime/transcript/done,
//   thread/realtime/closed, thread/realtime/error, turn/started, turn/completed, item/completed
```

### 5.7 Wake worker loop

```text
every 20 ms (std::thread::sleep on the worker thread):
  drain the channel: for each (guild, ssrc, 48 k stereo chunk)
      speaker = speakers.entry(ssrc).or_insert(new recognizer + spotter + StereoDownsampler(3))
      speaker.last_audio = now; speaker.silence_fed = 0
      pcm16 = speaker.downsampler.process(chunk)
      if let Some(rec) = speaker.recognizer.accept(&pcm16) { handle(rec) }
  for each speaker quiet for > 60 ms and silence_fed < 1.5 s:
      accept(320 zeros); silence_fed += 20 ms; handle any recognition
  drop speakers idle > 30 s
handle(rec): for hit in spotter.update(rec, now) → if guard allows → registry.get(guild).on_phrase(kind, phrase, Some(ssrc), since_end)
```

### 5.8 Build stamp

`build.rs` in `apps/desktop` writes `VOICEBRO_BUILD_STAMP = "<CARGO_PKG_VERSION>+<git short hash>[.dirty] <YYYY-MM-DD>"`
via `cargo:rustc-env`; the UI footer and the log's first line print it. The version is bumped in
`apps/desktop/Cargo.toml` and `tauri.conf.json` together for every build handed over.

## 6. Test corpus and harness

- **Unit tests** in every core module; `cargo test -p voicebro-core` needs no assets.
- **Vosk corpus** (environment-gated): a positive WAV per activation phrase, negatives
  (a look-alike of an activation word, ordinary speech), sequences
  (`a.wav=Deactivate;b.wav=Activate`), steady noise
  at 1500 and 3000 with no trailing silence, recordings from a real Discord channel once the
  real-channel test produces them. Kept outside the repository (size); the paths come from
  environment variables.
- **Codex smoke** (`VOICEBRO_CODEX_SMOKE=1`): handshake, `account/read`, read-only thread.
- **Probe binary**: RTC call with WAVs and a restart between questions; the canary
  instruction; Credential Manager round trip; tick jitter; the memory script.
- **Discord loopback** (`VOICEBRO_DISCORD_TOKEN`, `VOICEBRO_TEST_GUILD`): join, play, record.
- **Regression A/B**: keep the previous build's installer; when a user reports a regression,
  run the same corpus on both before answering.

## 7. Acceptance (real-channel test)

Mute: mute mid-answer then unmute, nothing of the old answer plays; the hotkey works while a
game has focus; the panel button, the app button and the hotkey toggle the same state and
the panel, the app and `/status` all show it.

Wake word, on recordings with steady noise and no trailing silence: each activation phrase
fires and a look-alike word does not; a sentence that ends with the deactivation phrase
deactivates within about 0.5 s of the phrase; a wake word followed by the deactivation
phrase in the same sentence deactivates; the deactivation phrase followed by a wake word does
not restart; an activation phrase during a
session does nothing; the idle timeout ends the session and a long answer is not cut by it;
the question after the phrase reaches the assistant and the phrase does not.

Codex: the canary instruction is followed; "What is my name?" returns the caller's nickname;
"Remember 42", end the call, start a new one, "Which number?" → 42; changing the model between
calls keeps the conversation.

App: closing the window exits the app and `codex.exe`; `/leave` ends the output track and
rejoin starts a fresh one with no old handlers; the release build writes the log and the log
shows what Vosk heard; the settings file contains no token; the token survives a restart;
the measurements of the real-channel phase are recorded; a real Discord test with two or more
people and a game running passes.

## 8. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| ~~`webrtc` 0.21's new API or ICE behaviour does not complete the call~~ | Closed by the call probe: the call completes | The probe stays in `apps/probe` as the regression check for the transport |
| Codex realtime protocol changed since 0.167.0 (experimental API) | Port breaks | The schema probe; pin the tested CLI version in `AGENTS.md`; auto-detect version and warn |
| The canary in `initialItems` stops working | Instructions and names never reach the model | Keep the canary test in the probe binary and run it before each hand-over |
| Opus encode plus mix on the songbird event task is too slow with many speakers | Drops, late frames | The receive-timing probe; move to a channel-fed thread if needed |
| Vosk small-ru fails in heavy noise (known at amplitude ≥ 1500 for activation) | False negatives in games | Known limitation, reported as such; engine trait allows sherpa-onnx later |
| Global shortcut swallowed by a full-screen game or anti-cheat | Hotkey useless in the main use case | The hotkey probe; raw `RegisterHotKey` fallback |
| Panel edits hit Discord rate limits | Stale panel | Coalesce to one edit per second; the state is also in `/status` |
| Credential Manager entry limits or roaming surprises | Token lost | The credential probe; DPAPI blob fallback behind the same trait |
| Terms: one personal ChatGPT account used by a whole channel | Legal gray area | Unchanged from v1; the user decides before publishing |

## 9. Decisions taken on 2026-11-09

- The MVP is the v1 feature set without the API-key path. Other providers and auth paths are
  later work.
- Mute hotkey and the member panel are in the MVP.
- Tray mode and the standalone Codex download are not.
- `/join` joins the caller's channel; no configured auto-join.
- Release builds optimize for speed, confirmed by one measurement in the real-channel phase.
- `webrtc` 0.21 is the WebRTC crate (live call completed on the first run).
- `docs/` and `openspec/changes/` are tracked in git.

## 10. After the MVP

In rough priority: API-key sign-in with the websocket realtime path (the v1 pump and the
24 kHz resamplers are kept for it); tray mode; standalone Codex app-server download with
pinned SHA-256 (no npm); sherpa-onnx engine and a Vosk-versus-sherpa measurement on the same
noisy corpus; model catalog for other languages with a `wake.language` setting; other
assistant providers behind `Assistant`; persistent conversation history (SQLite); Opus
passthrough for the single-speaker case; macOS once the call is in Rust.
