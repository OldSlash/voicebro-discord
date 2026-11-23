# Design

## Context

See proposal.md for why. The state this design starts from:

- **The card.** `apps/desktop/ui/index.html` shows a waiting code as
  `<code id="owner-command">/link K7QF-2M9X</code>` with `user-select: all` and no button.
  `main.js` already has `copyText(text, what)`, which writes through
  `navigator.clipboard.writeText` and logs `<what> copied to the clipboard` in the window's
  Console; the Console's Copy uses it.
- **The bot.** `discord/mod.rs` connects with `GUILDS | GUILD_VOICE_STATES` only, so no
  message reaches the app. `/link` is a slash command with one `code` option;
  `handle_link` passes the sender's id, a cleaned name and the typed text to
  `VoiceHost::complete_link`, which is `OwnerState::complete` (`owner.rs`). Its outcome's
  `reply()` is the ephemeral answer. `complete` already normalizes the code (case, spaces,
  dashes, `O`/`I`/`L`), counts wrong tries from anyone and never logs the typed value.
- **The status.** `DiscordStatus` carries `user_name` (set in `ready`, cleared on
  disconnect) but not the bot's account id.
- **serenity 0.12.5** (read in its source in the cargo registry):
  `GatewayIntents::GUILD_MESSAGES` (`1 << 9`) and `DIRECT_MESSAGES` (`1 << 12`) are not
  privileged; `MESSAGE_CONTENT` (`1 << 15`) is. `EventHandler::message(ctx, Message)`
  gets each new message; `Message` has `author` (with `bot`), `webhook_id`, `guild_id`
  (`None` in a direct message), `kind`, `mentions: Vec<User>` and
  `mentions_user_id(id)`. `CreateAllowedMentions::new()` sends an empty `parse` list, so
  the message notifies nobody; `answers.rs` already posts that way.
- **Discord.** Without the Message Content intent a bot still gets the text of direct
  messages to it and of messages that mention it; other messages arrive without their
  text. A reply with a message reference needs Read Message History, which is not among
  the five permissions the user guide asks for.

## Goals / Non-Goals

**Goals:**

- One copy in the window and one paste and Enter in Discord link the account.
- The code rules of `owner-only-access` stay as they are, whichever way the code arrives.
- No new permission, no privileged intent, no setting.

**Non-Goals:**

- Removing `/link`. It stays for whoever prefers it.
- Opening a direct message to the bot from the window, and deleting the pasted message.
- Reading any message for anything other than a link code.

## Decisions

### D1. The copied message is `<@bot id> link <code>` [decided alone]

- **Format.** `<@1568533518601031690> link K7QF-2M9X`: the bot's raw mention, the word
  `link`, the code as the card shows it.
- **Why the mention.** In a server channel Discord gives the bot the text only of a
  message that mentions it. In a direct message the mention changes nothing. So one text
  works in both places, and the user does not have to choose.
- **Why the raw form.** Discord makes a mention from `<@id>` in the text of a sent message;
  a typed `@Name` becomes a mention only when picked from Discord's list, and names can
  repeat. Discord shows `<@id>` as the bot's name once the message is sent. This is
  checked live in task 3.3.
- **Why `link`.** Others in the channel see what the message is for, and the parser can
  tell a link attempt from a chance mention.

Alternatives considered:
- **Copy `/link K7QF-2M9X`.** Discord's client does not run pasted slash text, as the
  user found.
- **The Message Content intent.** It would let the bot read `link K7QF-2M9X` without a
  mention, but it is privileged: the user would have to switch it on in the Developer
  Portal, the user guide says to leave it off, and Discord closes the connection (4014)
  when a bot asks for it without that switch.
- **Direct messages only.** The copy would be the code alone, but the user would first
  have to find the bot and open a direct message to it. Direct messages stay one of the
  places.

### D2. Which messages are link attempts

A new pure module `discord/link_message.rs`, tested without Discord:

```text
link_attempt(text, bot_id) -> Option<&str>   // the code part, or None
```

1. Remove every `<@bot_id>` and `<@!bot_id>` from the text; trim.
2. If the rest starts with `link` or `/link` (any case) followed by whitespace or the end,
   the attempt is what follows, trimmed (it may be empty or wrong: that counts as a wrong
   code, as with `/link`).
3. Otherwise, if `owner::normalize_code` accepts the whole rest, the attempt is the rest.
4. Otherwise `None`: no attempt, nothing counted, no answer.

The handler decides before calling it:

| Message | Handled |
|---|---|
| author is a bot, or `webhook_id` is set | no |
| `kind` other than a regular message or a reply | no |
| in a direct message (`guild_id` is `None`) | yes |
| in a server, `mentions_user_id(bot)` | yes |
| in a server without the bot's user mention (a role mention or `@everyone` does not count) | no |

A message that is handled and gives an attempt goes to `VoiceHost::complete_link` with
`author.id`, `display_name(&author)` and the attempt, as `/link` does. Edits are not read.

### D3. The answer

`channel_id.send_message` with `CreateMessage::new().content(outcome.reply())
.allowed_mentions(CreateAllowedMentions::new())`, in the channel the message came from. It
is a plain message, not a reply with a reference, so Read Message History is not needed. A
failed send logs `owner link: could not answer in <channel id>: <error>`; the window shows
the result through `AppEvent::Owner` either way.

The texts are `CompleteOutcome::reply()`'s. They speak to "you" and name the linked
account, which reads well in a channel too. None holds the code or the typed text.

### D4. Intents and what the bot receives

`GUILDS | GUILD_VOICE_STATES | GUILD_MESSAGES | DIRECT_MESSAGES`. With `GUILD_MESSAGES`
Discord sends an event for every message in the channels the bot can see; without the
content intent these come without text unless the bot is mentioned. The handler returns
on the first check for them, logs nothing and keeps nothing. The privacy policy
template's "What the bot receives from Discord" says this, and that the text of direct
messages to the bot and of messages that mention it is read only for a link code.

The serenity cache keeps no messages: `cache_settings()` (`discord/mod.rs`) already sets
`max_messages = 0`, and users and channels are not cached either.

### D5. The bot's account id in the status

`DiscordStatus.user_id: Option<String>`, set in `ready` next to `user_name` and cleared
with it on disconnect. A string, since a JavaScript number cannot hold a Discord id
exactly. The window builds the message from `status.discord.user_id` and the waiting code;
the code never goes to the core for this.

### D6. The card

While a code waits:

```text
Linked Discord account   not linked
  K7QF-2M9X                                [ Copy ]
  Press Copy, then paste it in Discord and send it: in a direct message to
  the bot, in the chat of its voice channel or in another channel it can read.
  Or type /link and the code.                                  9:41 left
  [ Cancel ]
```

- The code is shown large, as before, without `/link` in front of it.
- **Copy** is the primary button. It calls `copyText(message, "link message")`, so the
  Console line names no code. On success the card's message line says "Copied. Paste it
  in Discord and send it." On failure it says that copying failed and to type `/link` with
  the code.
- While `status.discord.user_id` is empty, Copy is disabled and the card says "Copy works
  once the bot is connected to Discord." The code stays shown for `/link`.
- The help line under the buttons says that the code can be pasted or sent with `/link`.
- The form-state rule holds: this part is status and is redrawn from it.

Alternative considered: **copy at once when Link is pressed.** It would save a click but
overwrite the clipboard without the user asking, also when "Hear only me" in the Claude
dialog starts the code.

### D7. The command mock and the checks

`mock.js` gives the Discord status a `user_id` (`"1234"`) while connected and none with
`?disconnected`. `check-config-tab.mjs` replaces `navigator.clipboard.writeText` with a
recorder in the page, so the check does not depend on the headless browser's clipboard
permission, and checks the copied text, the card's message and the disabled Copy. The
real clipboard is checked once in the real app (task 3.2).

## Risks / Trade-offs

- **A code pasted where the bot cannot read stays valid and visible.** If the user pastes
  into a channel the bot cannot see, or sends it without the mention, nothing happens and
  others there can read the code for up to 10 minutes; one of them could send it to the
  bot first. → The card names the places that work, the bot answers within a second where
  it can read, the window shows whose account was linked, and Unlink undoes it. The user
  guide says to press Cancel when the bot does not answer.
- **The pasted message stays in the channel.** The code in it is dead once used or ended.
  → Accepted; the user may delete their message.
- **The answer in a server channel is public.** Discord has no private answer to a
  message. → Accepted; it says only that an account was linked, or why not.
- **More gateway traffic.** One event per message in every channel the bot can see. →
  Small next to the voice traffic; dropped on the first check.
- **Discord could narrow the content exemption.** → `/link` keeps working without it.
- **Headless.** If `cross-platform-headless` is merged first, `voicebro-server` shares the
  adapter and gets the same intents. It issues no codes, so a pasted message is answered
  "No code is waiting", whose text points at the window. → Accepted for now; that change's
  open question on a terminal owner link covers it.

## Migration Plan

- Nothing to migrate: no setting or file changes. An existing link stays.
- Rollback: an older build connects without the two intents and ignores messages; `/link`
  works as before.
