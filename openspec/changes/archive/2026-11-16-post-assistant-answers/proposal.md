# Proposal

## Why

When the voice model hands a question to the Codex agent (a web search, a recipe, a price),
the agent writes a full answer with facts, lists and links, and the voice only retells it:
it shortens the answer, drops links and formatting, and sometimes says "the link is on the
screen" when nothing is shown (`docs/HANDOFF-MODES-REPORT.md`, question D1, all four
modes). The user wants that full answer posted to Discord as text. The people in the
channel can then read it, open its links and see their previews while the voice speaks
(the user's request of 2026-11-15).

## What Changes

- **The agent's answers are posted to Discord.** Each time the Codex agent completes a task
  the voice delegated, the bot posts the agent's final answer as the agent wrote it, as one
  or more messages in a Discord text chat. Only the final answer of a completed task is
  posted. Not posted: the agent's interim lines ("I'll look that up"), its tool calls and
  reasoning, what the voice says, and what people say. A task that is interrupted or fails
  posts nothing.
- **Where the answers go: the new setting `assistant.post_answers`**, one of:
  - `off`: nothing is posted. This is the default.
  - `voice_channel`: the built-in text chat of the voice channel the bot joined.
  - `command_channel`: the channel where `/join` was typed, where the panel is.
  - `channel`: one text channel chosen in the window and stored in
    `assistant.answers_channel` (its id). Only answers from that channel's server go there;
    in other servers nothing is posted.
- **Readable in Discord.**
  - The answer keeps its Markdown.
  - A long answer is split into messages of at most 2000 characters, at paragraph or line
    breaks and never inside a code block.
  - A Markdown table is posted inside a code block, so its columns stay aligned.
  - A small first line names the task.
  - Nothing in an answer pings anyone: `@everyone`, `@here`, roles and users stay plain
    text.
  - Link previews stay on, so a linked page shows its picture.
- **A control in the Config tab's Codex card.** "Post answers to Discord" offers the four
  choices. For `channel` it shows the text channels of the servers the bot is in.
- **A refused post is visible.** When Discord refuses a post (missing permission, deleted
  channel), the status shows a connection error that names the channel and the reason, and
  the log has the same line. A later successful post clears the error. Nothing is retried in
  a loop.
- **No new bot permission.** The bot already needs Send Messages and Embed Links for the
  panel. The user guide says how to check them for the chosen channel.

Not in this change: direct messages, a filter that hears only the owner, posting the voice's
or people's transcripts, uploading files, streaming an answer while it is written, and other
assistant engines. The answer type is engine-independent, so a later engine can post through
the same path.

## Capabilities

### New Capabilities

- `assistant-answer-posting`: which assistant answers are posted to Discord, where they go,
  how they are formatted and split, and how a refused post is shown.

### Modified Capabilities

- `app-settings`: the new settings `assistant.post_answers` and `assistant.answers_channel`,
  and their validation.
- `settings-ui`: the Codex card gets the "Post answers to Discord" control and the channel
  list; the list of fields per card names them.

## Impact

- `crates/voicebro-core`:
  - `assistant/mod.rs`: an engine-independent `AgentAnswer` and the sink it is sent
    through.
  - `assistant/codex/session.rs`: keeps the last agent message and the delegated task of
    each agent turn, and sends the answer at `turn/completed`.
  - A new `discord/answers.rs`: finds the destination, formats and splits the answer, posts
    it, reports refusals, and lists the text channels.
  - `discord/session.rs`: the session keeps the channel `/join` was typed in.
  - `settings/mod.rs`: two fields and their validation.
  - `runtime.rs`: wires the sink to the Discord adapter and applies the settings.
  - `testdata/fake-codex`: a delegated turn with interim and final agent messages.
- `apps/desktop`:
  - A command that lists the text channels.
  - `ui/index.html` and `ui/main.js`.
  - `ui-dev/mock.js` and `ui-dev/check-config-tab.mjs`.
- `apps/probe`: a probe that posts a sample answer to the test channel and reads it back.
- Docs: `docs/ARCHITECTURE.md` (4.2, 4.5, 4.6, data flow), `docs/user-guide.md` (setup and
  the setting), `AGENTS.md` (the new probe).
- No new dependency: serenity already sends messages. A settings file from 1.0.1 loads with
  `post_answers` set to `off`; an earlier build keeps the new fields as unknown fields.
