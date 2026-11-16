# Proposal

## Why

The assistant runs on the user's own ChatGPT subscription through Codex. Today anyone in
the voice channel can say the wake phrase and talk to it, so the subscription answers other
people's requests. Subscription terms limit who may send the requests: OpenAI's terms do not
let an account be made available to anyone else, and Anthropic's terms for a later Claude
engine do not let a third-party app route requests through a plan on behalf of other users.
Hearing or reading the answers is not limited. The user wants an optional mode in which only
the voice of the subscription holder's own Discord account reaches the assistant, linked to
the app once (the user's request of 2026-11-16). The bot itself stays open: anyone may bring
it into a channel. The change `post-assistant-answers` listed "a filter that hears only the
owner" as not in it; this is that filter.

## What Changes

- **Linking the owner's Discord account.** In the window, "Link my Discord account" reads the
  Codex account (as Check does) and shows a one-time code: 8 characters, valid 10 minutes,
  burned after 5 wrong tries. The owner types `/link <code>` in Discord, in a server where
  the bot is or in a direct message to the bot. Discord tells the bot which account sent it,
  and the app stores that account as the owner. Only the code is one-time; the link stays.
- **The link is encrypted and lives in the app's folder.** It is the file `owner-link.bin`,
  encrypted with Windows DPAPI for the current Windows user. A file that was edited, or
  copied to another PC or user, is no link. The window has no field to type an account id.
- **How long a link lasts.** A link stays until one of these:
  - the user presses Unlink;
  - the app's folder is deleted, or the app is uninstalled with its data;
  - another Discord account links with a new code, which replaces it;
  - Codex reports a different ChatGPT account when the app reads it (at a join, Check or
    Link). The link is then removed and the window shows why. The join itself goes on.
- **The setting `discord.owner_only`, off by default.** While it is off, the bot works as
  today, with or without a link.
- **While it is on, only the owner's voice reaches the assistant.**
  - Every other source is dropped before the mix: other members, bots, and sources with no
    known account. So no one else can say the wake phrase or talk to the assistant.
  - The exception is the "only person in the channel" rule. It may name an unknown source
    after the owner when the owner is the only person there.
  - With no owner linked, nobody's voice reaches the assistant until an account is linked.
  - `/status` adds a line saying that only the owner's voice reaches the assistant.
- **The commands stay open.** `/join`, `/leave`, `/status` and the panel's Mute button work
  for everyone, with the mode on or off.
- **The answers stay open.** Everyone in the channel still hears the voice and sees posted
  answers.
- **Speaker identity fix.** The app keeps which Discord account each audio source belongs
  to, bots included. Today a bot's stream, for example a music bot's, gets the name of the
  only person in the channel. Afterwards the "only person in the channel" rule applies only
  to sources Discord never announced.
- **The window.** A new "Access" card on the Config tab, after "Discord and Codex", holds:
  - the switch;
  - the link state: the owner's Discord name, or "not linked" with the reason;
  - Link, Cancel and Unlink;
  - while a code is waiting, the code with its countdown.
- **Install and uninstall.** An update keeps `owner-link.bin`. Uninstalling with "Delete
  the application data" removes it.

Not in this change:
- Claude or any engine other than Codex;
- OAuth or any sign-in to Discord;
- limits on who may use the bot's commands;
- ending the session or leaving the channel when the owner leaves;
- several owners, allow-lists or roles;
- limits on who hears or reads the answers.

## Capabilities

### New Capabilities

- `owner-only-access`: linking the owner's Discord account with a one-time code, storing
  the link, when the link ends, and which voices the owner-only mode lets through to the
  assistant.

### Modified Capabilities

- `voice-channel-session`: speaker identity keeps the Discord account of every source, bots
  included, and the "only person in the channel" rule applies only to sources Discord never
  announced.
- `discord-connection`: `/link` is registered with `/join`, `/leave` and `/status`, and is
  also offered in a direct message to the bot.
- `app-settings`: the new setting `discord.owner_only`.
- `settings-ui`: the Access card. It is also added to the field list, the card order and
  the collapsible cards, and its switch to the list of switches.
- `codex-assistant`: the app-server also starts when a link is started from the window, and
  stops one minute later, as after Check.
- `app-distribution`: updates keep `owner-link.bin`, and uninstalling with the data removes
  it.

## Impact

- `crates/voicebro-core`:
  - A new `owner.rs`: the link record, its DPAPI file, the one-time code, the account
    fingerprint and the filter value the receive path reads.
  - `discord/speakers.rs`: the account and the bot flag per source; participants with their
    account ids.
  - `discord/receive.rs`: the owner filter before the mix, and the fallback rule.
  - `discord/handler.rs`: `/link` and the `/status` line.
  - `discord/session.rs`: participants with their account ids.
  - `discord/mod.rs`: the owner state in `Shared`, `VoiceHost::complete_link`, and the
    ignored-frames counter.
  - `runtime.rs`: starting, completing and removing a link; the account comparison on every
    account read.
  - `events.rs`, `status.rs`: the link state in the status and its event.
  - `settings/mod.rs`: `discord.owner_only`.
- `apps/desktop`:
  - commands to start, cancel and remove a link;
  - the Access card in `ui/index.html` and `ui/main.js`;
  - `ui-dev/mock.js` and `ui-dev/check-config-tab.mjs`;
  - `installer-hooks.nsh`.
- Docs: `docs/ARCHITECTURE.md` (3, 4.2, 4.6, 4.7, 4.8, 4.10 and a new 4.11),
  `docs/user-guide.md`, `README.md`, `AGENTS.md`.
- Dependencies: the `Win32_Security_Cryptography` feature of `windows-sys`. No new crate:
  `rand`, `sha2` and `chrono` are already in the core.
- A settings file from 1.0.1 loads with `owner_only` off. An earlier build keeps the new
  field as an unknown field.
