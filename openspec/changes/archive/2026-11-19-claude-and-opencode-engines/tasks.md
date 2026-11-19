# Tasks

Decisions the request left open are in design.md, marked **[decided alone]** (D4's
instruction, D7, D11). The user decided on 2026-11-19: the subscription notice covers every
claude.ai sign-in, Team and Enterprise included, and none for an API key or a cloud
provider; Install buttons for both new engines; Sign in for Claude Code only; OpenCode's web
search on, with the docs saying where queries go. Questions that come up while building are
decided the same way and added to design.md under the decision they touch.

Nothing here enforces the owner-only mode: the notice and the dialog advise.

## 1. Branch, probes and version

- [x] 1.1 Create `change/claude-and-opencode-engines` from `main`. Verify that `git status` shows that branch with only this change's folder untracked
- [x] 1.2 Add `probe claude-smoke` to `apps/probe`. It prints `claude --version`, the `claude auth status` JSON fields (no secrets), the `initialize` answer's model ids and account fields, and runs two stream-json turns with every flag of D4. Verify, and record in design.md D4, D5 and D9:
  - no session file appears under `~\.claude\projects` for `claude-workspace\` with `--no-session-persistence`;
  - `--append-system-prompt` is followed with the subscription sign-in (a canary word);
  - the `initialize` field that identifies the account;
  - that `claude auth login` is the sign-in subcommand;
  - the variables a parent Claude Code session sets for its children

  2026-11-19, Claude Code 2.1.317, Max sign-in, haiku: PASS. Both turns began with the appended canary ("BANANA: …"), the second remembered the first ("cobalt"), `init` offered only `WebFetch` and `WebSearch`, and no session file was written (only an empty `memory` folder, removed by the probe). `claude auth status` fields: `analyticsDisabled, apiProvider, authMethod, configDirectory, email, loggedIn, orgId, orgName, projectsDirectory, subscriptionType`, so the account for the owner link is `email` (D9); `initialize`'s account fields are `apiProvider, email, organization, subscriptionType`; `claude auth --help` lists `login | logout | status`. The parent session's variables (about 35, `CLAUDECODE`, `CLAUDE_CODE_*`, `ANTHROPIC_BASE_URL` …) are in D4. The probe was run from a shell with them removed
- [x] 1.3 Ask the user before installing OpenCode with `winget install --id Anomaly.OpenCode --exact --source winget` (the probes ran on the npm 1.14.23; winget has 2.3.x), and ask them to sign in again with `opencode auth login` (the stored OpenAI sign-in fails with 401). Verify `opencode --version` shows 2.3.x and `opencode models` lists the user's provider's models

  2026-11-19, with the user's OK: winget installed `opencode v2.3.2` into `%LOCALAPPDATA%\Microsoft\WinGet\Packages\Anomaly.OpenCode_…` and added that folder to the user's PATH, with no link in `WinGet\Links` (the finder now also looks there); the npm 1.14.23 was removed (`npm uninstall -g opencode-ai`), the data under `~\.local\share\opencode` stayed. `opencode auth list` shows "OpenAI OAuth stored" and the server lists the OpenAI models, but OpenAI answers every request with 401, also to plain `opencode run`, so the user has to connect OpenAI again (`opencode auth login openai`). `auth list` had started OpenCode's background service; it was stopped (`opencode service stop`)
- [x] 1.4 Add `probe opencode-smoke`: the server with a password and the D7 config and variables, a session with a title, `prompt_async` with `/event` deltas, a second prompt while busy, `abort`, session delete, a request without the password, and `opencode acp`'s cancel and busy prompt. Verify, and record in design.md D7 and D8:
  - which keys `/event` uses on 2.3.x;
  - that only `webfetch` and `websearch` reach the model;
  - that no `~\.claude` content reaches it;
  - the web search providers that work without a key, and whether `websearch.provider` keeps a headless search from asking;
  - that `share: "disabled"` and `autoupdate: false` are accepted.

  If ACP turns out better on 2.3.x, D7 is changed before group 6

  2026-11-19: 1.14's API was outdated by the time of 2.3.x; D7 and D8 were rewritten from the source of tag v2.3.2 (the V2 API: `/api/session/{id}/prompt` with `delivery`, `/api/session/{id}/interrupt`, `/api/event` with `session.execution.*` and `session.text.*`), so ACP was not pursued. The probe is written to that API (`apps/probe/src/opencode.rs`) and runs end to end against the fake (`OPENCODE_EXECUTABLE` = a `.cmd` running `fake_opencode.js`: 401 without the password, the session, three turns, the delete, every event kind printed; it fails there only on what the fake cannot do: the canary, the memory and a streaming answer to interrupt). Its live run waits for 1.3

  2026-11-19, live on 2.3.2 (D7's "Live on 2.3.2"): the password, the session, the `queue` prompt, the execution events, the failure messages, the delete and the config file work as written; the providers come from `/api/provider` and the model names from `id`, both corrected in the engine, the fake and the tests. No model answered (OpenAI 401, OpenCode's free models at their usage limit), so the text events, the interrupt, the file lockdown and the web search are still to be checked

  2026-11-19, after the user connected OpenAI again (`openai/gpt-6-luna`): PASS, four runs. The keys of `/event` are recorded in D7 (text: `assistantMessageID`, `ordinal`, `delta`; tools: `id`, `name` first, `input` at `session.tool.called`); only `websearch` was called, and a file in the working folder and a command were refused; no `~\.claude` path is in OpenCode's log, while OpenCode's own global folder `~\.config\opencode` is read (empty of instructions here; D7); `websearch.provider: exa` searched without a key and without a form (D8); the config with `share: disabled` and `update: disable` was accepted. The probe now also asks a web search and fails without one
- [x] 1.5 Raise the version to 1.3.0 in `apps/desktop/tauri.conf.json`, `apps/desktop/Cargo.toml` and `crates/voicebro-core/Cargo.toml`. Verify that the window's version stamp shows 1.3.0 in a debug run (2026-11-19: "build 1.3.0+be3aba3.dirty 2026-11-19")

## 2. Settings

- [x] 2.1 Make `assistant.engine` an enum read through `TryFrom<String>` (`codex` | `claude` | `opencode`, default `codex`), refuse `assistant.mode = realtime` with a non-Codex engine, and report `engine` as its own part in `changed_sections` (D11). Verify tests for the `app-settings` scenarios "Unknown engine", "Engine and mode in one save" and "Realtime with Claude"
- [x] 2.2 Move `codex.instructions` to `assistant.instructions` with the load-time move of D11. Verify tests: a 1.2.0 file's instruction loads into `assistant.instructions` and the next save writes no `codex.instructions`; a file with both lets the new field decide; a save changing only the instruction rebuilds no gate or wake engine
- [x] 2.3 Add the `claude` section (`executable`, `model`, `subscription_notice_shown`) and the `opencode` section (`executable`, `model`) with their checks. Verify tests: the defaults; "sonnet 5" refused naming `claude.model`; "gpt-5" refused for `opencode.model`; `openrouter/anthropic/claude-sonnet-4.5` accepted. Run `cargo test -p voicebro-core settings::`
- [x] 2.4 Update `docs/ARCHITECTURE.md` 4.6 with the engine, the moved instruction and the two sections. Verify that the section's `Settings` listing matches the struct

## 3. The shared text session (Codex only)

- [x] 3.1 Take the engine-independent parts of `codex/text_session.rs` into `assistant/text.rs` as `TextSession` over an `Arc<dyn TurnBackend>` (the engine is chosen at run time) with the backend trait of D2, and make Codex's text turns its first backend. Verify that the existing text-session tests (one question, barge-in, deactivation, one speaker at a time, steer refused) pass unchanged: `cargo test -p voicebro-core codex::`
- [x] 3.2 Run the live `whisper_text_turn_when_enabled` with Codex. Verify that the answer is spoken, the latency line prints, and the channel-context canary still names the people

  2026-11-19, after 3.1: "Who is in the voice channel right now?" (David, `target\engines-test\who.wav`) with `VOICEBRO_WHISPER_PEOPLE=Alex,Sam` was answered "Alex and Sam are in the voice channel.", spoken (`codex-who-answer.wav`), and the line read "text -> Codex first words 8967 ms; … speech end -> voice 10260 ms" (a new thread's first turn)
- [x] 3.3 Update `docs/ARCHITECTURE.md` 4.5 (the shared session, the backend trait). Verify that it names no Codex method as part of the shared session

## 4. Engine switch, names, consent and the owner link

- [x] 4.1 Add `EngineSwitch` (D1), build it in `apps/desktop/src/main.rs`, and apply the `engine` part in `Runtime::apply` through the session rebuild of the mode switch. Verify runtime tests with two stub engines: a switch while in a channel keeps the bot in the channel, detaches the old engine's session and attaches the new one's; the old engine gets its stop; a failing `prepare` of the new engine sets its connection error and sends nothing
- [x] 4.2 Use `AssistantEngine::display_name()` (through `VoiceHost::engine_name`, the chat log's engine and `AgentAnswer.engine`) in the panel, `/status`, the posted label, the chat labels and the latency line; drop the provider from `/optin`'s description; write the chat file as `logs\chat-YYYY-MM-DD.log` (D12). Verify tests: the answer formatting test with Claude's label; the chat log test writes `chat-<date>.log` and leaves a `codex-chat-*.log` untouched; the command list test reads `/optin`'s new description
- [x] 4.3 Measure the selected engine's process tree in `apps/desktop/src/memory.rs` in place of Codex's. Verify with "Memory measurement" on in a debug run that the System tab names the engine's share

  2026-11-19, debug run with Claude Code selected (a scratch data folder, `target\engines-app`): the System tab named the share "Claude Code". The same run found `codex.exe` started at app start by the read of the Codex lists, although Claude Code was the engine; that read now runs only while Codex is the engine (`main.rs`), and a second run started no engine process
- [x] 4.4 Make the consent recipient follow the engine (D10): `provider_name`, the record key `codex` | `claude` | `opencode:<provider>`. Verify tests: the Codex text is byte for byte the 1.2.0 text; the Claude text names Anthropic and not OpenAI; the OpenCode text names the model's provider and the search service; a switch from Codex to Claude drops two records with the log line; an OpenCode model change to another provider drops the record
- [x] 4.5 Generalize the owner link (D9): the engine and account from `AssistantStatus` (`signed_in_account`), `Runtime::start_link` for every engine, `check_account` comparing only the same engine and re-recording on an engine change. Verify tests for the `owner-only-access` scenarios "Claude Code not signed in", "OpenCode", "Claude signed in with another account", "Codex to Claude Code" and "To OpenCode", and that a 1.2.0 `owner-link.bin` still reads as the same link
- [x] 4.6 Update `docs/ARCHITECTURE.md` sections 1, 3, 4.7, 4.11, 4.13 and 5 for the engine switch, the names, the consent recipient and the link. Verify that the event bus and data-flow listings name no Codex-only step as shared

## 5. Claude Code engine

- [x] 5.1 Add `testdata/fake-claude/` (D14) with its `.cmd` wrappers (`fake_claude.cmd`, `fake_claude_api_key.cmd`, `fake_claude_signed_out.cmd`) and a `logging_fake()` helper that skips without Node. Verify that `--version` and `auth status` print what the real CLI prints in the probe of 1.2
- [x] 5.2 Find Claude Code (`claude-assistant` "Finding Claude Code"), cache its version per file as Codex does, and read the sign-in kind (D5) with the turns' environment. Verify tests with the fake: each kind maps as D5 says; an API key sign-in (`authMethod: api_key`, which the real CLI reports when `ANTHROPIC_API_KEY` is set, as the probe of 2026-11-19 showed) gives `ApiKey`; a configured missing path searches nowhere else
- [x] 5.3 Run Claude Code per guild as D4 says, as a `TurnBackend`, with `prepare` refusals and the lifecycle of the `claude-assistant` spec. Verify tests with the fake:
  - a question is answered, spoken and posted, with the channel facts in the first question;
  - barge-in sends the interrupt before the new line;
  - one speaker at a time queues the line and both answers play in order;
  - a deactivation during a turn interrupts it;
  - a process exit sets the error;
  - the last leave stops the process within a second;
  - the fake's log shows every lockdown flag of D4 and the appended instruction
- [x] 5.4 Read the models through `initialize` (and the account through `claude auth status`) at Check and at a join's preparation, into `capabilities()`. Verify a test with the fake that Check fills the models without a turn
- [x] 5.5 Generalize the winget helper (D13) and add the Claude Code commands: install, sign in (console window, re-read after it closes), check, models. Verify tests of the helper's command line for each package, and a debug run in which Sign in opens a console with `claude auth login`

  2026-11-19: the helper's tests pass (winget lines of the three packages; a program whose path has a space runs in the console with its arguments and exit code). In a debug run with `claude.executable` pointing at the fake (so the user's own sign-in was not touched), Sign in to Claude Code opened `cmd.exe /V:ON /C ""…\claude.cmd" auth login & … & pause & exit !rc!"`, the fake logged `auth login`, and after the console closed the app read `auth status` and `initialize` again and logged "Claude Code: Claude subscription (Max)". The first run of this check found an extra pair of quotes in that line, which the test had missed because it built the line by hand; the test now runs `program_line` itself
- [x] 5.6 Add the live `claude_text_turn_when_enabled` (a real Claude Code turn into the shared text session and a stand-in voice). Verify on this PC with the user's Claude sign-in:
  - "who is in the voice channel?" names the people sent;
  - remember 42, a new process with the history, then the number is 42;
  - asked to list the PC's files, Claude says it cannot and touches nothing;
  - the answer is spoken and the latency line prints.

  The report says which plan the sign-in was

  2026-11-19, Max sign-in, haiku: PASS in 9 s. "Who is in the voice channel right now?" → "Alex and Sam are in the voice channel." in 1.7 s, spoken by the stand-in voice read every 20 ms, and the line "[latency:1] whisper turn 1: … text -> Claude first words 1596 ms; first words -> voice 53 ms"; "remember forty two", a leave (the process stopped), a new process, then "Which number…?" → "Forty-two (42)."; "list the files" → "I can't list files on this computer. My only tools here are web fetch and web search…". The questions went in as text, as Whisper hands them over; Whisper itself was not run with Claude Code (the shared session ran it with Codex in 3.2). Seen in the answer: Claude Code's own system prompt holds its working folder's path, with the Windows user name in it; the user guide's privacy part says so (task 8.2)
- [x] 5.7 Add the Claude parts to `docs/ARCHITECTURE.md` 4.5 and to `AGENTS.md` (stack, `probe claude-smoke`, the live test). Verify that the commands listed run as written

## 6. OpenCode engine

- [x] 6.1 Add `testdata/fake-opencode/` (D14) with a `fake_opencode()` helper that skips without Node. Verify that its `/event` stream and endpoints match what `probe opencode-smoke` recorded on 2.3.x

  2026-11-19: `fake_opencode.js` serves the V2 routes and events as read from the v2.3.2 source; the helper (`fake()` in `opencode/tests.rs`) writes an `opencode.cmd` and returns `None` without Node. The comparison with the live probe waits for 1.3 and 1.4

  2026-11-19, after the live probe: the fake now answers `/api/provider` (and an empty `/api/integration`), models with `id` and a shared `modelID`, the tool events in 2.3.2's order (`input.started` with the name, `input.ended`, `called` with the input, `progress`, `success`), and an OpenAI citation mark at the end of its answer, as the probe recorded
- [x] 6.2 Find OpenCode (the native exe, npm's package path), cache its version, list its providers and models (`opencode models`). Verify tests: the npm layout resolves to the native exe, never the shim; Check fills the models as `provider/model`

  2026-11-19: the lists come from the app's own server (`/api/provider`, which replaced `/api/integration` after the live probe of 1.4, `/api/model`, `/api/model/default`), not `opencode models`, which on 2.x starts a background service of its own (D7). Tests: `opencode_is_found_as_the_native_program` (the shim is never used, winget's link before npm), `refusals_and_the_check` (Check gives `openai` and the models as `provider/model`, the server is gone after it)
- [x] 6.3 Write the OpenCode config and variables of D7 and D8 into the app's folder before each start. Verify a test of the written JSON (the agent's permission, the prompt only when the instruction is set, `share`, `autoupdate`, the search provider) and that nothing is written under `~\.config\opencode`

  2026-11-19: `the_configuration_locks_opencode_down` (2.x's format: `agents.voicebro.permissions` deny all but `webfetch` and `websearch`, `system` only when set, `share: disabled`, `update: disable`, `websearch.provider` from the key variables or `exa`), and the fake records the file and the four variables it got. The app writes only `opencode\opencode.json` in its own folder; what OpenCode itself writes under the user's profile is for the live probe (1.4)
- [x] 6.4 Run OpenCode's server as D7 says, as a `TurnBackend`, with `prepare` refusals and the lifecycle of the `opencode-assistant` spec. Verify tests with the fake:
  - the same question, barge-in, one-at-a-time and deactivation scenarios as 5.3;
  - a request without the password is refused;
  - the app's sessions are deleted at the leave;
  - the server stops after the last leave;
  - a port taken at start is retried once

  2026-11-19: `cargo test -p voicebro-core opencode` passes 16 tests, among them each scenario above, a server exit setting the error, a model change applying from the next question (`/api/session/{id}/model`), and the refusals "Install OpenCode" and no usable model ("opencode auth login")
- [x] 6.5 Add the OpenCode commands: install, check, models. Verify the helper's command line for `Anomaly.OpenCode` and a debug run in which Check shows the providers and no key

  2026-11-19: the winget line of `Anomaly.OpenCode` is tested with the other two (5.5). Debug run with `opencode.executable` set to the fake (scratch folder `target\engines-app-oc`): Check OpenCode answered "OpenCode: providers openai.", the Status tab showed "OpenCode (2.3.2-fake)", "providers: openai" and the model `openai/gpt-5.4-mini`, the Model list in Config offered the two models, the log read "the server stops (Check is done)", and no key or password showed anywhere
- [x] 6.6 Add the live `opencode_text_turn_when_enabled` and run it after the user's `opencode auth login`. Verify:
  - the canary and the 42 checks of 5.6;
  - asked to run a command, OpenCode runs nothing;
  - a question about today's weather in London either shows a web search in the chat or logs why the search failed;
  - the request OpenCode sent holds no `~\.claude` content (OpenCode's own log)
  2026-11-19: the test is in place (`opencode_text_turn_when_enabled`, skipped without `VOICEBRO_OPENCODE_LIVE=1`); its run waits for 1.3

  2026-11-19, OpenCode 2.3.2, ChatGPT sign-in, `openai/gpt-6-luna`: PASS in 22 s, twice. "Who is in the voice channel right now?" → "Alex and Sam." in 2.4 s, spoken, and "[latency:1] whisper turn 1: … text -> OpenCode first words 2440 ms; first words -> voice 4 ms"; "run the command dir" → "I can't run shell commands in this chat."; the weather in London → a chat line "Web search: weather London today November 19 2026" and an answer from it; a leave (the server stopped), a new session, then "Which number…?" → "You asked me to remember **42**.". The first run showed the chat line without its query and the answer ending with OpenAI's citation mark; both are fixed (D7), and the test now fails on either. OpenCode's log holds no `~\.claude` path (D7). Whisper itself was not run with OpenCode: the questions went in as text
- [x] 6.7 Add the OpenCode parts to `docs/ARCHITECTURE.md` 4.5 and to `AGENTS.md` (stack, `probe opencode-smoke`, the live test). Verify that the commands listed run as written

  2026-11-19: `cargo run -p probe -- opencode-smoke` ran as written against the fake; the live test's line compiles and skips without its variable. Their live runs wait for 1.3

## 7. The window

- [x] 7.1 The Engine list in the Assistant card offers the three engines; picking Claude Code or OpenCode in the realtime mode saves the engine and Whisper in one patch, disables "Realtime voice" with its text, and chains the Whisper files dialog. Extend `ui-dev/mock.js` (`?engine=`, the validation of 2.1) and add these scenarios to a new `ui-dev/check-engine-config.mjs`. Verify that `node apps/desktop/ui-dev/check-engine-config.mjs` passes
- [x] 7.2 The engine card follows the engine (`settings-ui` "The engine card follows the engine", "Claude Code card", "OpenCode card"), with the instruction bound to `assistant.instructions`. Verify the scenarios in `check-engine-config.mjs`, and that `check-config-tab.mjs` and `check-whisper-config.mjs` still pass
- [x] 7.3 The Status tab's engine area for each engine, the two install offers, Claude Code's Sign in, OpenCode's setup hint (`status-view` delta). Extend the mock (`?claude_signin=`, `?opencode_providers=`, the install and sign-in commands). Verify the scenarios in `check-engine-config.mjs`
- [x] 7.4 The subscription notice in the Assistant and Access cards and the dialog of D6. Verify every scenario of `settings-ui` "The Claude subscription notice in the window" and "The Claude subscription dialog" in `check-engine-config.mjs`; "Hear only me" without a link shows a code
- [x] 7.5 The tab "Chat" and its labels per engine (`app-window`, `assistant-chat-log`). Verify in the mock that the second tab reads "Chat" and that a Claude entry is labelled `Claude`

  2026-11-19: `check-engine-config.mjs` passes all 21 scenarios (7.1 to 7.5 and the System tab's memory name of 4.3); `check-config-tab.mjs` (49), `check-whisper-config.mjs` (34) and `check-wake-dialog.mjs` (23) pass, after their expectations of the old "Codex chat" tab name, the Codex-only mode help and the Codex card's field list were brought to this change
- [x] 7.6 Run a trial build with each engine and take screenshots through the WebView2 debugging port: the Assistant card with Claude Code and the notice, the dialog, the engine card, the Status tab for each engine. Verify that the screenshots match the specs and that no Codex-only field shows for Claude Code or OpenCode

  2026-11-19, trial build 1.3.0-trial (stamp "1.3.0+be3aba3.dirty 2026-11-19"), a scratch folder with the fakes as Claude Code (a Max subscription) and OpenCode: picking Claude Code opened "A Claude subscription is for you alone" with Not now and Hear only me; after Not now the Assistant card showed the notice with the two links and Hear only me; the engine card was titled after the engine and showed for Claude Code and OpenCode only the executable, the model, speaker names, chat history, posting and the instructions (no lists, reasoning, voice or handoff); the Status tab showed "Claude Code sign-in: Claude subscription (Max)" with Sign in and Check Claude Code, and "OpenCode providers: providers: openai" with Check OpenCode and no sign-in button; OpenCode showed no notice

## 8. Documents for users and the notices

- [x] 8.1 `README.md`: features, requirements (one of the three engines), quick start step 3 for each engine, a section "A Claude subscription is personal" with links to Anthropic's Consumer Terms and Claude Code's legal page, limitations (the realtime voice only with Codex), Built with (Claude Code proprietary, OpenCode MIT, both installed separately), and "not affiliated with Discord, OpenAI, Anthropic or the OpenCode project". Verify the `app-distribution` scenarios "Limitations before installing" and "A Claude subscription is personal" against the text
- [x] 8.2 `docs/user-guide.md`: "Set up the assistant" for each engine (install, Claude Code's sign-in, OpenCode's providers), the notice and the dialog, privacy (what each engine and OpenCode's search service receive, the files Claude Code and OpenCode keep outside the app's folder), troubleshooting (not signed in, no provider, OpenCode's 401 refresh). Verify the "OpenCode's web search" scenario and that every reply text of the join refusals is explained

  2026-11-19: the OpenCode paragraphs now follow OpenCode 2.x as read from its source (OpenCode's own models in the Model list, the refusal without a usable model, Exa without a key by default and the key variables that choose another search service). They stay unconfirmed until the live probe (1.4) and the live test (6.6)

  2026-11-19, after 1.4 and 6.6: confirmed live (Exa searched without a key). Added from the live runs: the troubleshooting rows for "Request failed: 401" (connect the provider again with `opencode auth login openai`) and "Free usage limit reached" (OpenCode's default can be a free model; choose the provider's model), the rows for every join refusal of both engines ("not installed", "did not start", "Whisper mode only", no usable model), and in the privacy part that OpenCode's own global folder (`~\.config\opencode`) still applies. The install size of OpenCode is 250 MB on disk, as measured (the dialog said 320)
- [x] 8.3 `docs/PRIVACY-POLICY-TEMPLATE.md`: the recipient per engine and the search service with OpenCode. Verify that the template names no engine the operator does not use without saying "if you use"
- [x] 8.4 `scripts/third-party-notices.ps1`: name Claude Code and OpenCode as needed but not shipped. Verify that the script runs and the notices file lists the three engines with their licenses

  2026-11-19: the script wrote the notices (503 crates) with "Codex CLI (OpenAI)" Apache-2.0, "Claude Code (Anthropic)" proprietary under Anthropic's terms and "OpenCode (the OpenCode project)" MIT under "5. Needed but not shipped". Task 8.2 stays open until OpenCode's engine and its 2.3.x web search are checked (group 6): the guide's OpenCode paragraphs say what the 1.14.23 probe and OpenCode's docs show

## 9. Checks before the commit

- [x] 9.1 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`, and the three older `check-*.mjs` scripts with the new one. Verify all pass and the report quotes what they returned

  2026-11-19, after group 6: `cargo fmt --check` clean; `cargo clippy --all-targets` no errors, 3 warnings in files this change does not touch (`secrets.rs`, `discord/receive.rs`, `audio/output.rs`) and the probe's older ones; `cargo test`: voicebro-core 516 passed, voicebro (desktop) 17, probe 2, none failed; `check-wake-dialog` 23, `check-config-tab` 49, `check-whisper-config` 34 and `check-engine-config` 21 scenarios passed. To be run again before the commit, after the live OpenCode runs

  2026-11-19, after the live OpenCode fixes: `cargo fmt --check` clean; clippy no errors, the same warnings in untouched files; `cargo test`: voicebro-core 517, desktop 17, probe 2, none failed; the four window checks 49, 34, 21 and 23 scenarios passed; `openspec validate --strict` valid
- [x] 9.2 With a release trial build idle in the Whisper mode with Claude Code selected and the bot in no channel, run `scripts\measure.ps1 -Seconds 10 -Label "idle claude"`. Verify that no `claude.exe` or `opencode.exe` of the app runs, and report the memory next to the same measurement with Codex selected

  2026-11-19, trial build 1.3.0-trial, Discord connected, no channel, the real Claude Code found by itself: "idle claude" private working set 89.9 MB (voicebro 5.7, engine 0, WebView2 84.2), CPU 0 %; "idle codex" 93.5 MB (voicebro 5.6, engine 0, WebView2 87.9), CPU 0 %. The app's only children were six WebView2 processes in both. `measure.ps1` now sums the engine as codex, claude or opencode ("engine" in place of "codex" in its last two lines)

## 10. Live check (needs a second person and a game)

- [x] 10.1 With the user and a second person in the test channel, a game running and the trial build, for Claude Code and then OpenCode in the Whisper mode. Verify:
  - questions are answered and spoken;
  - barge-in, one speaker at a time and the deactivation phrase work as with Codex;
  - with Claude Code on a subscription and "Only my Discord account", the second person's question starts nothing;
  - an engine switch while in the channel keeps the bot there and the next question goes to the new engine;
  - answer posting names the engine.

  Not run. The user closed this task on 2026-11-19 by asking to commit, merge and archive the change without the check in a real Discord channel; none of its points is verified there for Claude Code or OpenCode

  The report says what was not tested

## Workflow follow-up

- When the user says to commit: one commit on `change/claude-and-opencode-engines` with the code, the docs and this change archived into `openspec/changes/archive/<date>-claude-and-opencode-engines`, its delta specs merged into `openspec/specs/`, with no AI attribution lines.
- When the user says to merge: `git merge --no-ff --log change/claude-and-opencode-engines` into `main` with the message `Merge change/claude-and-opencode-engines: <one line on the result>`, then delete the branch; tag `v1.3.0` before a release build.
