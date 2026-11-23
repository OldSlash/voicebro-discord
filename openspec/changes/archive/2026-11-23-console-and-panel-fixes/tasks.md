# Tasks

The branch is `change/console-and-panel-fixes`. It starts from the tip of
`change/discord-panel-controls`, which this change modifies and which is not merged into
`main` yet, and so continues the local chain of 2026-11-23. The version stays as it is.
Archive this change after `discord-panel-controls`.

The console code of group 1 was written before this proposal, while the user was still
checking. Task 1.1 reviews it against the design and verifies it.

## 1. The window app without a console of its own

- [x] 1.1 In `apps/desktop/src/main.rs`, make the binary a window program in every build (design D1). At the start of `main` in a debug build, call the new `apps/desktop/src/console.rs`: the attach to the parent's console and the signal handler of D2 and D3. Add the `windows-sys` feature `Win32_System_Console` to `apps/desktop/Cargo.toml`. Verify that `cargo build -p voicebro-desktop` and `cargo clippy -p voicebro-desktop --all-targets` pass with no warning in the desktop crate
  (2026-11-23: both passed; clippy's only warnings are the two older ones in `voicebro-core`'s `receive.rs` and `secrets.rs`)
- [x] 1.2 Check the debug exe without a person, with a scratch `VOICEBRO_DATA_DIR` and no other instance of the bot running:
  - Started through the shell, as Explorer starts it: no console window, and no `conhost.exe` child of `voicebro.exe`.
  - In the Claude app's terminal tab: the build stamp and the start lines appear in the tab.
  - In a classic console window and in a Windows Terminal window (`cmd /k`): each window closed with `WM_CLOSE` while the app runs; the app is still running 10 s later and its log file still grows.
  - Ctrl+C sent to that console with `GenerateConsoleCtrlEvent`: the app is still running.
  - Started with the output redirected to a file: the file holds the build stamp.

  Stop every instance afterwards

  Open when the change was archived on 2026-11-23: the user's `voicebro-server` from task
  10.3 of `cross-platform-headless` was still running, and stopping it was refused to the
  agent. The mechanism was measured on a test program with the same calls (design D2, D3);
  the real app was not run this way. The release build is not affected: it was a window
  program before and attaches to no terminal.

  Done later on 2026-11-23, after the archive and the 1.3.1 release: the user ran this
  check and reported it done, naming no problem.
- [x] 1.3 Update `docs/ARCHITECTURE.md` (the window binary is a window program in every build; a debug build's log reaches the terminal it was started from; the signal handling and why) and `AGENTS.md` (`cargo run -p voicebro-desktop`: the log in the terminal; Ctrl+C there does not stop the app; close the window or stop the process). Verify with a search that no doc still says a debug build opens or needs a console
  (2026-11-23: the search finds only the new section and the winget and sign-in console windows, which the app opens on purpose. The PE headers: `voicebro.exe` subsystem 2, window; `voicebro-server.exe` subsystem 3, console. The test harness keeps the console subsystem (`cfg_attr(not(test), …)`), so the desktop tests' `icacls` opens no window; `cargo test -p voicebro-desktop`: 17 passed)

## 2. One row on the panel

- [x] 2.1 In `crates/voicebro-core/src/discord/panel.rs`, make `components` return one row (design D4): Mute assistant / Unmute assistant, Text answers (primary and disabled only in the text answers mode) and Leave channel. Keep the custom ids and `PanelButton::parse` for all three mode names. Update the module comments. Verify with unit tests: one row for every mode and mute state, the labels in that order, the ids, Text answers disabled and primary only in the text answers mode, no Realtime voice or Whisper mode button, and the ids of an older panel (`voicebro:mode:realtime:<guild>`, `voicebro:mode:whisper:<guild>`, `voicebro:mute:<guild>`) still parsing: `cargo test -p voicebro-core discord::`
  (2026-11-23: 54 passed, 0 failed)
- [x] 2.2 Update `probe discord-panel` (`apps/probe/src/panel.rs`) to check the one row: the labels, the ids, and Text answers marked and disabled only in the text answers mode. Run it against the test channel, and verify that it passes and deletes its messages
  (2026-11-23: PASS; the six panels were stored with one row, the labels, the ids and Text answers marked only in the text mode, and all six were deleted)
- [x] 2.3 Update `docs/user-guide.md` (the panel's buttons; how to get back from the text answers mode, in the window), `docs/ARCHITECTURE.md` (the member panel's row, the ids kept for older panels, the mode-generic press handling) and `AGENTS.md` (the panel and the `discord-panel` probe line). Verify with a search that no doc still names a Realtime voice or Whisper mode button on the panel or two rows of buttons
  (2026-11-23: the search over `AGENTS.md`, `README.md`, `LOCAL-TESTING.md`, `CONTRIBUTING.md`, `docs/user-guide.md` and `docs/ARCHITECTURE.md` finds them only where ARCHITECTURE explains that an older panel's buttons still parse; the mock's comments now name the Text answers button)

## 3. Records and checks

- [x] 3.1 Record the user's live checks of 2026-11-23 in the tasks of the changes they belong to: `owner-link-paste` 3.3 and `cross-platform-headless` 10.3 passed; `discord-panel-controls` 4.2 passed, and the user then asked for the panel's one row, made here. Verify by reading the three tasks files
- [x] 3.2 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH), the four headless UI checks (`check-wake-dialog`, `check-config-tab`, `check-whisper-config`, `check-engine-config`) and `openspec validate console-and-panel-fixes --strict`. Record what each returned in this task and in `discord-panel-controls` 4.1, whose two UI checks were still open
  (2026-11-23:
  - fmt: clean.
  - clippy: no errors; the same six older warnings, in files this change does not touch.
  - `cargo test`: 559 + 17 + 2 passed, 0 failed. A first run while the four UI checks ran in Edge at the same time failed `opencode::tests::a_question_is_answered_spoken_and_posted` on its 120 s wait for an answer. That test alone (0.8 s) and the whole suite run again alone both passed. It touches neither the panel nor the console: a load-dependent flake of the fake-OpenCode test, not looked into further.
  - The UI checks all passed: `check-wake-dialog` 24, `check-config-tab` 53, `check-whisper-config` 39 and `check-engine-config` 29 scenarios.
  - `validate --strict`: valid. Its one note says archive would refuse the `discord-panel-controls` delta until that change is archived first, which the workflow follow-up below requires.)
- [x] 3.3 Ask the user for the live check, which needs a person:
  - The panel after `/join`: one row with Mute assistant, Text answers and Leave channel; Text answers switches the mode; in the text answers mode it is marked and cannot be pressed.
  - The debug exe double-clicked in Explorer: no console window.
  - The debug exe started from the user's own terminal: the log appears there, and closing that terminal leaves the app running.

  Record the results here

  Open when the change was archived on 2026-11-23: the user asked to merge, archive and
  release 1.3.1 before this check.

  Done later on 2026-11-23, after the archive and the 1.3.1 release: the user ran this
  check and reported it done, naming no problem.
- [x] 3.4 Report what was built and what was not tested. At least list: the user's own terminal setups beyond the ones measured, and a press on an older panel's Realtime voice or Whisper mode button unless 3.3 covered it
  (2026-11-23, reported to the user: built as tasks 1.1 to 3.2 say. Not tested: the real debug app from a terminal (1.2), the user's own terminal setups, the one-row panel and Text answers pressed in Discord, and a press on an older panel's Realtime voice or Whisper mode button (3.3))

## Workflow follow-up

- When the user says so: one commit on the branch with the code, the docs and this change archived after `discord-panel-controls` (its delta specs merged into `openspec/specs/`); then the merge into `main` with `git merge --no-ff --log`, in chain order.
