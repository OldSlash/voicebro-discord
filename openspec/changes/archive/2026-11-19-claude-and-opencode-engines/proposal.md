# Proposal

## Why

Today the assistant can only be OpenAI's Codex, so a user without a ChatGPT account, or one
who prefers another model, cannot use VoiceBro at all. The user wants two more engines:
Claude Code driven headless (`claude -p`) and OpenCode, each replacing Codex in the Whisper
mode (the user's request of 2026-11-19). Neither has a realtime voice call, so the voice
mode stays Codex-only.

A Claude subscription (Free, Pro, Max, Ultra, Team or Enterprise, signed in through
claude.ai) is personal: Anthropic's Consumer Terms (§2, effective 2025-10-08) forbid making
the account available to anyone else, and Claude Code's legal page says that subscription
limits assume individual use and that third parties may not route their users' requests
through subscription credentials. When the bot lets everyone in a voice channel talk to an
assistant that runs on the operator's subscription, the operator makes their account
available to the others. VoiceBro already has the remedy, the owner-only access mode with
the owner link; the user wants the app and the README to say so, and to offer that mode the
first time Claude is chosen. API keys and cloud providers (Bedrock, Vertex, Foundry) are
under Anthropic's Commercial Terms and get no such notice; OpenCode gets none either.

## What Changes

- **Three assistant engines.** `assistant.engine` takes `codex` (the default), `claude` or
  `opencode`, chosen in Config → Assistant and applied at once, without a restart and
  without the bot leaving its channel, as a change of the assistant mode is today.
- **The voice mode needs Codex.** Claude and OpenCode work only in the Whisper mode. A save
  that pairs either of them with `assistant.mode = realtime` is refused; choosing either in
  the window while the realtime mode is on switches to the Whisper mode in the same step
  (with the Whisper files dialog when they are missing), and the realtime choice is shown
  disabled with "The voice mode works only with Codex".
- **Claude Code engine.** The app runs the unmodified `claude.exe` in headless mode, one
  long-lived process per guild with stream-json input and output: questions as text turns,
  the answer streamed into the local voice, interrupt on barge-in, the next turn for a
  question asked while one runs. Only Claude's web search and web fetch tools are allowed;
  the user's own Claude Code settings, CLAUDE.md files and MCP servers are not loaded, and
  no session is written to disk. The app never reads, stores or passes Claude credentials.
- **Claude Code on the Status tab.** Found or not, version, the kind of sign-in (Claude
  subscription with its plan, API key, cloud provider), with "Install Claude Code" (winget
  `Anthropic.ClaudeCode` in a console window the user sees), "Sign in" (opens a console
  window running Claude Code's own sign-in) and "Check".
- **The Claude subscription notice.**
  - A notice in the Assistant and Access cards while the engine is Claude, the sign-in is a
    Claude subscription (or not known yet) and the access mode is not "Only my Discord
    account". It says, with links to Anthropic's terms, that a Claude subscription may be
    used only by its holder and how to let only the holder's voice through.
  - The first time the user chooses Claude in the window under those conditions, a dialog
    asks whether to let only their own voice through. "Only my voice" sets
    `discord.access = owner` and, when no owner is linked, starts the link code at once;
    "Not now" changes nothing. The dialog is shown once (`claude.subscription_notice_shown`).
  - No notice and no dialog for an API key, a cloud provider or OpenCode. The app enforces
    nothing: the choice stays the operator's, as the access mode is today.
- **OpenCode engine.** The app starts `opencode serve` on 127.0.0.1 on a free port with a
  password made for that run, with a configuration the app writes into its own folder: one
  agent that may only fetch pages and search the web, and the user's own instruction files
  and skills of other agents not loaded. Questions go to a session per guild; deltas are
  spoken, abort on barge-in, OpenCode's own queue for a question asked while one runs. The
  app's sessions are deleted when the bot leaves.
- **OpenCode web search is on.** OpenCode's web search goes to a separate search service
  (Exa or another provider OpenCode offers), not to the model's provider. The app turns the
  tool on; the README, the user guide, the consent text and the privacy policy template say
  that search queries the model writes go to that service.
- **OpenCode on the Status tab.** Found or not, version, the configured providers and the
  model in use, with "Install OpenCode" (winget `Anomaly.OpenCode`) and "Check". There is no
  sign-in button: the user sets up providers and keys in OpenCode itself, and the Status tab
  says how.
- **The engine card.** The Config tab's Codex card becomes the engine card, titled after
  the selected engine: Codex's own fields for Codex, the executable and the model for
  Claude Code and for OpenCode, and the fields every engine shares (speaker labels, save
  chat history, post answers, instruction) for all three.
- **One instruction for every engine.** The user's assistant instruction stays in the
  engine card and applies to whichever engine runs.
  **BREAKING (settings file):** `codex.instructions` becomes `assistant.instructions`; a
  file with the old field loads with its text moved, and the next save writes only the new
  one. A build before this change reading the new file finds no instruction.
- **Engine-neutral names.** The tab "Codex chat" becomes "Chat" and its labels name the
  engine in use; the chat history file becomes `logs\chat-YYYY-MM-DD.log` (older
  `codex-chat-*.log` files are left alone); `/optin`'s description, the member panel's
  Engine field, the posted answer's label and the latency line name the engine in use.
- **Owner link per engine.** Link reads the running engine's account: the ChatGPT account
  for Codex, the Claude account for Claude Code; OpenCode has none, so a link made with it
  records no account. Changing the engine keeps the link.
- **Consent names the right recipient.** The consent text names Anthropic through Claude
  Code, or OpenCode with the provider of the chosen model and the web search service. A
  change of engine, or of OpenCode's provider, ends the records as an engine change does
  today.
- **Docs.** README (features, requirements, quick start, limitations, a section on personal
  Claude subscriptions with links to the terms, Built with, not affiliated with Anthropic or
  the OpenCode project), user guide, privacy policy template, third-party notices,
  `docs/ARCHITECTURE.md`, `AGENTS.md`. Version 1.3.0.

Not in this change:

- a realtime voice for Claude or OpenCode;
- Claude through the Agent SDK, the Messages API or an API key the app stores; the app only
  runs Claude Code, which keeps its own sign-in;
- Claude's reasoning effort and OpenCode's agent variants (a model choice only);
- forcing the owner-only mode: the notice and the dialog advise, they do not refuse a join;
- saving Claude or OpenCode model lists to disk, as `codex-lists.json` does for Codex.

## Capabilities

### New Capabilities

- `assistant-engines`: the choice of the engine, which modes each supports, switching at
  once, what all engines share (session rules, instruction, answer posting, chat log) and
  how the engine is named to people.
- `claude-assistant`: Claude Code as the engine: finding and installing it, its sign-in kind,
  the headless process and its lockdown, text turns, interrupt, queued questions, history
  and channel context, the model list, and the subscription notice.
- `opencode-assistant`: OpenCode as the engine: finding and installing it, the local server
  and its lockdown, web search, text turns, abort, queued questions, history and channel
  context, sessions removed at the leave, the model list and the providers shown.

### Modified Capabilities

- `app-settings`: `assistant.engine` takes three values and is checked against the mode;
  `assistant.instructions` replaces `codex.instructions`; new `claude` and `opencode`
  sections.
- `whisper-assistant-mode`: the Whisper mode runs on the selected engine; its turns,
  instruction and "questions wait their turn" are described per engine.
- `owner-only-access`: Link reads the running engine's account; OpenCode links without one;
  an engine change keeps the link.
- `speaker-consent`: the consent text names the engine's recipient; records end when the
  engine or OpenCode's provider changes.
- `settings-ui`: the engine choice, the disabled realtime choice, the engine card with the
  Claude Code and OpenCode fields, the subscription notice and its dialog. The "Discord and
  Codex" card keeps its name and the Codex executable.
- `status-view`: the engine area for each engine, the Claude Code and OpenCode install
  offers, Claude's sign-in, OpenCode's setup hint.
- `assistant-chat-log`: the chat file's name and the labels follow the engine.
- `app-window`: the tab "Codex chat" becomes "Chat"; the Config tab lists the engine card
  in the Codex card's place.
- `voice-channel-session`: the member panel's button reads "Mute assistant" instead of
  "Mute GPT", since the assistant is not always GPT.
- `app-distribution`: files Claude Code and OpenCode keep outside the app's folder, the
  third-party notices and the README's limitations name the new engines.

## Impact

- `crates/voicebro-core`:
  - `assistant/`: a shared text-turn session taken out of `codex/text_session.rs` with a
    small per-engine turn interface; new `claude/` and `opencode/`; an engine switch that
    holds the three engines behind the `Assistant` trait;
  - `settings/`: the engine values and their check against the mode, the moved instruction
    with its migration, the `claude` and `opencode` sections;
  - `owner.rs`, `consent.rs`: the engine's account and recipient;
  - `discord/` (panel, answers, `/optin` description), `latency.rs`, `paths.rs`,
    `chat_log.rs`: engine-neutral names and files;
  - `runtime.rs`: engine changes applied like mode changes;
  - `testdata/`: Node stand-ins for Claude Code and OpenCode next to `fake-codex`.
- `apps/desktop`: the engine built at start (`main.rs`) becomes the engine switch; Claude
  and OpenCode commands (install, sign-in, check, models); the memory split's assistant
  process; `ui/index.html` and `ui/main.js` (Assistant, Claude Code and OpenCode cards, the
  notice dialog, the Status tab, the Chat tab); `ui-dev/mock.js` and a new check script.
- `apps/probe`: `claude-smoke` and `opencode-smoke`.
- Docs: `README.md`, `docs/user-guide.md`, `docs/PRIVACY-POLICY-TEMPLATE.md`,
  `docs/ARCHITECTURE.md`, `AGENTS.md`, `scripts/third-party-notices.ps1`.
- External: Claude Code (proprietary, Anthropic's terms) and OpenCode (MIT), both installed
  by the user, not shipped. No new Rust dependency is expected beyond what the workspace
  has for HTTP and JSON; the design confirms it.
