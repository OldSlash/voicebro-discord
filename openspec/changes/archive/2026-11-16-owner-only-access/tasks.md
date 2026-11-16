# Tasks

Decisions the request left open are in design.md, marked **[decided alone]** (D2, D4, D7).
Questions that come up while building are decided the same way and added to design.md under
the decision they touch, so the user can review them all at the end.

The mode limits only speaking: no task adds an owner check to `/join`, `/leave`, `/status`
or the Mute button (the user's decision of 2026-11-16). Group 3 holds the binding to the
ChatGPT account (D4) on its own.

## 1. Branch and the setting

- [x] 1.1 Create `change/owner-only-access` from `main`. Verify that `git status` shows that branch with this change's folder untracked; any other changed file is not this change's and stays out of its commit
- [x] 1.2 Add `owner_only: bool` (serde default `false`, with a doc comment) to `DiscordSettings` in `crates/voicebro-core/src/settings/mod.rs`. Verify with tests for the `app-settings` delta scenarios:
  - a 1.0.1 file loads `false` and the next save writes it;
  - `"yes"` is rejected naming `discord.owner_only` and the stored settings stay;
  - a save changing only this field rebuilds no gate, wake engine or call (as the answer-posting test does).

  Run `cargo test -p voicebro-core settings::` and add the field to the settings block in `docs/ARCHITECTURE.md` 4.6

## 2. Owner state, code and storage

- [x] 2.1 Add the `Win32_Security_Cryptography` feature to `windows-sys` in the workspace `Cargo.toml`. In the windows-sys 0.61 source, confirm:
  - `CryptProtectData`, `CryptUnprotectData`, `CRYPT_INTEGER_BLOB` and `CRYPTPROTECT_UI_FORBIDDEN` are under that feature;
  - `LocalFree` is under `Win32_Foundation`.

  Write the module paths into design D5. Verify that `cargo build -p voicebro-core` passes
- [x] 2.2 Create `crates/voicebro-core/src/owner.rs` with the code of D2 and the `OwnerState` of D1:
  - code generation from `OsRng` in Crockford base32, shown as `XXXX-XXXX`;
  - normalization of typed codes, and comparison in constant time;
  - `PendingCode`: 10 minutes, 5 wrong tries in total, Cancel, and Link again replacing it;
  - `filter` recomputed from the mode and the link.

  Verify unit tests with an injected clock:
  - the 8 characters come only from the alphabet;
  - `k7qf2m9x` and `K7QF 2M9X` match `K7QF-2M9X`, and `O` is read as `0`;
  - the code ends at 10 minutes;
  - the fifth wrong code ends it and the right code afterwards links nothing;
  - a used code sent again changes nothing;
  - Link again replaces the code;
  - `filter` is `0` with the mode off, `u64::MAX` with the mode on and no link, and the owner's id with a link
- [x] 2.3 Add the link file of D5 to `owner.rs`:
  - DPAPI with the entropy constant;
  - `owner-link.bin.tmp`, then a rename;
  - a read at start that turns any failure into no link with a reason and one log line;
  - Unlink deletes the file;
  - an in-memory store off Windows.

  Verify Windows tests in a temporary folder:
  - write then read gives the same link;
  - one changed byte reads as unreadable;
  - a JSON with `"version": 2` reads as unreadable;
  - Unlink leaves no file.

  Run `cargo test -p voicebro-core owner::`
- [x] 2.4 Wire `OwnerState` into `runtime.rs`:
  - `start_link`: Check's account read, then a code; not signed in gives an error and no code;
  - `cancel_link`, `unlink`, and `complete_link` for the adapter;
  - `Status.owner` and `AppEvent::Owner` (D7);
  - the log lines of D2 and D4, without the code;
  - `filter` recomputed on every settings save.

  Verify runtime tests with a fake assistant:
  - not signed in → no code;
  - a completed code → linked and the file written;
  - Unlink → the file gone and the status "not linked";
  - turning the mode on → `filter` set;
  - with a capturing log, no line holds the code or the typed value.

  Search the code for logging of `Status` or `AppEvent` values and record the result in this task's note

  Note (2026-11-16): searched `crates/` and `apps/` for `{:?}`, `{x:?}` and `dbg!` of statuses and events. Only tests print them (`owner.rs`, `runtime.rs`, `wake/mod.rs`, the Codex live test, a desktop icacls test); the desktop shell hands events to the webview with `emit`, not to the log. `PendingCode`'s `Debug` prints `<hidden>` anyway. The runtime tests check every event's `Debug` form and every log line for the code and the typed value
- [x] 2.5 Add a section "4.11 Owner link (`owner.rs`)" to `docs/ARCHITECTURE.md`, with the state, the code, the file and the filter values, and add `AppEvent::Owner` to the event bus in section 3. Verify that the text matches D1, D2, D5 and D7

## 3. Binding to the assistant account (D4)

- [x] 3.1 Add the account fingerprint (SHA-256 of `"<account_type>:<email trimmed, lower-cased>"`, and `"apiKey:"` without an email) to `owner.rs`, and record it in the code at Link. Verify unit tests:
  - case and spaces in the email give the same fingerprint;
  - an API-key account gives the `apiKey:` one;
  - two emails give two fingerprints
- [x] 3.2 Compare the fingerprint of every signed-in `AssistantStatus` the runtime receives with the link's: a different one removes the link with the reason "the Codex account changed", and a signed-out status keeps it. Verify runtime tests that feed statuses:
  - the same account keeps the link;
  - another account removes it with the reason in `Status.owner` and sets `filter` to `u64::MAX` while the mode is on;
  - signed out keeps it
- [x] 3.3 Make the same comparison in `GateHost::prepare_join` right after `Assistant::prepare` succeeds: a mismatch removes the link and the join goes on. Verify a runtime test with a fake assistant signed in as another account:
  - `prepare_join` returns `Ok`;
  - the link is gone and `filter` is `u64::MAX` before the call returns.

  Document the account check in `docs/ARCHITECTURE.md` 4.11

## 4. Speaker accounts and the filter

- [x] 4.1 In `discord/speakers.rs` and `discord/receive.rs`, keep `SpeakerEntry { user, bot, name }` per source (D6):
  - `member_info` returns the name and the bot flag;
  - the speaking handler records the account for every event, bots included;
  - the fallback runs only for a source with no entry and records the only person's account;
  - `channel_people` and `VoiceSession.participants` carry `(UserId, name)`, and the gate still gets names.

  Verify unit tests for the `voice-channel-session` delta:
  - a bot's entry never takes the only person's name;
  - an unannounced source with one person takes that person's name and account;
  - a rejoin keeps names and accounts.

  The existing speaker tests still pass
- [x] 4.2 Add the filter of D6 to the `VoiceTick` branch, as a pure function that picks the passing speakers from the filter value and the directory and is called before the own decode and the mix. Also add:
  - `Shared.owner`;
  - `VoiceCounters.ignored_frames`;
  - the once-per-source log line.

  Verify unit tests of that function and of the mix:
  - `0` passes everyone;
  - `u64::MAX` passes nobody;
  - an owner id passes only that account;
  - an unknown source is dropped;
  - the mix of the owner and another speaker equals the owner's frame alone.

  Run `cargo test -p voicebro-core discord::`
- [x] 4.3 Update `docs/ARCHITECTURE.md` 4.2 (the speaker directory, the filter, the counter) and section 5 "Audio in". Verify that the text matches D6

## 5. `/link` and `/status`

- [x] 5.1 Register `/link` with the option `code` (8 to 12 characters) and `contexts([Guild, BotDm])` globally, and in the configured guild as the other commands are (D3). Verify a unit test that serializes `command_definitions()`:
  - four commands;
  - `/link`'s contexts and option;
  - `/join`, `/leave` and `/status` unchanged from today
- [x] 5.2 Add `VoiceHost::complete_link` and handle `/link` with an ephemeral reply per outcome: linked, wrong code with the tries left, expired, no code waiting, ended after 5 tries. The reply text comes from a pure function. Verify a unit test for each outcome's text, and a runtime test that `complete_link` with the waiting code links the sender's id
- [x] 5.3 Add the `/status` line of D3 while `filter` is not `0`, built by the existing status text code. Verify a unit test of the text with the mode on and off. Read `handle_join`, `handle_leave` and `handle_component` once more and record in this task's note that none of them reads the owner state

  Note (2026-11-16): the status text moved into `status_text` with a test for both cases. `handle_join`, `handle_leave` and `handle_component` were read again: none of them calls `owner_filter` or `complete_link`, so the commands are as before
- [x] 5.4 Update `docs/ARCHITECTURE.md` 4.2 (the commands, and that the mode limits none of them). Verify that the command list matches the `discord-connection` delta

## 6. The window

- [x] 6.1 Add the Tauri commands `owner_link_start`, `owner_link_cancel` and `owner_unlink` in `apps/desktop/src`, and forward `AppEvent::Owner` as the other events are forwarded. Verify that `cargo build -p voicebro-desktop` passes
- [x] 6.2 Add the Access card to `apps/desktop/ui/index.html` and `ui/main.js` (D7):
  - the switch, saved on change;
  - the link state with the reason;
  - Link, Cancel and Unlink;
  - the code with `/link <code>` and the countdown;
  - the note when the switch is on without a link;
  - the help lines;
  - the collapse id `access`;
  - the link part redrawn from the status, the switch kept as form state.

  Add `?owner=linked`, `completeLink`, `expireCode` and `wrongCode` to `ui-dev/mock.js`. Verify by hand in the mock that the card shows each state
- [x] 6.3 Extend `apps/desktop/ui-dev/check-config-tab.mjs` with the `settings-ui` delta scenarios:
  - the card order and the field list;
  - first start: off, not linked, Link;
  - linked: name and Unlink;
  - the switch on without a link and its note;
  - the code shown with its countdown;
  - the link completed within a second;
  - Cancel;
  - a collapse that keeps the code;
  - the switch at least 44 × 24 px.

  Verify that `node apps/desktop/ui-dev/check-config-tab.mjs` and `node apps/desktop/ui-dev/check-wake-dialog.mjs` both pass
- [x] 6.4 Update the window section of `docs/ARCHITECTURE.md` 4.8 with the Access card and the three commands, and the mock parameters in `AGENTS.md`. Verify that the documented mock parameters work in the mock

## 7. Install, user guide and acceptance

- [x] 7.1 Add `owner-link.bin` and `owner-link.bin.tmp` to the data deletion in `apps/desktop/installer-hooks.nsh`, and update `docs/ARCHITECTURE.md` 4.10. Then build a trial with `scripts\build-release.ps1 -Trial`, install it into a scratch folder and link an account. Verify:
  - an update install over it keeps the link;
  - an uninstall without the box keeps `owner-link.bin`;
  - an uninstall with the box removes the folder

  Note (2026-11-16): trial `VoiceBro_1.0.1_x64-setup-trial.exe`, installed silently into `target\install-test\VoiceBro`. Linking needs a person in Discord, so the link was a placeholder `owner-link.bin` next to a `settings.json`; the installer only looks at the file's name.
  - The update install (`/S` over it) kept the file byte for byte.
  - The silent uninstall removed the program and kept `owner-link.bin` and `settings.json`.
  - After a reinstall, the uninstaller's window with "Delete the application data" checked (driven by window handle: NSIS shows its box and buttons as panes to UI Automation) removed the whole folder, and with it the real `VoiceBro/discord_token`.
  - `probe seed-secrets` stored the token again ("readable"), and no VoiceBro uninstall entry was left under HKCU;
  - the debug app then connected with it ("Discord: connected", 15:02:43) and registered the four commands again
- [x] 7.2 Add "Only my Discord account" to `docs/user-guide.md`:
  - why it exists (only the voice is limited; the commands and the answers stay open);
  - Link and `/link`, in a server or a DM;
  - the 10 minutes and the 5 tries;
  - when a link ends;
  - that with the switch on and no link the assistant hears nobody;
  - that the global `/link` can take up to an hour to show in DMs.

  Add the feature to `README.md` where it lists what the bot does, and `owner-link.bin` to the app's files in `AGENTS.md`. Verify that the guide's steps match the window in the mock
- [x] 7.3 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, both window checks and `openspec validate owner-only-access --strict`. Verify that all pass, and put their results in the report

  Note (2026-11-16): `cargo fmt --check` clean (after `cargo fmt` reflowed `handler.rs` and the new tests). `cargo clippy --all-targets`: no errors; 6 warnings, none in this change's new code: `chunks_exact` in `secrets.rs`, `audio/output.rs` and the probe's `credential.rs`, `opus.rs`, `rtc.rs`, and the `let…else` of the speaking handler in `discord/receive.rs`, which this change keeps as it was. `cargo test`: core 325 (after the `set_user` fix below), desktop 15, probe 2, all pass (the DPAPI tests ran against the real Windows API). `check-config-tab.mjs` 39 of 39 (9 new Access scenarios), `check-wake-dialog.mjs` 23 of 23. `openspec validate owner-only-access --strict`: valid
- [x] 7.4 Build a trial with `scripts\build-release.ps1 -Trial` and run it with the user in a real voice channel, with two people and a game running, the mode on and the owner linked. Verify:
  - the other person can run `/join`, and the bot joins;
  - the other person's activation phrase starts nothing;
  - the owner's activation phrase starts a session, and when the other person talks over the owner, only the owner reaches the assistant;
  - the other person's `/leave` and Mute work as before;
  - `/status` from the other person has the owner-only line;
  - Unlink during a session stops the owner's voice from the next frame;
  - `/link` works in a DM to the bot.

  If the user has a second ChatGPT account, also check that signing Codex into it removes the link at the next `/join` while the bot still joins. The report lists what was not covered

  Note (2026-11-16), done without the user on the real app (debug build, scratch folder `target\owner-test`, DevTools on 9226):
  - Discord accepted the registration: "registered 4 global slash commands", so `/link` with its `code` option and the `Guild`/`BotDm` contexts passes the live API;
  - Link read the real Codex account (signed in), showed a code with "10:00 left", and the status carried it;
  - the log said only "owner link: a code was issued, valid for 10 minutes" and held the code 0 times;
  - Cancel hid the code and showed "The code was cancelled.";
  - Codex stopped exactly a minute after Link ("stopping the app-server (not needed outside a voice channel)", 14:55:32 → 14:56:32), as the `codex-assistant` delta says.

  Closed by the user's decision of 2026-11-16 at the merge into `main` (merge and archive asked for before the run); not verified as a task. Not tested:
  - the bot in a real voice channel with two people and a game;
  - `/link` sent from a real account, in a server or a DM;
  - another member's activation phrase ignored, and only the owner reaching the assistant while two people talk;
  - another member's `/join`, `/leave` and Mute working as before, and their `/status` line;
  - Unlink during a session;
  - a second ChatGPT account ending the link at a join

## Workflow follow-up

- When the user says to commit: one commit on `change/owner-only-access` with the code, the docs and this change archived into `openspec/changes/archive/<date>-owner-only-access`, its delta specs merged into `openspec/specs/`.
- When the user says to merge: `git merge --no-ff --log change/owner-only-access` into `main` with the message `Merge change/owner-only-access: <one line on the result>`, then delete the branch.
