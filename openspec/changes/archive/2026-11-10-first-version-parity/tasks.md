# Tasks

## 1. Codex at `/join` (spec `codex-assistant`)

- [x] 1.1 Create the branch `change/first-version-parity` from the tip of `change/audio-gain-settings`; verify `git log -1` shows the previous tip as parent
- [x] 1.2 Remove the app-server start from `Runtime::start`; add `CodexAssistant::prepare(guild)` (start if needed, read the account, open or reuse the thread, typed errors for not found, failed handshake, signed out); verify unit tests with a missing executable and a runtime test that after `start` no app-server is running
- [x] 1.3 Add the asynchronous join preparation hook to the host and await it in the `/join` handler before `songbird.join`; on error reply with the reason and do not join; verify a test with a fake host that a failing preparation leaves the bot out of the channel and the reply carries the reason
- [x] 1.4 Stop the app-server when the last guild detaches and one minute after an on-demand Sign in or Check with no guild attached; keep the conversations; verify a test with the stub process that the server stops on the last detach and the next `prepare` starts a new one with the history kept, and the gated live smoke test (`VOICEBRO_CODEX_SMOKE=1`) for a real start and stop (the stub process is a Node fake in `testdata/fake-codex`; the live test `real_codex_starts_at_join_and_stops_after_the_last_leave_when_enabled` prepared in 720 ms and stopped 552 ms after the leave)

## 2. Mute ends the session (specs `mute-control`, `assistant-session-gate`, `audio-transport`)

- [x] 2.1 Make `wants_session` depend on mute, end the session on mute (flush, session not wanted), make it wanted again on unmute in always mode, and stop sending silent frames while muted; verify gate tests: always mode mute gives `[true, false]` wanted calls and unmute `[true, false, true]`, no frame is sent while muted, wake-word mode mute ends an active session
- [x] 2.2 Add the four earcons with the exact notes and durations of design.md D4, replacing today's start and end tones, and play the mute and unmute tones on mute changes when earcons are on; verify tests: each tone's length and first note match design.md D4, a mute change queues the mute tone even though output is muted, no tone with earcons off

## 3. Mixing (spec `audio-transport`)

- [x] 3.1 Divide the summed samples by the number of speakers in `mix_into` when two or more speak; verify tests: two speakers of 30 000 give 30 000, one speaker passes unchanged, three speakers average; and a two-speaker recording still crosses the gate's speech threshold (the two-speaker case is a synthetic pair of voices at a quiet speaking level, peak 4 000, one starting half a frame later; two real people are the user's check)

## 4. The window gets the app log (spec `diagnostic-logging`)

- [x] 4.1 Check the `tauri-plugin-log` 2.10 source for a callback target and record the choice in design.md D6; verify the note exists
- [x] 4.2 Add the 500-record ring buffer target with the file's filters, emit each record to the window, add the command that returns the buffer, and make the log pane use only these records; verify by running the app with a scratch data dir in wake-word mode: the window's log and `logs\voicebro.log` hold the same "wake word ready" line, and the command returns the lines written before the window loaded (read through the WebView2 debugging port, `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`: same line in both, `log_history` returned the 8 lines of the file)

## 5. Discord (specs `discord-connection`, `settings-ui`; existing `voice-channel-session`)

- [x] 5.1 Implement the unknown-speaker fallback in the receive handler (once per source, exactly one non-bot member, logged); verify unit tests of the choice: one member gives the name, two members give none, a later real mapping wins
- [x] 5.2 Register the commands globally and in the configured guild; verify a live run with the test guild: the log shows both registrations
- [x] 5.3 Remove `discord.auto_connect` (settings, runtime, Settings tab) and connect at start whenever a token is stored; verify a settings test that an old file with `auto_connect: false` loads and the field is gone after a save, and a live run that the app connects at start with the user's old settings file copied into a scratch data dir
- [x] 5.4 Configure the serenity cache without messages and with only what names and voice channels need (field names from the serenity 0.12.5 source); verify names still resolve in a live `/status` run and record the private working set before and after with `scripts\measure.ps1` (the private working set was read from the `Working Set - Private` counter, since `measure.ps1` reports private bytes until change `memory-measurement`: 169.2 MB before, 169.5 to 169.7 MB after, no measurable change with one guild; `/status` with people in the channel is the user's step)

## 6. Docs and checks

- [x] 6.1 Update `docs/ARCHITECTURE.md` (4.2 Discord, 4.3 mute, 4.5 Codex lifecycle, 4.7 runtime start, 4.8 log forwarding) and `AGENTS.md`; verify no text still says the app-server starts with the app or that mute keeps the call
- [x] 6.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output (fmt clean; clippy no errors, the same 6 warnings in code this change does not touch; tests: core 129, desktop 2, all pass)

## Workflow follow-up

- Checkpoint commits per task group on `change/first-version-parity`, with the user's OK.
- `/join` with a signed-out account, the mute tones in the channel and two people talking at
  once are the user's checks.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
