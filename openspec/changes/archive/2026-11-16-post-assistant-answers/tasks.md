# Tasks

Decisions the request left open are in design.md, marked **[decided alone]** (D1, D4, D9).
Questions that come up while building are decided the same way and added to design.md under
the decision they touch, so the user can review them all at the end.

## 1. Branch and settings fields

- [x] 1.1 Create `change/post-assistant-answers` from `main` and check that `git status` shows that branch with this change's folder untracked; any other changed file is not this change's and stays out of its commit
- [x] 1.2 Add the `PostAnswers` enum (`off`, `voice_channel`, `command_channel`, `channel`; serde `snake_case`; default `off`), `assistant.post_answers` and `assistant.answers_channel: String` (default empty) to `AssistantSettings` in `crates/voicebro-core/src/settings/mod.rs`, with doc comments, and their checks in `Settings::validate`. Verify with new tests for every scenario of the `app-settings` delta: an older file loads the defaults and a save writes them; `thread` is rejected, listing the four values; `channel` without an id and an id of `#general` are rejected, naming `assistant.answers_channel`; an id is kept while `off`. Run `cargo test -p voicebro-core settings::`
- [x] 1.3 Add a runtime test that a save changing only these two fields restarts nothing: a fake gate stays registered, nothing rebuilds the gate, reloads the wake engine or reconfigures the assistant's call. Update the settings block in `docs/ARCHITECTURE.md` 4.6 with both fields

## 2. The answer path in the core

- [x] 2.1 Add to `crates/voicebro-core/src/assistant/mod.rs`:
  - `AgentAnswer { guild, task: Option<String>, text }`;
  - `AnswerSink`, a bounded tokio mpsc of capacity 16 whose `send` uses `try_send`, so a full channel drops the answer with a warning and a counter;
  - `AnswerSink::none()` for tests.

  Add `answers` to `AudioPorts`, fill it in `GateHost::on_join`, and pass `AnswerSink::none()` in every test that builds `AudioPorts`. Verify `cargo test -p voicebro-core` passes
- [x] 2.2 Add to `crates/voicebro-core/src/runtime.rs` (D3, D4, D8):
  - the `AnswerPoster` trait;
  - the destination resolved from the current `assistant` settings for each answer;
  - the handler task spawned by `Runtime::start` from the receiver, which reports the `answers` error on a refusal and clears it on a success.

  Verify runtime tests with a fake poster: `off` makes no call; a settings change between two answers sends the second one to the new destination; a refusal sets the `answers` error with the poster's message; the next success clears it
- [x] 2.3 Document `AgentAnswer`, the sink and the handler in `docs/ARCHITECTURE.md` 4.5 and 4.7, and add the answer to the Control flow in section 5; verify that section 9's assistant row still holds (a new engine posts through `AudioPorts.answers`)

## 3. The Codex session sends the answers

- [x] 3.1 Dump the app-server schema of Codex CLI 0.167.0 with `cargo run -p probe -- codex-schema <scratch dir>`. Write into design D2 the field that carries the turn id on `turn/started`, `item/completed` and `turn/completed`, and the values of `turn.status`. Verify the note names the schema file and the fields
- [x] 3.2 In `CodexSession` (`assistant/codex/session.rs`), keep the per-turn map of D2 and send the `AgentAnswer` at `turn/completed` when the status is `completed` and the final text is not empty; clear the map at shutdown. Verify with unit tests that feed `on_notification`, on a session built as the handoff test builds one, with its own answer receiver:
  - interim message, web search, final message → one answer with the final text and the unwrapped task;
  - `interrupted` → none;
  - failed with `turn.error` → none;
  - an empty final message → none;
  - two turns → two answers in order
- [x] 3.3 Give the live call test (`live_call_when_enabled`) its own answer receiver in `AudioPorts` and print each answer. Run it with `VOICEBRO_CODEX_CALL_WAV3` (a delegated question). Verify that exactly one answer arrives per completed delegation, holding the agent's final text as the chat log shows it, and record the result in this task's note

  Note (2026-11-16): PASS in 50 s against Codex CLI 0.167.0 with
  `target\opus-compare\input\calls\q1-remember.wav` and `d2-weather.wav` (libopus, items
  handoff). The first question was not delegated and gave no answer. The weather question
  was: one task, a web search, "Codex finished the task in 18.7 s", and exactly one answer
  for posting (1 of 1 completed tasks), its task "Какая погода будет завтра в Бангкоке" and
  its text, 189 characters with a Markdown link to a forecast page, equal to the chat log's
  last agent line. The voice's retelling ended with "the link to the forecast is on the
  screen", which is what this change is for. The test now asserts the count and the text.

## 4. Posting to Discord

- [x] 4.1 Create `crates/voicebro-core/src/discord/answers.rs` with `format_answer(task, text)` (D6: the task line, tables in code blocks) and `split_for_discord(text, 2000)` (D7). Verify unit tests:
  - paragraphs are cut at blank lines;
  - one 5000-character line is cut at spaces;
  - a 2500-character fenced `rust` block is closed and reopened with its language;
  - a part of exactly 2000 characters stays whole;
  - the task line is cut at 150 characters with `…`;
  - a table is wrapped and the rest of the answer is unchanged
- [x] 4.2 Keep `command_channel` in `VoiceSession`, set by `/join` before the panel is posted (D5). Add the destination resolution over the session state for the four values: the voice channel, the join's channel, and the chosen channel's guild from the cache. It returns a channel or the reason not to post. Verify unit tests for every row of D4, including a channel of another server and an answer after the leave
- [x] 4.3 Implement `AnswerPoster` for `DiscordRuntime`: send the parts in order with `allowed_mentions` allowing nothing, stop at the first refusal with Discord's message and the channel's name, and log as D12 says. Verify `cargo test -p voicebro-core discord::` passes and `cargo clippy --all-targets` reports no errors
- [x] 4.4 Add `DiscordRuntime::text_channels()` (D9): text and announcement channels per cached guild in Discord's order. `can_post` comes from a pure function over the guild's roles, the channel's overwrites and the bot's member, or is `null`. Verify unit tests of that function:
  - an overwrite denying Send Messages → `false`;
  - a role allowing all three permissions → `true`;
  - no member → `null`
- [x] 4.5 Document in `docs/ARCHITECTURE.md`:
  - 4.2: answers to a text chat, the destinations, the formatting and splitting, no pings, refusals, the channel list;
  - section 7: the `answers` error source and the log lines.

  Verify the text matches D4 to D9

## 5. The live probe

- [x] 5.1 Add `probe discord-post [--file <answer.md>]` (D11) to `apps/probe` and list it in `probe list`. Run it against the test channel from `LOCAL-TESTING.md`. Verify:
  - four messages, each at most 2000 characters;
  - balanced fences;
  - `mention_everyone` is false and `mentions` is empty;
  - a preview under the link.

  Record the message links in this task's note

  Note (2026-11-16): the built-in sample is 5 537 characters (5 763 formatted), not 4 500:
  four messages need more than 2 000 characters after the code block. PASS against the
  `General` voice channel's chat of the test guild: 1 417, 1 949, 1 927 and 477
  characters, code blocks closed in each, `mention_everyone` false, no user or role
  mentioned, a preview under the link after 1.9 s (Wikipedia's Bangkok page with its
  photo). Messages: `…/1091401169441656933/1091401169890443327/1571740798150770779`,
  `…/1571740805125775432`, `…/1571740806681862178`, `…/1571740809315876957`.
  Two findings on the way:
  - The first run's sample had its link inside parentheses, `(source: [name](url))`, and
    Discord gave it no preview; the same link outside parentheses got one, a bare link and
    a masked link alike. Answers are posted as the agent wrote them, so a link the agent
    puts in parentheses shows without a preview (design, Risks).
  - Discord sends the preview as an embed of kind `components` (a text block and a media
    gallery with the picture), which serenity 0.12's `Embed` does not model. The probe
    names the kind; the picture was checked in the raw message through the REST API.
- [x] 5.2 Add the probe to the commands block of `AGENTS.md`; verify the documented command runs as written

## 6. The window

- [x] 6.1 Add the Tauri command `discord_text_channels` in `apps/desktop/src`, calling `text_channels()`; verify `cargo build -p voicebro-desktop`
- [x] 6.2 Add the "Post answers to Discord" select and the channel select to the Codex card in `apps/desktop/ui/index.html` and `ui/main.js`, as D10 says:
  - the save rules;
  - the help line;
  - the "can't post" mark;
  - the not-connected note;
  - the reload when the Config tab opens and on connection changes;
  - the form state kept apart from the status.

  Add the command and its cases (connected with one channel marked can't post, disconnected) to `ui-dev/mock.js`. Verify by hand in the mock that both selects show and save
- [x] 6.3 Extend `apps/desktop/ui-dev/check-config-tab.mjs` with the `settings-ui` delta scenarios:
  - first start: Off and no channel list;
  - the voice channel's chat is saved without a button and shown after a reload;
  - picking a channel saves both fields;
  - A chosen channel with nothing stored saves nothing and asks to pick a channel;
  - the can't-post mark;
  - disconnected: the stored id and the note;
  - the field list of "Every setting is editable".

  Verify that `node apps/desktop/ui-dev/check-config-tab.mjs` and `node apps/desktop/ui-dev/check-wake-dialog.mjs` both pass
- [x] 6.4 Update the window section of `docs/ARCHITECTURE.md` 4.8 with the Codex card's new controls and the command

## 7. User guide and acceptance

- [x] 7.1 Add "Posting the answers to Discord" to `docs/user-guide.md`, from design "Setting it up": the three destinations, the three permissions and where a channel's overrides are checked. Add the feature to `README.md` where it lists what the bot does. Verify the guide's steps match the window in the mock
- [x] 7.2 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`; verify all three pass, and put their results in the report

  Note (2026-11-16): `cargo fmt --check` clean (after `cargo fmt` reflowed the new files);
  `cargo clippy --all-targets` no errors, 6 warnings, all in files this change does not
  touch (`discord/receive.rs`, `secrets.rs`, `audio/output.rs`, the probe's
  `credential.rs`, `opus.rs`, `rtc.rs`); `cargo test` 298 core, 15 desktop and 2 probe
  tests pass. The window checks: `check-config-tab.mjs` 30 of 30 (with the chosen-channel
  rule removed on purpose it failed on "A chosen channel with nothing stored…" with exit
  1), `check-wake-dialog.mjs` 23 of 23.
- [x] 7.3 Build a trial with `scripts\build-release.ps1 -Trial` and run it with the user in a real voice channel with two people and a game running: one delegated question each for `voice_channel`, `command_channel` and `channel`, then one for `off`. Verify:
  - each answer appears where chosen while the voice retells it, and pings no one;
  - `off` posts nothing;
  - a channel without Send Messages shows the `answers` error.

  The report lists what was not covered

  Note (2026-11-16): the trial is built (`target\release\dist-trial\`,
  `VoiceBro_1.0.1_x64-setup-trial.exe` and the portable zip, from the uncommitted branch, so
  its stamp ends in `.dirty`). The run with the user in a real voice channel is still to
  do. Done without the user: the debug app on a scratch folder listed the test server's
  `#general` with `can_post: true` from the real cache and saved A chosen channel with it.
  Closed by the user's decision of 2026-11-16 at the merge into `main` (commit, merge and
  archive asked for before the run); not verified as a task. Not tested: the bot in a real
  voice channel with two people and a game, posting through the app to each destination,
  `off` posting nothing in the app, and the `answers` error for a channel without Send
  Messages.

## Workflow follow-up

- When the user says to commit: one commit on `change/post-assistant-answers` with the code, the docs and this change archived into `openspec/changes/archive/<date>-post-assistant-answers`, its delta specs merged into `openspec/specs/`.
- When the user says to merge: `git merge --no-ff --log change/post-assistant-answers` into `main` with the message `Merge change/post-assistant-answers: <one line on the result>`, then delete the branch.
