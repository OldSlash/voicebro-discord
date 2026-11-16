# Design

## Context

See proposal.md for the motivation. The state this design starts from:

- **Agent work in the session.** The voice model delegates a question to the Codex agent.
  `CodexSession::on_notification` (`assistant/codex/session.rs`) already sees that work:
  - `turn/started`, then `item/completed` for each item, then `turn/completed` with
    `turn.status` (`completed`, `interrupted`, or failed with `turn.error`) and
    `durationMs`;
  - `agent_item_line` (`assistant/chat_log.rs`) turns a `userMessage` into the delegated
    task, unwrapped from `<realtime_delegation>`, and an `agentMessage` into an `agent`
    line.

  In the calls of 2026-11-10 (`docs/HANDOFF-MODES-REPORT.md`), a turn had an interim
  `agentMessage` ("I'll look that up") before the web search and the final one after it,
  with Markdown and a link. The voice spoke its retelling about 15 s after
  `turn/completed`.
- **What crosses from the assistant to the rest of the app.** `AudioPorts` is built per
  guild by `GateHost::on_join` (`runtime.rs`). Besides the audio it already carries
  `failures`, a per-guild reporter of codec failures. Those reports reach a handler task
  that `Runtime::start` spawns, which acts on Discord through the `VoiceControl` trait
  (implemented by `DiscordRuntime`, faked in the tests).
- **What Discord already knows.**
  - `/join` (`discord/handler.rs`) knows the command's channel (`command.channel_id`) and
    keeps it only inside `PanelRef` once the panel is posted.
  - `VoiceSession` (`discord/session.rs`) keeps the voice channel.
  - The cache holds the guilds with their channels and roles (intents `GUILDS` and
    `GUILD_VOICE_STATES`).
  - The bot already needs Send Messages and Embed Links for the panel
    (`docs/user-guide.md`).
- **Connection errors.** Connection errors per source (`ConnectionErrors`, `status.rs`) are
  in the status snapshot and the window. A source is cleared by reporting `None`.

## Goals / Non-Goals

**Goals:**

- One post per completed agent task, made at `turn/completed`, so the text is in Discord
  while the voice still retells it.
- One engine-independent path from the assistant to Discord, so a later engine (the
  cascaded mode discussed on 2026-11-15) posts through it unchanged.
- Settings applied at the moment of the post, so a change applies to the next answer
  without restarting anything.

**Non-Goals:**

- Editing a message while the agent writes, threads, reactions, buttons on the answer.
- Uploading images or files; pictures come only from Discord's link previews.
- Posting to channels of servers the bot is not in a voice channel of, or posting the
  answers of tasks still open at a leave (D4).
- Changing what the voice says or how the answer reaches the voice.

## Decisions

### D1. What is posted: the last agent message of a completed task [decided alone]

The request was "what Codex itself says, the big answers". This is read as the agent's
answer to a delegated task, not the voice's speech and not the people's transcripts. A task
can hold several `agentMessage` items: the interim line before a search and the answer
after it (report, D1 `items`). Only the last one is posted, when the turn completes with
status `completed`.

Alternatives considered:
- **Every agent message.** It posts the interim lines too, as noise.
- **The voice's transcript.** It is the short retelling this change exists to avoid.

Both can be added later as values of a separate setting.

### D2. The session collects per turn and sends at `turn/completed`

`CodexSession` keeps a small map from turn id to `{task, last_agent_text}`:
- `turn/started` inserts an entry.
- `item/completed` with a `userMessage` sets the task, using `delegated_task_text`.
- `item/completed` with an `agentMessage` replaces the last text.
- `turn/completed` removes the entry. When the status is `completed` and the text is not
  empty, it sends an `AgentAnswer { guild, task, text }`.

The turn id comes from the notification's `turnId` (or `turn.id` on the turn events). An
item without a known turn id is held under the most recent turn. The map is cleared when
the session shuts down. The chat log keeps its own lines as today.

**Checked in the schema** (task 3.1, 2026-11-16): `probe codex-schema` against Codex CLI
0.167.0 (423 v2 types), files `v2/TurnStartedNotification.json`,
`v2/ItemCompletedNotification.json` and `v2/TurnCompletedNotification.json`:
- `turn/started` and `turn/completed` carry `{threadId, turn}`; the turn id is `turn.id`
  (a string, "UUIDv7" for Codex's own turns), required.
- `item/completed` carries `{threadId, turnId, item, completedAtMs}`; `turnId` is required.
- `turn.status` (`TurnStatus`) is one of `completed`, `interrupted`, `failed` and
  `inProgress`; `turn.error` (`TurnError`, with a required `message`) is set for a failed or
  interrupted turn; `turn.durationMs` is optional.
- An `agentMessage` item has `text` (required) and an optional `phase` (`MessagePhase`:
  `commentary`, `partial_answer` or `final_answer`). The schema says providers do not
  emit it consistently, so it is not used: the last agent message stays the answer (D1).

Alternative considered: deriving the answer from the chat log's entries. It would couple
the post to the window's log and its bound of 600 entries, and the log has no turn
boundaries.

### D3. An engine-independent `AgentAnswer` through `AudioPorts`, a handler in the runtime

- **The types.** `assistant/mod.rs` gets `AgentAnswer { guild: u64, task: Option<String>,
  text: String }` and `AnswerSink`, a clonable sender over a bounded tokio channel
  (capacity 16).
- **A full channel.** `try_send` drops the new answer when the channel is full, logs a
  warning and counts the drop. Sixteen waiting answers means Discord has been refusing or
  stalling for minutes.
- **The ports.** `AudioPorts` gains `answers: AnswerSink`, next to `failures`, and
  `GateHost::on_join` fills it.
- **The handler.** `Runtime::start` takes the receiver, as it does for codec failures, and
  spawns a handler. For each answer the handler reads the current `assistant` settings and
  calls `AnswerPoster::post(answer, destination)`. `AnswerPoster` is a trait implemented by
  `DiscordRuntime` and faked in the runtime tests.

Alternative considered: publishing on the `AppEvent` bus and letting Discord subscribe. A
broadcast receiver that posts over HTTP can lag and lose events silently, and the core
avoids routing control through the window's event bus.

### D4. Four destinations, `off` by default [decided alone: `command_channel` and the default]

The user asked for "where the bot joins, or some other text channel". These are
`voice_channel` and `channel`.
- **`command_channel`** is added because `/join` is often typed in a text channel, where
  the panel already is. It costs one stored channel id.
- **Off by default**, like `save_chat_history`: an answer posted in Discord is public in
  that channel and stays there. The user turns it on once in the Codex card.

The destination is resolved when the answer arrives:

| `post_answers` | Channel | Answer already handed over when the bot left |
|---|---|---|
| `off` | none | — |
| `voice_channel` | the session's voice channel; its built-in text chat has the same id | not posted, logged |
| `command_channel` | the channel `/join` was typed in for the current session | not posted, logged |
| `channel` | `answers_channel`, if it belongs to the answer's guild | posted |

**A task still open at the leave posts nothing [decided alone, at the archive,
2026-11-16].** The table's last column first read "Bot has left the voice channel", and
the spec had the chosen channel still get an answer whose task completes after `/leave`.
That cannot happen as built: D2 forgets the session's open turns at the leave, the session
unmaps its thread at the leave (`CodexSession::shutdown`), and Codex stops 0.5 s after the
last leave, so such a task produces no answer at all. The question went to the user while
building; the user asked to commit, merge and archive without choosing, so the spec follows
the code: a task still open at the leave posts nothing, whatever the destination, and the
two "Task completes after the leave" scenarios became one under "Only the final answer of a
completed agent task is posted". The resolution's last column covers only the race of an
answer handed over just before the leave. Posting such answers would mean keeping the
session's thread and open turns, and Codex itself, running after the last leave, against
the rule that the app-server stops after it; that is a change of its own if wanted.

For `channel` with a channel of another server, nothing is posted and the log says so once
per voice session [decided alone]. The alternative, falling back to the voice channel's
chat, would post publicly somewhere the user did not pick.

### D5. The session keeps the join's channel

`VoiceSession` gets `command_channel: ChannelId`, set by `/join` from `command.channel_id`
before the panel is posted. This makes `command_channel` work even when the panel post
fails. `PanelRef` stays as it is.

### D6. Plain messages, a task line, tables in code blocks, no pings

- **Plain content, not embeds.** Links in message content get Discord's previews, which are
  the pictures; links inside an embed do not. Plain text is also selectable and copyable as
  written.
- **Task line.** The first part starts with `-# Codex: <task>`, Discord's small subtext
  line. The task is cut to 150 characters with `…`. Without a task, the line is
  `-# Codex`.
- **Tables.** A Markdown table (a header row, then a `|---|` separator row) is wrapped in a
  code block, because Discord does not render tables. Everything else stays as the agent
  wrote it.
- **No pings.** Every message is sent with `allowed_mentions` that allow nothing
  (`parse: []`). Mention text stays visible but notifies no one. The probe (D11) checks it
  on a real message.

Decided while building [decided alone]:
- **The table's columns are padded.** In a code block a table lines up only if its cells
  are padded, and the agent writes them unpadded (`| Day | High |`). So each column is
  padded to its widest cell, and the separator row becomes plain dashes (`:---:` alignment
  marks are dropped). Width counts characters, so CJK or emoji cells can still be off.
  The rest of the answer stays as written.
- **A link inside a table cell** ends up inside the code block: it shows as text, with no
  preview. Links outside tables keep their previews.
- **The task line** has its whitespace folded to single spaces, so a task with line
  breaks stays one line.

### D7. Splitting is a pure function

`split_for_discord(text, limit = 2000) -> Vec<String>` in `discord/answers.rs`:
- It cuts at the last blank line before the limit, else the last line break, else the last
  space, else at the limit.
- It tracks code fences. A cut inside a fence closes it in that part and reopens it with
  the same info string (language) in the next.
- The task line counts toward the first part.

The parts go out one after another. A refused part stops the rest (D8). Discord's rate
limits are waited out by serenity; a 429 is not a refusal.

Decided while building [decided alone]:
- **A cut in the second half of the message comes first.** Taken literally, "the last
  blank line" can be the one after a short intro line, which makes a first message of one
  line. So a cut of any kind in the second half of the message is preferred to a better
  kind earlier, outside code blocks first, then a line break inside a code block; only when
  the second half has none is an earlier cut taken. "Here is the code:" followed by a
  3 000-character block is then two messages, not three.
- **No cut right after a code block opens or right before it closes**, which would leave
  an empty block.
- **Length is counted in UTF-16 units**, which is never less than the number of
  characters; a character outside the Basic Multilingual Plane (most emoji) counts twice,
  so a part is within Discord's limit however Discord counts.
- A cut at a space drops that one space; a cut at a line break drops the blank lines
  after it.

### D8. A refused post is a connection error of the source `answers`

On an HTTP error from Discord:
- The handler reports `answers` with "answers: cannot post to #<channel> in <server>:
  <Discord's message>" and logs the same line.
- It sends no further parts and never retries.
- The next successful post reports `None`, which clears the error.

A channel that is gone from the cache is reported the same way, without a request.

Decided while building [decided alone]:
- **The missing permission is named.** Discord's reason for a missing permission is only
  "Missing Permissions" (code 50013). When the cache holds the bot's member, the line adds
  which of View Channel, Send Messages and Embed Links the bot lacks there, computed as for
  the channel list (D9): "answers: cannot post to #rules in Test server: Missing
  Permissions (HTTP 403, code 50013); the bot lacks Send Messages here".
- The refusal is logged at Warn, the post at Info.
- The stored message is "cannot post to #… in …: …" without the `answers:` in front: the
  Status tab prints every connection error as "<source>: <message>", so it reads
  "answers: cannot post to …" as above. The log line is "[answers:<guild>] cannot post to
  …".
- An answer not posted on purpose (the bot had left, another server's channel, Discord not
  connected, posting off) sets no error and leaves an `answers` error as it is; only a post
  clears it.

### D9. The channel list comes from the cache, with a can-post mark

A Tauri command `discord_text_channels` asks `DiscordRuntime::text_channels()`. It returns,
for each cached guild, its text and announcement channels in Discord's order (category,
then position), each with `id`, `name` and `can_post`.
- **`can_post`** comes from the bot's permissions in the channel: View Channel, Send
  Messages and Embed Links, computed from the cached roles and overwrites. When the bot's
  own member is not in the cache, `can_post` is `null`, and the window marks nothing.
- **Voice channels are not listed** [decided alone]. Their chat is the `voice_channel`
  choice, which follows whatever channel the bot joins.
- **Not connected.** The command returns no guilds, and the window shows the stored id.

### D10. The Codex card control

After "Save chat history", the card shows:
- a select "Post answers to Discord" with the four choices;
- under it, only for A chosen channel, a second select grouped by server
  (`<optgroup>`), with "can't post" after a channel whose `can_post` is `false`.

Saving:
- Choosing Off, The voice channel's chat or The channel of /join saves `post_answers` at
  once and keeps `answers_channel`.
- Choosing A chosen channel saves both fields together, once a channel is picked. A stored
  channel id makes that immediate. With no channel picked, nothing is saved, and the card
  says "Pick a channel".

The help line names the three permissions. The list is reloaded when the Config tab opens
and on every Discord status event that changes the connection. The mock gets a channel list
with one channel marked can't post, and a disconnected case.

### D11. Tests and the live probe

- **Unit tests:**
  - `split_for_discord`: paragraphs, one long line, a long code block with a language, the
    limit hit exactly, the task line;
  - the table wrapping;
  - the task line cut;
  - destination resolution for every row of D4;
  - the settings validation of every spec scenario.
- **Session test.** `fake_codex.js` gets a mode that answers a delegation with
  `turn/started`, an interim `agentMessage`, a `webSearch` item, a final `agentMessage`
  and `turn/completed`, plus an interrupted variant. A test checks that one `AgentAnswer`
  arrives with the final text and the task, and none for the interrupted turn.
- **Runtime test** with a fake `AnswerPoster`:
  - a settings change between two answers moves the second one;
  - a refusal sets and a success clears the `answers` error;
  - `off` makes no call.
- **Probe** `probe discord-post [--file <answer.md>]`. It uses `VOICEBRO_DISCORD_TOKEN` and
  `VOICEBRO_TEST_CHANNEL`, which the `discord-lifecycle` probe already uses, and posts a
  built-in sample answer through the same formatting. The sample is 4500 characters with a
  table, a 2500-character code block, a link, `@everyone` and a user mention. The probe
  checks the messages Discord returns: each at most 2000 characters, the fences balanced,
  `mention_everyone` false, `mentions` empty. It prints the message links. A post needs no
  Read Message History, because the create call returns the message.
- **Live acceptance**, by the project rule: a delegated question in a real voice channel
  with two people and a game running, once per destination. The report says what was not
  covered.

### D12. Logging

- Each post: `[answers:<guild>] posted the Codex answer to #<channel> (<n> messages,
  <chars> characters)` at Info.
- Each answer not posted: the reason at Info.
- The text itself: `detail!("answers", …)`, so it reaches the file only with the verbose
  log, as the chat lines do.

As built [decided alone]: the post line also names the server ("to #<channel> in
<server>"). With posting off, each completed task logs "the Codex answer was not posted:
posting answers is off", one line per task. The other-server line comes once per voice
session of the answer's server, as the spec says; later answers of that session log
nothing.

## Setting it up (for the user guide)

The bot needs no new permission. For every destination it needs View Channel, Send
Messages and Embed Links in that channel. They are in the five permissions of the install
link, but a channel's own overrides can take them away.

- **The voice channel's chat** (`voice_channel`). Every voice channel has a built-in text
  chat, opened with the speech-bubble icon of the voice channel. Nothing to configure; the
  answers follow the bot to whatever channel `/join` puts it in. If one channel stays
  silent:
  1. Right-click that voice channel.
  2. Choose Edit Channel → Permissions.
  3. Check the bot's role for Send Messages and Embed Links.
- **The channel of /join** (`command_channel`). The answers go where the panel is. Type
  `/join` in the text channel you want the answers in. Typed in the voice channel's own
  chat, this is the same as the voice channel's chat.
- **A chosen channel** (`channel`). Good for a dedicated `#answers` channel that keeps every
  answer in one place.
  1. Create the channel in Discord. If it is private, add the bot's role to it.
  2. In VoiceBro, open Config → Codex → Post answers to Discord, choose A chosen channel and
     pick the channel from the list. The list comes from the connected bot.
  3. A channel marked "can't post" needs the three permissions above. Fix them in Discord;
     the mark goes away the next time the Config tab is opened.

  Only answers from that channel's server go there. In another server nothing is posted.

## Risks / Trade-offs

- **Flooding.** A long answer posts several messages, and a chatty session posts many. →
  Each answer is one post of at most a few messages. The setting is off by default and is
  one click to turn off. No cap in this change.
- **Privacy.** A chosen channel may be seen by more people than the voice channel, and the
  posts stay in Discord's history. → Off by default, and the help line names where the
  answers go.
- **The interim line is the last message.** If the agent's last message is an interim one
  (a task that ends after a tool call without a final message), that line is posted. →
  Accepted; such a turn usually ends `interrupted` or with an error, which posts nothing.
- **The bot's member is missing from the cache.** `can_post` is then `null`, and the window
  marks nothing. → A refused post is still shown as the `answers` error (D8).
- **Discord changes `-#` subtext or table rendering.** → Text only. The worst case is a
  visible `-#` or an unrendered table.
- **A link in parentheses gets no preview.** Found by the probe on 2026-11-16: Discord gave
  no preview to `(source: [Wikipedia on Bangkok](https://…))`, and gave one to the same
  link outside parentheses, bare or masked. Agents often cite sources that way. → Accepted
  for now: the answer is posted as written, the link still works. Moving or repeating such
  links would change the agent's text, which this change does not do; it is the user's
  call.

## Migration Plan

- No migration. A 1.0.1 settings file loads with `post_answers: off` and an empty channel;
  the next save writes both.
- [decided alone] An unknown `post_answers` value is refused while the document is read,
  with the field and the four values in the message, at a save and at a load alike, as
  `speaker_labels` and `session.mode` already are. A file edited by hand to `thread` stops
  the app's start with that message until it is fixed; a later build that adds a value
  would need the same care as `diagnostics.opus_codec` got.
- [decided alone] `changed_sections` reports the two fields as their own part, `answers`,
  not as `assistant`: an `assistant` change reconfigures the gates and the assistant, and
  these two fields need neither.
- A build before this change keeps the two fields as unknown fields.
- To roll back, choose Off, or install the previous tag; nothing else is stored.
