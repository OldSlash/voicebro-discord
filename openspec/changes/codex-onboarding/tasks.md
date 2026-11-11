# Tasks

## 1. Finding Codex (spec `codex-assistant`)

- [x] 1.1 Create the branch `change/codex-onboarding` from `main` (after `release-logging` is merged); verify `git log -1` shows the `main` tip as parent (branched at `36e0304`, the `main` tip after the archive of `release-logging`)
- [x] 1.2 Split `find_codex` into the environment wrapper and `find_codex_in` with winget's links folder third in the order (design D1); verify unit tests on a temp tree: the configured path wins, PATH wins over the links folder, the links folder is found when PATH has nothing, npm's folder and `codex.cmd` come after it, and a missing configured path finds nothing (`the_finder_looks_in_winget_links_after_the_path_and_before_npm` adds the files one by one from the shim up and checks each new winner)
- [x] 1.3 Update `docs/ARCHITECTURE.md` 4.5 (the finder's order); verify the text matches `find_codex_in` (a "Finding Codex" bullet before `app_server.rs`)

## 2. Texts when Codex is missing (spec `codex-assistant`)

- [x] 2.1 Change `PrepareError::NotFound`, the status summary and the `/join` reply to the texts of design D4; verify unit tests that the error and the reply name Install Codex and the Status tab and no longer name npm, and that the signed-out reply is unchanged (the status keeps the "codex not found" start that the main spec's scenario names, so design D4 now has the two texts; `NOT_FOUND_STATUS`; `a_join_is_refused_without_codex_or_without_a_sign_in` checks both texts and the unchanged signed-out reply; 56 Codex tests pass)

## 3. The install command (spec `status-view`)

- [x] 3.1 Add `codex_install_info` (winget presence by `symlink_metadata`, package, source, sizes) and `codex_install` (the visible console of design D2, the install state, refusing a second start) to the desktop shell; verify unit tests: the command line is exactly the one in design D2, a second start while running is refused, and the state becomes `finished { code }` with the exit code of a stand-in process (`cmd /C exit 3`) used in place of winget in the test (`apps/desktop/src/codex_install.rs`, arguments passed with `raw_arg` so cmd sees them as written; the stand-in pings for a second before `exit 3` so the second start meets a running one; a `Failed` state when the console cannot start; the alias test works in a folder next to the test executable inside `target\`, no new dev-dependency; 12 desktop tests pass)
- [x] 3.2 Update `docs/ARCHITECTURE.md` 4.8 (the two commands, the install state); verify the text matches the code (the Status bullet)

## 4. The notice on the Status tab (spec `status-view`)

- [x] 4.1 Add the four states and the sign-in step to `apps/desktop/ui-dev/mock.js` (design D5); verify in the browser pane that `window.__mock` can switch Codex between missing, installing, failed and installed-not-signed-in, with and without winget (`__mock.setCodex({ installed, signedIn, accountRead, winget, nextExitCode })`, `codex_install_info`, `codex_install` running 1.5 s and ending with the chosen code, the `codex_install` event; the mock page has to be regenerated with `make.mjs` after `ui/index.html` changes)
- [x] 4.2 Build the notice in the Status tab's Codex area (design D5); verify in the mock each scenario of "Codex install offer", "Install without winget" and "Sign-in step after install": the texts, the button present or absent, the button calls `codex_install` once and is disabled while running, screenshots in Dark and Light, and no other tile moves when the notice appears or goes (missing: the package, the source, 200 MB and 370 MB, the button, no `codex_install` call before the click; running: the button disabled and "Installing Codex in the console window…"; code 0: "Codex is installed (codex-cli 0.167.0). Next: Sign in to Codex …" without the button; code 3: "winget ended with code 3 …" and the button again; no winget: the npm and the manual ways, no button; signed in: no notice; Codex found but the account not read: no notice, after Check reading no account: the sign-in step. The sign-in step needs an account read, which only Check, Sign in or `/join` makes, so it shows after an install in this window or after a Check that read no account; the app starts nothing by itself. Install info is read at start and on every `assistant` event. Every status tile kept its position; screenshots in Dark and Light)

## 5. Integration

- [x] 5.1 Release build with `VOICEBRO_DATA_DIR=target\onboarding-e2e`, started with a PATH and `APPDATA` that hide the npm Codex, through the WebView2 debugging port; verify the Status tab shows the notice with the Install Codex button, no winget process runs, and the status text is the one of design D4; then restore the environment, restart, and verify the notice is gone and the version shows (build `0.2.0+36e0304.dirty`; winget found as the alias in `%LOCALAPPDATA%\Microsoft\WindowsApps`; status executable `null`, summary "codex not found: press Install Codex on the Status tab, or set the path of codex.exe in Config", install state idle, the notice and the button shown, 0 winget processes, screenshot of the real window; after the restart with the normal environment: the npm `codex.exe`, "codex-cli 0.167.0", no notice)
- [x] 5.2 Run Install Codex for real only with the user's OK (it installs Codex 0.167 through winget into the user profile, about 200 MB); with the OK, verify the console opens, the install ends with code 0, the Status tab shows the version within 5 s and asks to sign in; without it, record the step as the user's (no OK for this install was given: it is the user's step in the follow-up below, and the real console and winget run are not tested)
- [x] 5.3 `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`; verify all pass and report their output (fmt clean after `cargo fmt`; clippy: no errors, its two "unnecessary clone to make a slice" in this change's tests fixed with `std::slice::from_ref`, the old warnings remain; tests: core 208, desktop 12, all pass. Also fixed here: a doc comment that change `release-logging` had left on the wrong function in `main.rs`)

## Workflow follow-up

- Checkpoint commits per task group on the branch, each with the user's OK, no AI
  attribution lines.
- The user, on a machine or account without Codex: Install Codex from the Status tab, Sign
  in to Codex, `/join` in a real channel with a second person and a game running, and a
  first answer.
- Merge into `main` with `--no-ff --log` when the user says so; archive the change in the
  same step; a build handed over is tagged on `main`.
