# Proposal

## Why

Whoever runs VoiceBro runs their own Discord bot, and Discord's developer terms make that
person responsible for it. To share people's speech with a third party they need each
person's own instruction, and every bot needs a privacy policy that people can reach from
the bot. Today the operator has only two choices: everyone in the channel is heard, or only
the linked owner. There is no way for friends to agree, withdraw or read a policy. The user
wants VoiceBro to give operators these options ready-made (the user's request of
2026-11-18). Using them stays the operator's choice, as the owner link is today. The task
is described in `docs/TASK-CONSENT-AND-TURNS.md`, Part 1.

## What Changes

- **Who the assistant hears: one setting with three values.** `discord.access`:
  - `everyone` (the default): as today without owner-only;
  - `agreed`: the linked owner and every person who ran `/optin` and agreed;
  - `owner`: today's owner-only mode.

  The Access card's switch "Only my Discord account" becomes a list with these three
  choices. A change applies from the next voice frame, as the switch does now.
- **BREAKING (settings file):** `discord.owner_only` is replaced by `discord.access`. A file
  with `owner_only: true` loads as `owner`, one with `false` or without it as `everyone`. The
  next save writes `access` and drops `owner_only`. A build before this change reading the
  new file finds no `owner_only` and hears everyone.
- **`/optin`.** Replies only to the sender with the consent text, a link to the operator's
  privacy policy and two buttons, "I agree" and "Cancel". The text says what is sent after
  the activation phrase, to whom and how to withdraw. "I agree" records the person. Without
  a privacy policy URL in the settings, `/optin` records nothing and says the operator has
  not published a policy.
- **`/optout`.** Deletes the sender's record. From the next voice frame their voice no
  longer passes in the `agreed` mode.
- **`/privacy`.** Replies only to the sender with the policy URL, or says none is published.
- **The consent records.** One record per person: Discord id, name, time, the consent text's
  version and the assistant engine. They are kept in `consents.bin` in the app's folder,
  encrypted for the Windows user like the owner link, never in `settings.json`. All records
  are dropped when the consent text's version or the engine changes, since the consent
  names who receives the speech.
- **The privacy policy URL.** A new setting, `discord.privacy_url`: empty or an `https://`
  URL, edited in the Access card.
- **The Access card lists the people who agreed,** with the time and a Remove button that
  acts like their `/optout`. While the mode is `agreed` and the URL is empty, it warns
  that `/optin` is refused.
- **Names sent to the assistant follow the mode.** In `agreed` and `owner` only the names
  of people whose voice passes are sent as participants. Today every name in the channel is
  sent, in the owner-only mode as well.
- **`/join` and `/status` name the `agreed` mode** and point to `/privacy`.
- **The privacy policy template** `docs/PRIVACY-POLICY-TEMPLATE.md` (written before this
  change, untracked) becomes part of it. It gets a paragraph for each access mode and the
  note that a voice picked up by an agreeing person's microphone passes with theirs.

Not in this change:
- one speaker at a time toward Codex (Part 2 of the task, its own change);
- limits on who may use `/join`, `/leave`, `/status` or Mute;
- deleting a person's lines from saved chat history files: the template tells the operator
  to do it on request;
- an export of the records: the Access card shows them.

## Capabilities

### New Capabilities

- `speaker-consent`: `/optin`, `/optout` and `/privacy`, the consent records and their
  file, the voices and names the `agreed` mode lets through, and its lines in `/join` and
  `/status`.

### Modified Capabilities

- `owner-only-access`: the owner-only mode is now the `owner` value of the access mode,
  and switching between the three values applies at once.
- `app-settings`: `discord.owner_only` is replaced by `discord.access`; new
  `discord.privacy_url`.
- `settings-ui`: the Access card holds the access mode list, the privacy policy URL and the
  list of people who agreed; "Only my Discord account" leaves the list of switches.
- `discord-connection`: `/optin`, `/optout` and `/privacy` are registered with the other
  commands and are offered in a direct message to the bot.
- `assistant-session-gate`: the participant names in the call context follow the access
  mode.
- `app-distribution`: updates keep `consents.bin`, and uninstalling with the data removes
  it.

## Impact

- `crates/voicebro-core`:
  - a new `consent.rs`: the records, their DPAPI file and the consent text with its version;
  - `owner.rs`: the DPAPI helpers shared with `consent.rs`; the mode read from
    `discord.access`;
  - one access filter for the three modes, read by `discord/receive.rs` once per tick in
    place of the owner's single id;
  - `discord/handler.rs`: the three commands, the two buttons and the `/join` and `/status`
    lines;
  - `discord/session.rs`: participant names filtered by the mode;
  - `runtime.rs`, `events.rs`, `status.rs`: the records in the status and their event;
  - `settings/mod.rs`: `discord.access` with its migration, `discord.privacy_url`.
- `apps/desktop`: commands to list and remove records; the Access card in `ui/index.html`
  and `ui/main.js`; `ui-dev/mock.js` and `ui-dev/check-config-tab.mjs`;
  `installer-hooks.nsh`.
- Docs: `docs/ARCHITECTURE.md` (4.2, 4.6, 4.11 and a new section for consent),
  `docs/user-guide.md`, `docs/PRIVACY-POLICY-TEMPLATE.md`, `README.md`, `AGENTS.md`.
- Dependencies: none new.
