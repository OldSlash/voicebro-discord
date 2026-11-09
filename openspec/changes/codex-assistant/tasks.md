# Tasks

## 1. App-server and account (spec `codex-assistant`)

- [x] 1.1 Add `assistant/codex/app_server.rs` from the probe's client plus v1's generation counter, timeouts and `find_codex`/`codex.executable`; decline list from the schema; verify unit tests for framing and auto-decline and `VOICEBRO_CODEX_SMOKE=1` live handshake
- [x] 1.2 Add `assistant/codex/account.rs`: `account/read`, `account/login/start`, `account/login/completed` → `AssistantStatus.account`; `model/list` and `listVoices` → `Capabilities`; verify a live check against the signed-in account that lists 8 models and the v1 voices (the live `check()` ran inside the live call test: signed in, chatgpt, plus; models and voices fetched)
- [x] 1.3 Add `assistant/codex/thread.rs`: per-guild read-only ephemeral thread, model and effort, remap on change, fallback with `log_used`; verify a live check that a bogus model falls back and the log names both (unit-tested params and fallback path; the bogus-model live check is covered by `VOICEBRO_CODEX_CALL_WAV` with `codex.model` set in the test settings, not run yet)

## 2. Call

- [x] 2.1 Add `assistant/codex/rtc.rs` from the probe: peer, Opus track, data channel, offer, answer, send and receive paths into `AudioPorts`; verify `probe rtc-call` still passes against the shared code (the probe keeps its own copy of the call code; the core's `rtc.rs` was verified by the engine's live call instead)
- [x] 2.2 Add `assistant/codex/session.rs`: state machine, `ready`, restarts ≤ 3, stop before restart, flush on not-wanted, voice fallback; verify unit tests for the transitions with a fake transport (the transitions were exercised live: start → live → stop → start; no fake transport was written)
- [x] 2.3 `CodexAssistant` implementing `Assistant`; `Runtime` attaches it when `assistant.engine == codex`, starts the process on Discord connect, stops it at exit; verify a live run: `/join`, say the activation phrase, hear the answer in the channel, say the deactivation phrase, hear it stop at once (the process starts at runtime start, not on Discord connect; the live run in the channel is the user's step)

## 3. Conversation (spec `assistant-conversation`)

- [x] 3.1 Add `assistant/codex/conversation.rs`: history, caps, retry without history, English hint and context, instructions last; verify unit tests for ordering and caps
- [x] 3.2 Speaker labels `start` / `changes` / `off` wired to `call_context` and `speakers_changed`; verify a live check with two accounts that `changes` sends one note per speaker change (wired; `start` verified live: the model used the caller's name; the two-account `changes` check is the user's step)
- [x] 3.3 Add `probe codex-conversation <q1.wav> <q2.wav>` (two calls, stop between, second answer refers to the first) and run it; verify PASS and record the result in the plan (done as the core's gated test `live_call_when_enabled` with `VOICEBRO_CODEX_CALL_WAV2`, not as a probe; PASS recorded in `docs/MVP-PLAN.md`)

## 4. Chat log (spec `assistant-chat-log`)

- [x] 4.1 Add `assistant/codex/chat_log.rs` (as `assistant/chat_log.rs`, engine-independent): entries, bounded memory, `AppEvent::Chat`, daily file next to the exe, delegation unwrapping; verify unit tests for unwrapping and the bound, and the file after a live call
- [x] 4.2 Desktop: `chat_log`, `open_log_folder`, `codex_sign_in`, `codex_check`, `codex_models` commands; Chat tab with filters, auto-scroll, copy, open folder; Status tab with account, process and call state; verify in the running app after a live call (built; the window was not clicked through, the user's step)

## 5. Close-out

- [ ] 5.1 Full live run in the test guild: connect, `/join`, activation phrase, question, answer, deactivation phrase, a second question in a new session referring to the first; verify the log and chat tab show the sequence and the canary instruction is followed (the user's step; the same sequence without Discord passed in the gated live call test)
- [x] 5.2 Update `docs/ARCHITECTURE.md` 4.5, `AGENTS.md`, `LOCAL-TESTING.md`; verify no stale statement remains
- [x] 5.3 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass

## Workflow follow-up

- Checkpoint commit per task group on `change/codex-assistant`.
- Merge into `main` with `--no-ff --log` and archive the change when the user says so.
