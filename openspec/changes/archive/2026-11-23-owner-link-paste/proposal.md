# Proposal

## Why

On 2026-11-22 the user asked again, as before, for a simpler owner link: copy one thing
from the window, paste it into Discord, send it, done. Today the Access card shows
`/link K7QF-2M9X` as text. It does not copy reliably, and pasting it into Discord does not
run the command: Discord offers `/link`, and the code then has to be typed into the
command's field by hand.

## What Changes

- **A Copy button in the Access card.** While a code waits, Copy puts a ready message on
  the clipboard: the bot's mention, the word `link` and the code
  (`<@bot account id> link K7QF-2M9X`). Discord shows the mention as the bot's name once
  the message is sent. The card shows the code and says where to paste the message. Copy
  is unavailable until the bot has connected to Discord, since the mention needs the bot's
  account id.
- **The bot reads a pasted code.** A message the user sends in a direct message to the
  bot, or in any channel of a server the bot can read when the message mentions the bot,
  links the sender as `/link` does when its text, without the bot's mention, is
  `link <code>`, `/link <code>` or the code alone. The same code rules apply: one use,
  10 minutes, 5 wrong codes end it. Every other message is ignored and nothing of it is
  kept.
- **The bot answers in the same chat**, with the texts `/link` answers with, and pings
  nobody. In a server channel the answer and the pasted message are visible to the
  channel, since Discord has no private answer to a message; the code is dead once used.
- **Two more gateway intents**, guild messages and direct messages. Neither is
  privileged: Discord gives a bot the text of direct messages to it and of messages that
  mention it without the Message Content intent, so the three privileged intents stay off
  as the user guide says.
- **The Discord status carries the bot's account id**, for the window's mention.
- **`/link` stays** as it is, as a second way.
- **Docs**: the user guide (the link steps, the commands table, what Send Messages is
  for), the privacy policy template (what the bot receives from Discord) and
  `docs/ARCHITECTURE.md`.

Not in this change: deleting the pasted message (it needs Manage Messages, which the bot
does not ask for), opening a direct message to the bot from the window, and a Discord
sign-in (OAuth) in place of the code.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `owner-only-access`: a code sent in a message links like `/link`; messages are read only
  for a link code; the code reaches the clipboard on Copy and never the bot's answers.
- `settings-ui`: the Access card's waiting code with Copy and where to paste it; the
  Claude subscription dialog's "Hear only me" shows that card.
- `discord-connection`: the bot connects without any privileged gateway intent and still
  reads a pasted link code.

## Impact

- Core: `discord/mod.rs` (the intents, the bot's account id in `DiscordStatus`),
  `discord/handler.rs` (the `message` event and the answer), a new
  `discord/link_message.rs` (which messages are link attempts), `owner.rs` (its log
  wording names a code rather than `/link`).
- Window: `apps/desktop/ui/index.html`, `main.js`, `styles.css`; the command mock
  `ui-dev/mock.js` (the bot's id in the status), `check-config-tab.mjs` and
  `check-engine-config.mjs`.
- Docs: `docs/user-guide.md`, `docs/PRIVACY-POLICY-TEMPLATE.md`, `docs/ARCHITECTURE.md`,
  `AGENTS.md`.
- Privacy: with the guild messages intent Discord also sends the bot an event for every
  message in the channels it can see, without the text unless the message mentions the
  bot. The bot drops these at once; the privacy policy template says so.
- No new crate, no settings change, no new permission. The version does not change.
