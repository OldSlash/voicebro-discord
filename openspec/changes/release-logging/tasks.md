# Tasks

## 1. Detail lines and the file filter (spec `diagnostic-logging`)

- [x] 1.1 Create the branch `change/release-logging` from `main` (after `wake-languages` is merged); verify `git log -1` shows the `main` tip as parent (branched at `76e5e62`, the archive commit of `wake-languages` on `main`)
- [x] 1.2 Add the `detail!` macro to the core and move every line listed in design Context to it (design D1); verify a unit test that the macro logs with the target `detail::<name>`, that a search of the core for `[wake]` "heard", `[latency`, `[delivery`, the `[rtc]` buffer, track and counter lines and `[chat:` finds them only in `detail!` calls, and that `cargo test -p voicebro-core` passes (`detail!` and `DETAIL_TARGET_PREFIX` in `lib.rs`; areas wake, latency, rtc, delivery, receive, chat; the speaker lines left `shared.log` for `detail!`; `a_detail_line_logs_at_info_with_its_own_target`; the search finds those strings only in `detail!` calls and in the `delivery.rs` functions that build the lines; 203 core tests pass)
- [x] 1.3 Set the global level to Warn with Info for the app's targets, drop `CHATTY_CRATES`, add the file target's filter with the `VERBOSE` flag, and set the rotation to 1,000,000 bytes and `KeepSome(1)` (design D2); verify unit tests of the filter function: an app event line is kept, a detail line is dropped unless verbose or debug, a third-party warning is dropped, a third-party error is kept, a `songbird::driver::tasks::udp_rx` error is dropped, and `cargo test -p voicebro-desktop` passes (`keep_in_file` and `VERBOSE_LOG`; `the_release_file_keeps_events_and_errors_only` also checks that `voicebrother::x` is not an app target, `the_app_logs_at_info_and_songbirds_receive_task_at_error`; 8 desktop tests pass)
- [x] 1.4 Update `docs/ARCHITECTURE.md` 1 (the "Diagnosable in the field" row) and 7 (what the release file holds, detail lines, rotation, the window unchanged); verify both match the code (also the `tauri-plugin-log` bullet of 4.8)

## 2. The two switches (specs `app-settings`, `settings-ui`, `diagnostic-logging`)

- [x] 2.1 Add `diagnostics.verbose_log` and `assistant.save_chat_history` (design D5); verify unit tests: an older file loads with both off and the next save writes them, a string value is rejected, a save changing only `diagnostics.verbose_log` reports only `diagnostics` as changed (`the_log_switches_default_to_off_and_change_only_their_own_section`; a number is rejected too; the diagnostics test now expects `verbose_log` in the written section; the `diagnostics` branch of `Runtime::apply` logs all three switches)
- [x] 2.2 Set `VERBOSE` from the settings at start and on `SettingsChanged` in the desktop shell (design D2); verify a unit test that a settings change flips the flag, and in a debug-free check (release build, task 5.1) that a detail line reaches the file within a second of turning the switch on (`apply_verbose_log` after `runtime.start()` and in the event bridge next to the memory reader; `set_verbose_log` logs only a change; `the_verbose_switch_flips_the_flag_the_file_filter_reads`; the release check is in 5.1)
- [x] 2.3 Add the "Verbose log" switch to the Diagnostics card and "Save chat history" to the Codex card with their help lines, and both fields to `apps/desktop/ui-dev/mock.js` (design D5); verify in the mock: both shown off on a file without them, each save is one `save_settings` patch with the saved mark, the help lines read as the spec says, no other field moves, in Dark and Light (both "off" at first; `{"diagnostics":{"verbose_log":true}}` and `{"assistant":{"save_chat_history":true}}`, both names marked, the feedback line "Saved: Save chat history", every control of the tab at the same position before and after; screenshots in Dark and Light)
- [x] 2.4 Update `docs/ARCHITECTURE.md` 4.6 (the two fields) and 4.8 (the Diagnostics and Codex cards); verify the text matches the window (4.6 gained the missing `diagnostics` line of the schema)

## 3. Chat history only when wanted (spec `assistant-chat-log`)

- [x] 3.1 Give `ChatLog` the `save` flag set from the settings and in `CodexAssistant::configure`; the file is appended only while it is set and the copy into the app log is a detail line (design D3); verify unit tests on a temp dir: with saving off a recorded entry creates no `codex-chat-*.log` and still reaches memory and the event bus; turned on, the next entry is appended; turned off again, nothing more is written (`nothing_is_written_while_saving_is_off`, which also checks that not even the folder is created; `the_chat_history_switch_reaches_the_chat_log`; the old daily-file test turns saving on first; `configure` only stores the flag, so a change restarts nothing; 206 core tests pass)
- [x] 3.2 Update `docs/ARCHITECTURE.md` (the chat log part of 4.5) and the chat tab's help in the window if it mentions the file; verify both say the file is written only with the switch on (the chat tab mentions no file, only its "Open log folder" button, so it needed no change)

## 4. No process for a status read (spec `codex-assistant`)

- [x] 4.1 Cache the Codex version by path, size and modification time (design D4); verify a unit test with a fake `codex.exe` that counts its runs: ten status reads run it once, replacing the file runs it once more, and a missing executable runs nothing (`executable_version` with `VersionCache`; the fake is a `codex.cmd` that appends to `runs.txt`; `a_status_read_runs_codex_version_only_for_a_new_file`)
- [x] 4.2 Update `docs/ARCHITECTURE.md` 4.5 (the version is read once per executable); verify the text matches the code (the `app_server.rs` bullet)

## 5. Integration

- [x] 5.1 Release build with `VOICEBRO_DATA_DIR=target\logging-e2e`, the Discord token seeded, `codex.executable` pointing at a fake that counts its runs, through the WebView2 debugging port: run 60 s connected and idle; verify the file starts with the build stamp, holds the connection lines, holds no third-party warning and no detail line, the fake ran once; then turn "Verbose log" on and off from the Diagnostics card and verify the file follows within a second each time; report the file's line count by target (build `0.2.0+76e5e62.dirty`, always mode, the fake a `codex.cmd`: after 65 s connected the file had 8 lines, 4 `voicebro_core::events`, 2 `voicebro_core::runtime`, 2 `voicebro`, starting with the build stamp and holding "Discord: connecting", "connected as Братан", the slash commands; no third-party line, no detail line. The fake ran once in those 65 s and not again after 10 more status reads. The switch from the Diagnostics card: "verbose log on: detail lines go to the file too" and "verbose log off" each in the file within a second of the click, and `settings.json` holds the value. No `codex-chat-*.log` was created; the logs folder held 1377 bytes after 80 s. A live detail line in the file with the switch on needs speech in a channel: not tested here, the filter is covered by the unit tests)
- [x] 5.2 `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`; verify all pass and report their output (fmt clean after `cargo fmt`; clippy: no errors; its "items after a test module" on `lib.rs` from this change was fixed by moving the test module to the end; the remaining warnings are the old ones in `secrets.rs`, `audio/output.rs`, `discord/receive.rs` and the probe; tests: core 207, desktop 9, all pass)

## Workflow follow-up

- Checkpoint commits per task group on the branch, each with the user's OK, no AI
  attribution lines.
- The user runs a session in a real channel with a second person and a game running, first
  with both switches off (the file has the session and call events, no speech, no chat
  file), then with both on (the detail lines and the day's chat file appear).
- Merge into `main` with `--no-ff --log` when the user says so; archive the change in the
  same step; a build handed over is tagged on `main`.
