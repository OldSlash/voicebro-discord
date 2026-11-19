# Tasks

Decisions the request left open are in design.md, marked **[decided alone]** (D1, D2, D4,
D5). Questions that come up while building are decided the same way and added to design.md
under the decision they touch, so the user can review them all at the end.

The options are the operator's choice: the default access mode stays `everyone`, and no
task limits `/join`, `/leave`, `/status` or Mute.

## 1. Branch and settings

- [x] 1.1 Create `change/voice-consent` from `main`. Verify that `git status` shows that branch with this change's folder, `docs/PRIVACY-POLICY-TEMPLATE.md` and `docs/TASK-CONSENT-AND-TURNS.md` untracked; any other changed file is not this change's and stays out of its commit
- [x] 1.2 Replace `owner_only` in `DiscordSettings` (`crates/voicebro-core/src/settings/mod.rs`) with `access: AccessMode` (`everyone` | `agreed` | `owner`, default `everyone`) and the deserialize-only `owner_only` of D1. Verify with tests for the `app-settings` scenarios:
  - no file gives `everyone`;
  - `"friends"` is rejected naming `discord.access` and its three values;
  - a 1.1.0 file with `owner_only: true` loads `owner` and the next save writes `"access": "owner"` and no `owner_only`;
  - a file without either field loads `everyone`;
  - a file with both lets `access` decide;
  - a save changing only `access` rebuilds no gate, wake engine or call
- [x] 1.3 Add `privacy_url: String` to `DiscordSettings`: empty or `https://` with a host, at most 512 characters, trimmed. Verify tests: the trimmed URL is stored; `http://…` is rejected naming `discord.privacy_url`; 513 characters are rejected; a save changing only it rebuilds nothing. Run `cargo test -p voicebro-core settings::`
- [x] 1.4 Raise the version to 1.2.0 in `apps/desktop/tauri.conf.json`, `apps/desktop/Cargo.toml` and `crates/voicebro-core/Cargo.toml`. Verify that the window's version stamp shows 1.2.0 in a debug run

## 2. Consent records

- [x] 2.1 Move the DPAPI calls of `owner.rs` into `crates/voicebro-core/src/protect.rs` (D2) with the entropy as a parameter. Verify that `cargo test -p voicebro-core owner::` passes unchanged and an `owner-link.bin` written by build 1.1.0 still reads as the same link
- [x] 2.2 Create `crates/voicebro-core/src/consent.rs` with `ConsentRecord`, `ConsentState` and the file of D2 (`consents.bin`, entropy `VoiceBro consents v1`, `.tmp` then rename, read once at start, in-memory off Windows). Verify Windows tests in a temporary folder:
  - add then reload gives the same records;
  - delete leaves the others;
  - one changed byte reads as no records with a reason;
  - `"version": 2` reads as unreadable;
  - a record of another `text_version` or `assistant` is deleted at load, with one log line naming the count and the reason
- [x] 2.3 Add `CONSENT_TEXT_VERSION = 1` and the versioned texts of D3. Verify a test that the version-1 text names OpenAI, the voice and the Whisper text, what is kept, `/optout` and the URL it is given, and that every version up to the current one has a text
- [x] 2.4 Wire `ConsentState` into `runtime.rs`: records at start, clearing when `assistant.engine` changes, `Status.consent` and `AppEvent::Consent` (D7), and the log lines of D5. Verify a runtime test with the stub assistant: an agree publishes one event and appears in the status; an engine change with a record of another engine clears it. Run `cargo test -p voicebro-core consent::`

## 3. One access filter

- [x] 3.1 Replace `VoiceHost::owner_filter() -> u64` with `voice_filter() -> Arc<VoiceFilter>` (D4). The runtime rebuilds it on a mode change, a link or unlink, and a record added or deleted. Verify runtime tests: `everyone` gives `Everyone`; `owner` gives the owner alone, or an empty set with no link; `agreed` gives the owner and every record; each change is seen by the next `voice_filter()` call
- [x] 3.2 Update `discord/receive.rs`: `passes` and `passing_sources` on `VoiceFilter`, `try_sole_person` for every `Only` filter, and the log reason per mode. Verify unit tests for the `speaker-consent` scenarios:
  - a source without a record is dropped and counted;
  - two speakers give only the agreeing one's frame to the mix;
  - the owner without a record passes in `agreed`;
  - an empty set passes nobody;
  - an unknown source with one agreeing person in the channel passes, with two people it is dropped

  Run the owner-only tests of `receive.rs` and `runtime.rs` unchanged in meaning

## 4. Commands and buttons

- [x] 4.1 Add `/optin`, `/optout` and `/privacy` to `command_definitions` with the descriptions of D5 and, globally, the Guild and BotDm contexts. Verify that the registration test expects seven commands in the order `join`, `leave`, `status`, `link`, `optin`, `optout`, `privacy`, with contexts only on the last four globally and none on the guild copy
- [x] 4.2 Handle `/optin` and its two buttons in `discord/handler.rs` (D5). Verify unit tests on the reply builders:
  - no URL gives the note and no buttons;
  - a current record gives its time and `/optout`;
  - otherwise the text of D3 with the URL and the ids `vb:optin:agree:1` and `vb:optin:cancel`;
  - Agree with version 1 records, Agree with version 0 asks for `/optin` again;
  - Cancel records nothing;
  - the confirmation names the mode when it is `everyone` or `owner`
- [x] 4.3 Handle `/optout` and `/privacy`. Verify unit tests: a record is deleted and the reply says earlier speech was already sent; no record gives "nothing to remove"; `/privacy` gives the URL or the note
- [x] 4.4 Give `status_text` the access mode and add the `agreed` line to it and to the `/join` reply. Verify tests: the `owner` line as before, the `agreed` line naming `/optin` and `/privacy`, no line in `everyone`

## 5. Names

- [x] 5.1 Filter the participants by the current filter before `set_participants`, and re-apply on every filter change (D6). Verify tests for the `assistant-session-gate` scenarios: in `agreed` with Alex recorded and Sam not, the call context names Alex only; in `owner` the owner only; in `everyone` both; after Sam's `/optin` the next activation names both

## 6. Window, mock and installer

- [x] 6.1 Add the Tauri command `consent_remove(id)` in `apps/desktop/src/main.rs` and pass `Status.consent` and `AppEvent::Consent` to the window. Verify that `cargo build -p voicebro-desktop` passes and a remove through the command deletes the record from the file
- [x] 6.2 Rework the Access card in `apps/desktop/ui/index.html` and `ui/main.js` (D7): the select, the privacy policy URL field with its `https://` check, the list with Remove, "Nobody yet", the unreadable note, and both warnings. Remove "Only my Discord account" from the switch styling and the switch list. Verify in the mock:
  - first start shows Everyone, an empty URL, "not linked" and "Nobody yet";
  - picking "People who agreed" with no URL saves and shows the warning;
  - `example.org/privacy` is not saved and the field names `https://`;
  - `__mock.optin("Alex")` lists Alex within a second, and Remove deletes it
- [x] 6.3 Extend `ui-dev/mock.js` with the parameters and helpers of D7 and add the Access card's scenarios to `ui-dev/check-config-tab.mjs`. Verify that `node apps/desktop/ui-dev/check-config-tab.mjs` passes with them, and that `check-wake-dialog.mjs` and `check-whisper-config.mjs` still pass
- [x] 6.4 Add `consents.bin` and `consents.bin.tmp` to the data deletion in `apps/desktop/installer-hooks.nsh`. Verify with a `-Trial` build in `target\install-test`: an install over the previous trial keeps a `consents.bin`; an uninstall with "Delete the application data" checked removes it

  2026-11-18, trial 1.2.0: a silent install, a `consents.bin` put in, a second silent install over it and a silent uninstall all kept the file, and the built `installer.nsi` includes `installer-hooks.nsh` with the two new lines. Not run: the uninstall with the box checked, which needs the uninstaller's window and deletes the real `VoiceBro/discord_token`. Closed by the user on 2026-11-19 (commit, merge and archive asked) with that part not run.

## 7. Docs

- [x] 7.1 Update `docs/ARCHITECTURE.md`:
  - 4.2: the commands and buttons;
  - 4.6: `discord.access` and `discord.privacy_url`;
  - 4.11: the filter of D4;
  - a new section for `consent.rs` with D2 and D3;
  - the files list: `consents.bin`
- [x] 7.2 Finish `docs/PRIVACY-POLICY-TEMPLATE.md` as D8 says
- [x] 7.3 Update `docs/user-guide.md` and `README.md`: the three access modes, `/optin`, `/optout`, `/privacy`, the policy URL, the template, and that global commands may take up to an hour
- [x] 7.4 Update `AGENTS.md`: the commands, the files of the data folder (`consents.bin`), the Access card, and the mock's new parameters and helpers

## 8. Checks

- [x] 8.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, the three window checks and `openspec validate voice-consent --strict`. Verify that all pass, and put their results in the report
- [x] 8.2 Build `scripts\build-release.ps1 -Trial` and run it with a scratch `VOICEBRO_DATA_DIR` and the test bot of `LOCAL-TESTING.md`. Verify alone in the test channel:
  - the seven commands appear in the guild;
  - `/privacy` without a URL gives the note;
  - `/optin` without a URL gives the note, and with one the text, the buttons and a record in the Access card;
  - `/optout` deletes it;
  - `/optin` in a direct message to the bot works

  2026-11-18, trial 1.2.0 (`target\release\voicebro.exe`, data in `target\voicebro-consent-test`): the log says "registered 7 global slash commands" and "registered 7 slash commands in guild 1091401169441656933"; the window shows 1.2.0, the mode `agreed`, the warning that `/optin` is refused and "Nobody yet"; a privacy policy URL typed in the window is saved to `settings.json` and clears the warning. Not run: `/privacy`, `/optin`, the buttons, `/optout` and the direct message, which need a person's Discord account to send them. Closed by the user on 2026-11-19 (commit, merge and archive asked) with those parts not run.

## 9. Live check (needs a second person and a game)

- [x] 9.1 With the user and a second person in the test channel, a game running, the trial build, the mode `agreed` and a policy URL set. Verify:
  - the second person, not recorded, says the activation phrase and no session starts;
  - after their `/optin` and "I agree" their phrase starts a session;
  - while the user talks over them in a session, only the recorded voices reach the assistant;
  - the call context names only the recorded people;
  - `/optout` mid-session drops their voice at once;
  - Remove in the window does the same and sends them nothing;
  - `/status` and `/join` show the `agreed` line;
  - in `everyone` both are heard as before, in `owner` only the linked owner.

  The report says what was not tested

  Closed by the user on 2026-11-19 (commit, merge and archive asked) without the live check: none of it was run in a real channel with two people and a game.

## 10. Commit and merge

- When the user says to commit: one commit on `change/voice-consent` with the code, the docs (the template among them) and this change archived into `openspec/changes/archive/<date>-voice-consent`, its delta specs merged into `openspec/specs/`.
- When the user says to merge: `git merge --no-ff --log change/voice-consent` into `main` with the message `Merge change/voice-consent: <one line on the result>`, then delete the branch.
