# Design

## Context

The owner-only mode (change `owner-only-access`) already filters voices per speaker before
the mix:

- Every 20 ms songbird hands the receive path (`discord/receive.rs`) each speaker's frame
  separately, keyed by SSRC. `SpeakerDirectory` maps an SSRC to a Discord account once
  Discord announces it.
- The path reads one value per tick, `VoiceHost::owner_filter() -> u64` (`0` everyone,
  `u64::MAX` nobody, else the owner's id), drops every source that does not pass, then
  mixes. A dropped source never reaches the wake engine, the gate, Whisper or the call.
- The "only person in the channel" rule (`try_sole_person`) assigns a source Discord never
  announced to the only non-bot member, then the filter decides.
- `OwnerState` (`owner.rs`) holds the link, its DPAPI file `owner-link.bin` and the filter
  atomic; `discord.owner_only` is a bool with its own deserializer.
- At activation the gate sends `call_context(caller, participants)`. The participants are
  every name in the voice channel (`discord/session.rs`, `refresh_participants`), in the
  owner-only mode too.
- The handler registers four commands (`/join`, `/leave`, `/status`, `/link`); `/link` is
  global with the Guild and BotDm contexts. Buttons are handled in `handle_component`,
  today only the panel's Mute.
- The settings keep unknown fields through `extra` (`#[serde(flatten)]`).

Nothing here is about Discord's own consent features: Discord has none for bots. The
operator's duties come from Discord's developer terms (privacy policy, a user's own
instruction to share their data with a third party); the change gives the tools and leaves
their use to the operator.

## Goals / Non-Goals

**Goals:**

- One access filter for three modes, read once per tick, replaced whole on every change.
- Consent that is the person's own act in Discord, recorded with what they agreed to, so
  the operator can show it when asked.
- Nothing rebuilt when the mode, the link or a record changes.
- The default stays as today: everyone is heard.

**Non-Goals:**

- One speaker at a time toward Codex (Part 2 of the task, its own change).
- Limits on the commands, on who hears the answers, or on who may bring the bot in.
- Messages to people other than replies to their own commands.
- Editing saved chat history files on a withdrawal.

## Decisions

### D1. One setting with three values [decided alone: replaces the switch]

`discord.access`: `everyone` | `agreed` | `owner`, default `everyone`.

- Two switches (owner-only and agreed-only) would allow a meaningless pair (both on); one
  list cannot.
- `DiscordSettings` gets `access: AccessMode`. The old `owner_only` stays a
  deserialize-only field (`Option<bool>`, `skip_serializing`), so `extra` does not keep it
  and the next save drops it.
- After loading: when `access` was absent, `owner_only == Some(true)` gives `Owner`,
  anything else `Everyone`. When both are present, `access` decides.
- An unknown value is rejected with the three names. The runtime's `changed_sections` keeps
  its `owner` part and adds `access` and `privacy_url` to it: a save of either rebuilds
  nothing, only the filter.

### D2. The consent records and their file [decided alone: format, clearing]

A new `consent.rs`, next to `owner.rs`:

- `ConsentRecord { discord_user_id: u64, discord_name: String, agreed_at: String (RFC 3339,
  UTC), text_version: u32, assistant: String }`.
- The file `consents.bin` holds `{"version": 1, "records": [...]}`, encrypted with the
  same DPAPI calls as the link. The entropy is `VoiceBro consents v1`. It is written
  through `consents.bin.tmp` and a rename, and read once at start.
- The DPAPI helpers move out of `owner.rs` into a small shared module (`protect.rs`), used
  by both. The owner link keeps its own entropy, so its file reads as before.
- A file that does not decrypt, parse or match the version gives no records and one
  warning. The status keeps the reason, and the window shows it. The next write replaces
  the file.
- At start, and whenever `assistant.engine` changes, records whose `text_version` or
  `assistant` differs from the current ones are deleted. The file is rewritten and one log
  line says how many and why.
- `ConsentState` holds the records behind a mutex, publishes `AppEvent::Consent` on every
  change, and exposes `ids() -> HashSet<u64>` for the filter.
- Off Windows the records are kept in memory only, like the link.

### D3. The consent text and its version

`consent.rs` holds `CONSENT_TEXT_VERSION: u32 = 1` and a function that builds the text
from the engine's provider name ("OpenAI, through Codex") and the policy URL. The text says
four things:

- after the activation phrase, their voice in the voice mode, or the text of their speech
  made on the operator's PC in the Whisper mode, is sent to the provider so the assistant
  can answer;
- their Discord id, name and the time they agreed are kept on the operator's PC until they
  withdraw;
- `/optout` withdraws at once, and what was said before is already sent;
- the policy is at `<url>`.

Every published version's text stays in the code, as a list by version, so that what a
record's `text_version` agreed to can always be shown. Changing the wording raises the
version, which clears the records (D2).

### D4. One filter for the receive path

`owner_filter() -> u64` becomes `voice_filter() -> Arc<VoiceFilter>`:

```text
enum VoiceFilter {
    Everyone,
    Only(HashSet<u64>),   // agreed: owner + records; owner: the owner alone; empty: nobody
}
```

- The runtime owns `RwLock<Arc<VoiceFilter>>`. It rebuilds it from the mode, the link and
  the records whenever any of them changes, and swaps the `Arc`.
- The receive path clones the `Arc` once per tick. No lock is held while mixing.
- `passes(filter, user)`: `Everyone` passes all; `Only(set)` passes `Some(id)` in the set.
  A source with no known account never passes `Only`.
- `try_sole_person` runs for every `Only` filter, as it ran for the owner filter.
- The once-per-source log line names the reason by mode: "not the owner" or "has not run
  /optin".
- The ignored-frames counter is shared by both modes.
- The owner is in the set in the `agreed` mode without a record [decided alone: the
  operator links themself and needs no `/optin`].
- [decided alone while building] The filter lives in a new `access.rs` (`AccessState`), not
  in the runtime: it owns the mode and rebuilds from `OwnerState` and `ConsentState`, which
  each call a listener after a change. The owner's listener runs under the owner's lock, so
  `OwnerState` gained a lock-free `linked_id()` (an `AtomicU64`) for it; the consent
  state calls its listener after releasing its own lock. `AccessState` also has observers,
  run after each rebuild; the gate host uses one for D6.

### D5. The commands and the two buttons

- `command_definitions` adds `/optin`, `/optout` and `/privacy`, none with options.
  Globally they get the Guild and BotDm contexts, like `/link`. `/optin`: "Let VoiceBro's
  assistant hear you (your speech goes to OpenAI after the wake phrase)". `/optout`: "Stop
  VoiceBro's assistant hearing you and delete your consent". `/privacy`: "Show this bot's
  privacy policy".
- `/optin` [decided alone: refused without a policy URL]:
  - no URL: an ephemeral note that the operator has not published a privacy policy;
  - a record for the current version and engine: an ephemeral note with its time and
    `/optout`;
  - otherwise: an ephemeral message with the text of D3 and two buttons, custom ids
    `vb:optin:agree:<version>` and `vb:optin:cancel`.
- `handle_component` parses these ids next to the Mute button's:
  - Agree with the current version records `component.user`: the id, the cleaned display
    name as `/link` does it, and the time now. Then it updates the ephemeral message to
    the confirmation without buttons. The confirmation names the mode when it is not
    `agreed`.
  - Agree with an older version records nothing and asks for `/optin` again.
  - Cancel updates the message to "Nothing changed".
  - An ephemeral message is seen and pressed only by the person who ran the command, so
    the presser is the person who agrees.
- `/optout`: deletes the sender's record and replies ephemerally, as the spec says.
- `/privacy`: an ephemeral reply with the URL or the note.
- `/status`: `status_text` takes the access mode instead of `owner_only`. The `owner` line
  stays; `agreed` adds "Only the owner and people who ran /optin are heard here. See
  /privacy."
- `/join`: the reply text gets the same line in the `agreed` mode.
- Log lines: "consent: <name> agreed (text v<n>)", "consent: <name> withdrew",
  "consent: <name> removed in the window", and the clearing line of D2. No other personal
  data.

### D6. Names follow the filter

`refresh_participants` passes the people (account id, name) to the host. The host keeps
only those the current filter lets through before `set_participants`. Each filter change
re-applies it to every joined guild, so a later `/optin` adds the name for the next
activation. In `Everyone` nothing changes. [decided alone while building:
`VoiceHost::set_participants` takes the people with their ids; the gate host keeps them per
guild until the leave and re-applies them from its `AccessState` observer. The Discord
panel and the window's status still list everyone in the channel: they are not sent to the
assistant.]

### D7. Window, status and the mock

- `Status.consent`: the records as `{ id (string), name, agreed_at }` and the unreadable
  reason. `AppEvent::Consent` carries the same. The Tauri command `consent_remove(id)`
  deletes one record and returns the new list.
- The Access card:
  - the switch becomes a select "Who the assistant hears" bound to `discord.access` (saved
    on change);
  - a text field "Privacy policy URL" bound to `discord.privacy_url`, saved on change. The
    settings' own validation rejects anything but `https://` with a host, and the field
    shows its message; the window has no second copy of the check [decided alone while
    building: one validator, as for every other field];
  - the link block as it is;
  - the list "People who agreed" with a Remove per row, drawn from the status. The form
    never writes it.
  - Warnings: `owner` without a link (as today), and `agreed` with an empty URL.
- The mock:
  - `?access=agreed|owner`, `?privacy=<url>`, and `?consents=<name,name>` for the stored
    state;
  - `__mock.optin(name)` and `__mock.optout(id)` act as Discord would;
  - `__mock.consentUnreadable()` simulates the unreadable file.

  `check-config-tab.mjs` gets the card's scenarios.
- `installer-hooks.nsh` deletes `consents.bin` with the data and keeps it on update.

### D8. The privacy policy template

`docs/PRIVACY-POLICY-TEMPLATE.md` loses its "Status" note and gains:

- a paragraph to keep per access mode: everyone heard after the phrase; only people who
  ran `/optin`; only the owner;
- the note that a voice an agreeing person's microphone picks up passes with theirs;
- the consent text of version 1, quoted, so the operator can publish what people agree to.

The user guide and the README point operators to the template and to the Access card.

## Risks / Trade-offs

- **Global commands take up to an hour to appear.** The guild copy appears at once when
  `discord.guild_id` is set, as today. The user guide says so.
- **A voice in an agreeing person's microphone passes.** The filter works on Discord's
  streams, not on voices. The template tells people.
- **Songbird still receives and decrypts everyone.** In the libopus mode it also decodes
  them. The frames of people who do not pass live one tick in memory and are dropped
  unread. This is the same as the owner-only mode today.
- **A name changes in Discord after agreeing.** The record keeps the old name as a label;
  the id decides, as with the link.
- **The settings file of this build in an older build.** It has no `owner_only`, so an
  older build hears everyone. Installers are built only from tags, so a downgrade is a
  deliberate act; the release notes say it.

## Migration Plan

- First start of this build: `access` comes from `owner_only` (D1). No `consents.bin`
  exists, so no records. The next save writes `access` only.
- The version goes to 1.2.0: the settings file changes shape.
- Rollback: install the previous tag. `consents.bin` is ignored there and the access mode
  falls back to everyone (Risks).
