# Design

## Context

See `proposal.md` for why. The current state that shapes the approach:

- **The engine is fixed at process start.** `apps/desktop/src/main.rs` builds
  `CodexAssistant` when `assistant.engine == "codex"` and `StubAssistant` otherwise;
  `Runtime.assistant` and `GateHost.assistant` hold one `Arc<dyn Assistant>` for the whole
  run. `Settings::validate` accepts only `"codex"`. Every Codex Tauri command goes through
  the `codex()` helper.
- **The Whisper mode's session mixes two kinds of code.** `assistant/codex/text_session.rs`
  holds what any text engine needs (speaker prefix, `SentenceSplitter` into the
  `SpeechPlayer`, barge-in, the one-speaker-at-a-time queue, history and channel context,
  `AgentAnswer` posting, chat entries, the Whisper latency line) next to what only Codex
  needs (`turn/start`, `turn/steer`, `turn/interrupt`, `additionalContext`, the thread).
- **The mode switch already rebuilds sessions in place.** `Runtime::apply` treats `mode` as
  its own changed section and calls `GateHost::switch_mode`, which detaches the guild's
  session, builds a new gate and attaches again on the same output, with the bot in the
  channel.
- **The owner link is tied to the assistant account.** `Runtime::start_link` needs
  `signed_in_account(assistant.status())` (a `type:email` fingerprint from Codex);
  `owner.rs` stores `assistant: "codex"` and the fingerprint, and `check_account` ends the
  link when the fingerprint differs. An engine with no account cannot link today.
- **Codex is named in strings**: `consent::provider_name`, `/optin`'s description
  (`discord/handler.rs`), the panel's Engine field (`discord/panel.rs`), the posted label
  `-# Codex:` (`discord/answers.rs`), the latency line (`latency.rs`), the chat file
  `codex-chat-*.log` (`chat_log.rs`), the "Codex chat" tab, `memory.rs`'s Codex share.
- **The core has no HTTP client of its own**; downloads go through `curl.exe`. The lock file
  already holds `reqwest` 0.12.28 (through serenity) and 0.13.5.
- **What the live probes of 2026-11-19 found** (scratch notes, not in the repository):
  - Claude Code 2.1.317 (winget offers 2.1.318), native `claude.exe` in
    `%USERPROFILE%\.local\bin`. `claude auth status` prints JSON without a model call
    (`authMethod` `claude.ai` with `subscriptionType`, or `api_key`); an
    `ANTHROPIC_API_KEY` in the environment wins over the subscription. One process with
    `--input-format stream-json --output-format stream-json --verbose
    --include-partial-messages` takes many turns; text deltas arrive as `stream_event`
    `content_block_delta`; a `control_request` `interrupt` stops a turn in about 5 ms; a
    user line sent during a turn is queued as the next turn, never merged. `--tools
    "WebSearch,WebFetch"`, `--permission-mode dontAsk`, `--strict-mcp-config` with an empty
    MCP config and `--setting-sources ""` were verified (the personal CLAUDE.md was not
    loaded). The `initialize` control request lists the models and the account without a
    model call. Cold start to the first delta 2.0–2.7 s; a warm turn on haiku 0.7 s.
    `--bare` accepts only an API key. The system prompt is fixed for a session.
  - OpenCode 1.14.23 from npm (winget offers `Anomaly.OpenCode` 2.3.2). The stored OpenAI
    sign-in on the test PC fails its refresh with 401, so only a local fake provider was
    used. `opencode serve` gives `message.part.delta` over SSE `/event`, `prompt_async`,
    `abort` (idle within 1 ms) and a queue for a prompt sent while busy, taken up at the next
    step. A config passed by `OPENCODE_CONFIG` with one agent whose permission is
    `"*": "deny"` plus `webfetch` and `websearch` reached the model with only those two
    tools. OpenCode loads `~\.claude\CLAUDE.md` and `~\.claude\skills` unless
    `OPENCODE_DISABLE_CLAUDE_CODE=1` and `OPENCODE_DISABLE_EXTERNAL_SKILLS=1`. The server
    warns that it is unsecured without `OPENCODE_SERVER_PASSWORD`. A session created
    without a title costs an extra model call for one. OpenCode 2.x documents five web
    search providers (Exa, Firecrawl, Parallel, Tavily, TinyFish), chosen by
    `websearch.provider` or by a prompt at the first search, each with its own key.

## Goals / Non-Goals

**Goals:**

- One text-turn session for all engines, so barge-in, the queue, the voice, history,
  posting and latency behave the same and are tested once.
- An engine change without a restart and without leaving the channel, through the same
  path as a mode change.
- Each engine locked down to web access only, with none of the user's personal agent
  configuration loaded, and no credential ever passing through the app.
- The Claude subscription notice shown exactly when it applies, and never enforcing.

**Non-Goals:**

- A shared abstraction over the realtime call: it stays Codex-only.
- Supporting OpenCode 1.x. The app targets the version winget installs (2.3.x at the time of
  writing); an npm install of another version is used as found, without promises.
- Changing the Codex realtime path, its lists file or its install flow beyond the shared
  installer helper.

## Decisions

### D1. An engine switch behind the `Assistant` trait

`EngineSwitch` (new, `assistant/switch.rs`) implements `Assistant` and holds the three
engines: Codex, Claude Code and OpenCode, each built at start and starting nothing until
used. It forwards `prepare`, `attach`, `detach`, `status`, `capabilities`, `configure`,
`stop` and `use_tts` to the selected one. `main.rs` builds it in place of the engine chosen
at start; the `codex()` helper keeps working because the Codex object always exists, and
`claude()` and `opencode()` helpers are added for their commands.

`changed_sections` reports `engine` as a part of its own (as `mode` is). `Runtime::apply`
for `engine`: `EngineSwitch::select(new)`, then `GateHost::switch_mode`'s rebuild (renamed
`rebuild_sessions`, called for both parts), then the old engine's "last guild detached"
stop. A failed `prepare` of the new engine in a channel sets the engine's connection error
and leaves the gate without a ready session, so nothing is sent until the user fixes it.

*Alternative:* require a restart for an engine change. Rejected: the UI promises "Applies
at once", and the mode switch already does the hard part.

### D2. A shared text session over a small turn backend

`assistant/text.rs` takes everything engine-independent out of `codex/text_session.rs`:
`TextSession` over an `Arc<dyn TurnBackend>` (a trait object, since the engine is chosen
at run time). The backend trait is small:

- `start(text, context) -> Result<TurnId>`, where `context` holds the channel facts and the
  history text when the conversation is new or the session's first turn;
- `add(text) -> Added | NotRunning`, for one speaker at a time;
- `interrupt(turn)`;
- a stream of `TurnEvent`: `Delta(text)`, `Message(text)`, `Tool(label)`,
  `Completed { text }`, `Failed(reason)`, `Exited`.

Codex's backend sends `turn/start` (with `additionalContext`), `turn/steer` and
`turn/interrupt`, as today. Claude's `add` answers `NotRunning` only when no turn runs and
otherwise queues the line in Claude Code (next turn); OpenCode's `add` posts the prompt to
the busy session (OpenCode's own queue). The session keeps the existing rules: the stale
turn id, `turn_lock`, `waiting`, epochs on the player, the latency line.

*Alternative:* a separate session type per engine. Rejected: barge-in, the queue and the
voice would be written three times. The fake-codex text-session tests guard the refactor:
they must pass unchanged before Claude or OpenCode code is added.

### D3. Context and history as text for Claude Code and OpenCode

Neither has a field like Codex's `additionalContext`. The first question of a session gets
a facts block before the question, the same facts Codex gets ("Discord voice channel.",
the people, the caller), unless speaker labels are off. A new conversation (first join,
after an app-server or process restart, or after an instruction change) gets the guild's
history text (`Conversation::history_text`, 24 KB) in the same block. The live canary of
the Codex change is repeated per engine: "who is in the voice channel?" must name the
people sent.

### D4. Claude Code runs as one stream-json process per guild

Started at `prepare` with the working folder `claude-workspace\` (empty, in the app's
folder), the window hidden, and:

```text
claude -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --tools "WebSearch,WebFetch" --permission-mode dontAsk
  --strict-mcp-config --mcp-config {"mcpServers":{}} --setting-sources ""
  --no-session-persistence [--model <claude.model>]
  [--append-system-prompt <assistant.instructions>]
```

- *The instruction* **[decided alone]** is appended to Claude Code's own system prompt, as Codex keeps its
  base instructions and adds the user's as developer instructions. Replacing the system
  prompt (`--system-prompt`) was considered and left out: it changes more than the user
  asked for and its behavior with a subscription sign-in is not probed.
  Checked live on 2026-11-19 (Claude Code 2.1.317, Max sign-in, haiku): with "Always begin
  every reply with the single word BANANA" appended, both turns of one process began with
  "BANANA:".
- *No session on disk.* `--no-session-persistence` keeps the conversation in the process
  only; continuity across joins comes from D3's history text, as with Codex's ephemeral
  threads. Checked live the same day: two turns in one process kept their context and no
  session file was written; Claude Code only created an empty
  `~\.claude\projects\<working folder>\memory` folder.
- *The environment* is the app's, so the sign-in the status shows is the one the turns use.
  When the app itself runs inside a Claude Code session (`CLAUDECODE` is set, which happens
  only in development), every variable matching `^(CLAUDE|ANTHROPIC|OTEL_)` is removed for
  the child, as the probes did: that session sets about 35 of them (its entry point, session
  id, messaging socket, `ANTHROPIC_BASE_URL` and others) and they would route the child
  through the parent. Otherwise the environment is passed unchanged, so a user's
  `ANTHROPIC_API_KEY` or cloud-provider variables decide the sign-in as they do in a
  terminal.
- *Turns:* one user line per question; `stream_event` text deltas feed `Delta`; an
  `assistant` message's text blocks feed `Message`; `tool_use` blocks feed `Tool`; a
  `result` ends the turn: `Completed` unless `is_error`. `interrupt` is a
  `control_request` with subtype `interrupt`.
- *Lists:* the `initialize` control request, sent once per process (and by Check through a
  short-lived process), gives the models without a model call. The sign-in kind and the
  account for the owner link come from `claude auth status` (D5, D9).
- *Lifecycle* as Codex's app-server: stop 0.5 s after the last detach, at an engine switch
  and at exit; the process is killed with the app.

### D5. Claude sign-in kinds

`claude auth status` prints JSON by default (about 1 s, no model call): `loggedIn`,
`authMethod`, `apiProvider`, `subscriptionType`, and for a claude.ai sign-in `email`,
`orgId` and `orgName`; with `ANTHROPIC_API_KEY` set, `authMethod` is `api_key`,
`apiKeySource` names the variable and the subscription fields are gone. It maps to
`ClaudeSignIn`:

- `authMethod` `claude.ai` → `Subscription { plan }`, for every plan, Team and Enterprise
  included (the user's decision of 2026-11-19);
- `api_key` → `ApiKey`;
- a cloud `apiProvider` (Bedrock, Vertex, Foundry) → `Cloud(name)`;
- not signed in → `None`;
- anything else, or a failed read → `Unknown`.

The notice condition is: engine Claude, sign-in `Subscription` or `Unknown` (or not read
yet), access not `owner`. It lives in one function shared by the status (for the window)
and the tests; the window only renders it.

### D6. The notice and the dialog in the window

The Engine list's change is intercepted as the wake model's "choice" flow is
(`chooseWakeModel`): when the notice condition holds for the new engine and
`claude.subscription_notice_shown` is false, the window first reads the sign-in kind
(Check), then opens the dialog. Either answer saves the flag; "Hear only me" also saves
`discord.access = owner` and, without a link, starts the link code and opens the Access
card. The engine is saved in the same patch as the mode (Whisper) when the realtime mode
was on; the Whisper files dialog follows the subscription dialog. The persistent notice in
the Assistant and Access cards is drawn from the status on every refresh.

### D7. OpenCode runs behind its own local server **[decided alone]**

*OpenCode 2.x replaced the server API.* The probe of 1.14.23 (in Context) used the V1 API;
OpenCode 2.x serves only a new `/api/...` API (`migrate-v1.mdx`: "Integrations that call
the V1 server API must migrate to the V2 API"). What follows is read from the source at tag
`v2.3.2` (commit `c0ad18b4` of github.com/anomalyco/opencode: `packages/protocol`,
`packages/server`, `packages/core`, `packages/cli`) on 2026-11-19, and must be confirmed
by the live probe of task 1.4 before group 6 is closed.

Started at the first `prepare` (one server for the app, a session per guild): the native
`opencode.exe serve --hostname 127.0.0.1 --port <free>` in `opencode-workspace\`, window
hidden, with:

- `OPENCODE_PASSWORD`: 32 random bytes as hex, made per start, kept in memory only. Every
  route, `/api/event` included, takes HTTP Basic with the user `opencode`
  (`packages/server/src/auth.ts`); without the variable the server makes its own password.
- `OPENCODE_CONFIG`: `opencode\opencode.json` in the app's folder, written before each
  start, in the V2 format (`packages/schema/src/config.ts`): `share: "disabled"`,
  `update: "disable"`, `default_agent: "voicebro"`, the web search provider (D8), and
  `agents.voicebro` with `mode: "primary"`, `system` = the user's instruction when it is
  not empty, and `permissions` = `[* * deny, webfetch * allow, websearch * allow]` (ordered
  rules, the last match wins, agent rules after the global ones), so no `shell`, `edit`,
  `read`, `skill`, `subagent`, `execute` or MCP tool is allowed.
- `OPENCODE_DISABLE_CLAUDE_CODE=1` (no `CLAUDE.md`, no `.claude` skills),
  `OPENCODE_DISABLE_PROJECT_CONFIG=1`, `OPENCODE_DISABLE_AUTOUPDATE=1`.
  `OPENCODE_DISABLE_EXTERNAL_SKILLS` no longer exists in 2.x: skills under `~\.agents` are
  still found, and the `* * deny` rule keeps the agent from using any of them.

The server is ready when `GET /api/info` answers 200 (`{version, …}`), waited for at most
20 s; its stdout line `server listening on http://127.0.0.1:<port>` is only logged. The free
port is taken by binding `127.0.0.1:0` and releasing it, passed with `--port` (without it
OpenCode starts at 4096, and on Windows a reserved port fails with EACCES instead of being
skipped); a start that does not become ready is retried once with a new port.

The calls (`packages/protocol/src/groups/session.ts`; every body refuses unknown fields):

- a guild's session: `POST /api/session {title, agent: "voicebro", model?: {providerID,
  id}, location: {directory: <opencode-workspace>}}` → `{data: {id, …}}`. The title keeps
  OpenCode from a title-generation call. A model change (`opencode.model`) applies with
  `POST /api/session/{id}/model` before the next question;
- a question: `POST /api/session/{id}/prompt {text, delivery}`, always asynchronous;
  `delivery: "queue"` starts a turn when the session is idle; `"steer"` (one speaker at a
  time, `add`) joins the running loop at its next step boundary;
- barge-in: `POST /api/session/{id}/interrupt` → `{interrupted}`; the backend then waits,
  at most 3 s, for the execution's end event before the next question goes out, so a late
  `interrupted` event is not taken for the new turn;
- the leave: `DELETE /api/session/{id}`;
- Check and the join: `GET /api/info`, `GET /api/provider` (the providers OpenCode can use
  now, by `id`; a `disabled` activation is left out; the CLI's `opencode auth list` and
  `opencode models` start OpenCode's persistent background service, so the app asks its own
  server), `GET /api/model` (the enabled models; `id` names a model, `modelID` is the
  provider's model under it) and `GET /api/model/default`.

**Live on 2.3.2** (`probe opencode-smoke`, 2026-11-19, winget's package; corrections to the
source reading above are already in it). `GET /api/integration` answered an empty list with
an OpenAI sign-in stored: it lists integrations set up in configuration files, so the
providers are read from `GET /api/provider`, which answered `opencode` (OpenCode's own,
`activation: enabled`) and `openai` (`auto`). Its entries also carry the provider's
settings and headers (with OpenAI the ChatGPT account id); the app reads only the ids. In
`GET /api/model` several entries share a `modelID` (`gpt-6.2-sol`, `-fast`, `-ultrafast`)
and `id` tells them apart, so the name is `providerID/id`, each once. With an OpenAI sign-in
stored, OpenCode's default model was still one of its own free ones
(`opencode/tidepool-alpha-free`). Seen as written above: the password (401 without it),
`POST /api/session` with the agent, the folder and the model, the `queue` prompt
(`session.inbox.enqueued`, `.delivered`), `session.execution.started` and `.failed` with
`data.error.message` ("Free usage limit reached. …" for the free models once their limit
was used up; "Request failed: 401" for an OpenAI sign-in OpenAI no longer accepts, which
plain `opencode run` gave too), and `DELETE`. Events without a session (`server.connected`,
`*.updated`) and `session.step.*` are ignored. The config file with the `voicebro` agent,
`share: disabled` and `update: disable` was accepted, and the session used that agent. The
only instructions the session got were OpenCode's `core/date` and `core/environment`
(`session.instructions.updated`): no file of the repository it ran in (OpenCode still
recognised the git folder above the working folder as the project).

After the user connected OpenAI again (`openai/gpt-6-luna`, ChatGPT sign-in), the probe
passed: every answer began with the canary of the agent's `system` ("BANANA: …"), the second
turn remembered the first, asked for `secret.txt` in its folder the model answered "I can't
access files in your working folder" and the secret never appeared, an interrupt of a
streaming answer replied `{interrupted: true}` and `session.execution.interrupted {reason:
"user"}` came 63 to 65 ms later, and the delete succeeded. Text arrives as
`session.text.started`, `session.text.delta {assistantMessageID, ordinal, delta}` and
`session.text.ended`; `session.reasoning.*`, `session.step.*` and `session.usage.updated`
come too and are ignored. A tool call arrives as `session.tool.input.started {id, name}`,
`.input.ended {id, text}` (the input as JSON text), `session.tool.called {id, input}` (no
name), `.progress {id, metadata: {provider}}` and `.success {id, content}`: the backend keeps
the name by call id and writes the chat line at `.called`. An OpenAI model sometimes ends an
answer with OpenAI's citation mark `\u{e200}cite\u{e202}turn0search0\u{e201}` (two of
three weather answers in a row); the backend drops every mark and every other character of
the private use area from the stream and the final text (`strip_citations`), so the voice
does not read it and the post does not show it. The answers use Markdown (`**7–8°C**`),
which the voice's `speakable` already removes.

OpenCode's own log (`~\.local\share\opencode\log\opencode.log`) holds no `~\.claude` path for
the probe's runs, but shows that the server watches OpenCode's global folder
(`~\.config\opencode`: its `AGENTS.md`, `skill`, `skills`), which the variables of this
design do not turn off: `OPENCODE_DISABLE_PROJECT_CONFIG` covers the project's files only.
On this PC the folder holds no `AGENTS.md` or `opencode.json`, so nothing of it reached the
model. A user's global OpenCode instructions, settings and plugins would apply to the
assistant as to any OpenCode session; skills stay unusable through the agent's permissions.
Pointing the server's `XDG_CONFIG_HOME` at the app's folder is expected to shut the global
folder out (not tried; the log's paths follow the XDG layout), but would also hide providers
the user configured in the global `opencode.json` (local models, custom endpoints). The
user's decision of 2026-11-19: leave it as it is. OpenCode's global folder is the setup of
whoever runs it, and applies to the assistant as to any OpenCode session; the user guide
says so.

The events (`GET /api/event`, server-sent `data: <json>` frames, live only, a `: heartbeat`
every 15 s): `{id, created, type, data}`. The backend reads, for the sessions it made:
`session.execution.started`; `session.text.delta {sessionID, assistantMessageID, ordinal,
delta}` → `Delta` (and kept per message to make the whole text); `session.text.ended` →
`Message` with the kept text; `session.tool.*` → a `tool` chat line; and the end of a turn:
`session.execution.succeeded` → `Completed`, `.failed {error: {message}}` → `Completed`
not ok with the message, `.interrupted {reason}` → "interrupted". A turn is the backend's
own number from a `queue` prompt to the end event; a `steer` prompt joins the running one,
and is refused (`add` → error, the session starts the next turn) when no execution runs.
The stream is opened once per server and opened again, once a second, when it ends while
the server runs.

HTTP goes through `reqwest` 0.12 with `default-features = false` and the `stream` feature,
which serenity's tree already enables (`cargo tree -e features -i reqwest@0.12.28` on
2026-11-19), so the lock file gains no crate; the event stream is read with
`Response::chunk`, JSON is written and read with `serde_json`.

*Alternative:* `opencode acp` (JSON-RPC over stdio, like the Codex app-server): no port and
no password. Not chosen: the server's queue (`delivery`) maps one to one onto the session's
add and next-turn rules, and its interrupt reports how the turn ended.

### D8. OpenCode web search

The tool is on (the user's decision of 2026-11-19). In 2.3.x OpenCode registers five
providers (`websearch.provider`: `exa`, `firecrawl`, `parallel`, `tavily`, `tinyfish`, or
`random`, which picks among all five and moves on only after an HTTP 429). With no provider
set, the first search asks in a form and fails after a minute, which a headless server
cannot answer, so the config always names one: the first provider whose key is in the
environment the app passes on (`EXA_API_KEY`, `TAVILY_API_KEY`, `FIRECRAWL_API_KEY`,
`PARALLEL_API_KEY`, `TINYFISH_API_KEY`, in that order), else `exa`, which works without a key
through Exa's hosted endpoint (`packages/core/src/websearch.ts`). The app holds no search
key. When a search fails, the answer still comes, and the chat shows the failed tool.

Live on 2.3.2 (2026-11-19): with `websearch.provider: exa` and no key, the search ran at
once (`metadata.provider: exa`), nothing waited on a form, and the answer cited Exa's page
for the weather in London.

### D9. Owner link per engine

The engine and the account come from the selected engine's `AssistantStatus`
(`runtime::signed_in_account` returns `(engine, fingerprint)`): Codex the email
fingerprint as today, Claude Code the `email` of `claude auth status` (a claude.ai
sign-in; an API key or a cloud provider has no account, like OpenCode), OpenCode `Some`
with an empty fingerprint. `owner-link.bin` keeps its
fields (`assistant`, `account`). `check_account` compares only when the link's engine is
the selected engine; otherwise it records the new engine and account in the link (a
rewrite of the file) and logs it. A link file of build 1.2.0 (`assistant: "codex"`) reads
as before.

### D10. Consent per recipient

`consent::provider_name(engine, opencode_provider)`: "OpenAI, through Codex", "Anthropic,
through Claude Code", and for OpenCode the model's provider by a short table (`openai` →
"OpenAI", `anthropic` → "Anthropic", `google` → "Google", others by their id) followed by
"through OpenCode", plus the sentence about the search service. The record's `assistant`
field becomes the recipient key: `codex`, `claude` or `opencode:<provider>`, so
`set_assistant` drops records on an engine or provider change with the existing code. The
Codex text stays word for word, so `CONSENT_TEXT_VERSION` stays 1 and no Codex record is
lost by the update.

### D11. Settings

`AssistantEngine` becomes an enum read through `TryFrom<String>` like `assistant.mode`;
`validate` refuses `realtime` with a non-Codex engine. `assistant.instructions` replaces
`codex.instructions` with a load-time move like `discord.owner_only` → `discord.access`.
The instruction stays a shared field of the engine card (the user's instruction is one
text; switching engines must not lose it). **[decided alone]**: the alternative, one
instruction per engine (`claude.instructions`, `opencode.instructions` next to
`codex.instructions`), needs no migration but makes the user copy the text at every
switch. New `claude` and `opencode` sections. A change
of the instruction or a model opens a new conversation for Claude Code (its system prompt
and model are fixed per process) and applies from the next question for OpenCode.

### D12. Names and files

`Assistant::display_name()` ("Codex", "Claude", "OpenCode") feeds the panel, `/status`, the
posted label, the chat labels and the latency line. `/optin`'s description loses the
provider. The chat file becomes `logs\chat-YYYY-MM-DD.log`. `paths.rs` adds
`claude-workspace\`, `opencode-workspace\` and `opencode\`. `memory.rs` measures the
selected engine's process tree in place of Codex's.

### D13. Installing

`codex_install.rs` becomes a small winget helper taking the package id
(`OpenAI.Codex`, `Anthropic.ClaudeCode`, `Anomaly.OpenCode`), still a console window the
user sees, accepting nothing on their behalf. Claude Code's Sign in runs
`claude auth login` (the subcommand is confirmed by the probe) in a console window, and the
window re-reads the sign-in after that console closes.

### D14. Tests

- `testdata/fake-claude/fake_claude.js`: `--version`, `auth status` (flags for subscription,
  API key, signed out), stream-json turns with deltas, a tool use, `result`, the
  `initialize` and `interrupt` control requests, a queued line, `--log <file>` of every
  argument and line.
- `testdata/fake-opencode/fake_opencode.js`: an HTTP server with the password check,
  sessions, `prompt_async`, `/event` SSE deltas, the busy queue, `abort` and delete.
- The text-session scenarios run against all three fakes: one question answered, spoken
  and posted with the channel context; barge-in; one speaker at a time; deactivation during
  a turn; the process or server exiting.
- Live, gated by variables: `claude_text_turn_when_enabled`,
  `opencode_text_turn_when_enabled`; probes `claude-smoke` and `opencode-smoke`.
- The command mock gains `?engine=claude|opencode`, `?claude_signin=subscription|api_key|
  none|unknown`, `?opencode_providers=…`; `check-engine-config.mjs` runs the dialog and
  card scenarios in headless Edge.

## Risks / Trade-offs

- [Anthropic changes the terms or how subscriptions may be used headless] → the notice and
  the docs link to the source pages instead of quoting them at length; the notice text is in
  one place; the API key path is unaffected.
- [OpenCode 2.3.x differs from the 1.14.23 that was probed] → task 1 re-probes on the winget
  version before any OpenCode code; D7's alternative is ready.
- [OpenCode cannot reach a model on the test PC (401)] → the user signs in again with
  `opencode auth login` before the live tests; until then only the fake is tested, and the
  report says so.
- [Search queries go to a third company with OpenCode] → said in the README, the guide, the
  consent text and the policy template; the user chose to keep the tool on.
- [Claude Code's cold start adds 2–3 s to the first answer] → the process starts at the
  join's preparation, not at the first question.
- [The refactor of the Codex text session breaks the Whisper mode] → the existing
  fake-codex tests and `whisper_text_turn_when_enabled` run before and after it.
- [OpenCode keeps the app's sessions after a crash] → they are deleted at each leave; a
  crash leaves them in OpenCode's own database, which the guide mentions.
- [Claude Code writes `~\.claude.json` on every run] → outside the app's control; the
  privacy part of the guide says so, as it does for `~\.codex`.
- [An interrupted Claude turn keeps its partial text in Claude's context] → the same as
  Codex's interrupted turns; no action.
- [The port picked for OpenCode is taken between release and bind] → one retry with a new
  port.

## Migration Plan

- Settings: `codex.instructions` moves to `assistant.instructions` at load; the next save
  drops the old field. An older build reading a new file has no instruction (stated in the
  proposal). Rollback: the previous release tag; the user types the instruction again.
- Consent records: kept with Codex (text unchanged); dropped on the first engine change, as
  today.
- Owner link: the 1.2.0 file reads as before.
- Chat files: new entries go to `chat-*.log`; old `codex-chat-*.log` stay.
- Version 1.3.0, tag `v1.3.0` on `main` after the merge.

## Open Questions

- Everything D7 and D8 say about OpenCode 2.3.x is read from its source, not run: the live
  probe of task 1.4 confirms the routes, the Basic password, the event names, the steer
  delivery and Exa without a key, and corrects the backend where they differ. Deferrable:
  the backend's shape and the specs do not depend on the details.
