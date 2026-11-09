# Tasks

## 1. Probe binary and workspace

- [x] 1.1 Create the workspace `Cargo.toml` (`members = ["apps/probe"]`, release profile from `docs/MVP-PLAN.md` section 3) and `.gitattributes` (LF for text, CRLF for `*.ps1` and `*.cmd`); verify `cargo metadata` succeeds and `git check-attr eol` reports the expected endings
- [x] 1.2 Create `apps/probe` with a `main` that dispatches `probe <name>` to one module per probe and prints `PASS`/`FAIL` with measured values; verify `cargo run -p probe -- list` prints the probe names
- [x] 1.3 Add the probe commands to `AGENTS.md` (Commands section) with the environment variables each needs; verify every documented command runs as written

## 2. Codex protocol

- [x] 2.1 Run `codex app-server generate-json-schema --experimental --out <scratch>` with the installed 0.167.0 and record the version; verify the output directory contains the realtime types
- [x] 2.2 Diff `thread/start`, `thread/realtime/start`, `thread/realtime/sdp`, `initialItems`, `voiceSessionId`, `realtimeSessionId`, the approval-policy enum and `model/list` against the field names used in `docs/MVP-PLAN.md` 5.6; verify by recording each field as confirmed or renamed in the probe results of the plan
- [x] 2.3 Implement `probe codex-smoke`: spawn `codex app-server --disable shell_tool`, `initialize` with `experimentalApi`, `account/read`, `model/list`, open and close a read-only ephemeral thread; verify it prints `PASS` with the account state and the model list

## 3. WebRTC call and Opus

- [x] 3.1 Implement `probe opus-roundtrip` with `opus2` 0.4: encode and decode 48 kHz 20 ms mono frames of a 1 kHz tone; verify `PASS` when the decoded signal correlates with the input above a stated threshold, and record the result in the plan
- [x] 3.2 Read the `webrtc` 0.21 examples and build `probe rtc-loopback`: two peer connections in one process exchange an Opus track and a data channel; verify audio arrives on the second peer (this separates crate misuse from remote rejection)
- [x] 3.3 Implement `probe rtc-call <wav>`: one Opus send track, data channel `oai-events`, offer with ICE gathering awaited (≤ 2 s), `thread/realtime/start` with `transport: webrtc`, `version: v3`, voice `juniper`, the canary developer item last in `initialItems`; set the answer, send the WAV, decode the remote track into `<out>.wav`; verify `PASS` when DTLS connects, remote audio is written, and the `thread/realtime/transcript/done` text starts with «Банан»; record the call and canary results
- [x] 3.4 Not needed: 3.3 passed with `webrtc` 0.21 on the first run (2026-11-09), so the `str0m` attempt was never started
- [ ] 3.5 Record the chosen crate and any renamed fields in `docs/ARCHITECTURE.md` 4.5 and `docs/MVP-PLAN.md` section 3; verify the documents name one crate and no "to be confirmed" remains for the call

## 4. Credential Manager, Vosk assets, hotkey

- [x] 4.1 Implement `probe credential`: write, read, delete a `VoiceBro/test` generic credential through `windows-sys` with `CRED_PERSIST_LOCAL_MACHINE`; verify `PASS` on a matching round trip and that the entry is visible in the Credential Manager control panel between write and delete; record the result (the reboot check is done by hand and written into the plan)
- [x] 4.2 Implement `probe vosk-assets <dir>`: download both pinned archives to `<name>.part`, hash, rename, extract with `tar.exe`, load `libvosk.dll` with `libloading` 0.9 and call `vosk_model_new`; verify `PASS` on matching hashes and a non-null model; record the result and update the pins in the plan if they changed
- [ ] 4.3 Implement `probe hotkey` as a minimal Tauri window registering `Ctrl+Alt+M` through `tauri-plugin-global-shortcut` and logging each press; verify by pressing it with a full-screen game in front and record the result (game name, outcome); if it fails, note the `RegisterHotKey` fallback as the decision

## 5. songbird lifecycle and receive timing

- [ ] 5.1 Implement `probe discord-lifecycle` (needs `VOICEBRO_DISCORD_TOKEN`, `VOICEBRO_TEST_GUILD`, `VOICEBRO_TEST_CHANNEL`): join, register a counting `VoiceTick` handler and a `RawAdapter` source that logs `TrackEvent`s, `remove`, rejoin; verify `PASS` when the first handler's counter stops after `remove` and exactly one `TrackEvent::End` was logged; record the result
- [ ] 5.2 Implement `probe receive-timing`: with two speakers (a second account or a WAV played from another client), log the interval between ticks and the handler's own duration for one minute; verify the report prints p50/p99 for both and record the result with the stated pass criterion (p99 handler time well under 20 ms)

## 6. Close-out

- [ ] 6.1 Walk the probe table in `docs/MVP-PLAN.md`: every row has a dated result and a "not tested" note where applicable; verify no row is empty
- [ ] 6.2 Pin the probed Codex CLI version and the probe prerequisites in `AGENTS.md`; verify the file names the version and the environment variables

## Workflow follow-up

- Checkpoint commit per task group on `change/probe-external-apis`, with the user's OK.
- Merge into `main` with `--no-ff --log` and archive the change when the user says so.
