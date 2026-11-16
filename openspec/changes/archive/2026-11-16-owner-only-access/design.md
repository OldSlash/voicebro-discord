# Design

## Context

See proposal.md for the motivation. The state this design starts from:

- **Who is speaking.** `ReceiveHandler` (`discord/receive.rs`) gets a `SpeakingStateUpdate`
  with the source's `user_id` and turns it into a display name through `member_label`
  (`discord/speakers.rs`). It then drops the account id. `SpeakerDirectory` keeps only
  SSRC → name.
  - For a bot, `member_label` returns `None`, so the source stays unnamed. The "only person
    in the channel" fallback (`try_sole_person`) then gives the bot's stream the name of the
    only person in the channel.
  - Every `VoiceTick` mixes all speakers and hands the mix and the per-speaker frames to
    `VoiceHost::on_tick`, which reaches the gate and through it the wake feed and the
    assistant.
- **Commands.** `discord/handler.rs` registers `/join`, `/leave` and `/status` (globally,
  and in `discord.guild_id`) and handles the panel's Mute button. None of them checks who
  sent it, and this change keeps it that way.
- **Participants.** `refresh_participants` (`discord/session.rs`) reads the voice states of
  the bot's channel from the cache on every voice state update. It has the user ids but
  keeps only names.
- **The Codex account.** `account/read` (checked live with Codex CLI 0.167.0) gives `type`,
  `email` and `planType`. They are kept in `AccountInfo` and shown in
  `AssistantStatus.{account_type, email}`.
  - `Assistant::prepare` reads the account at every join.
  - Check (`CodexAssistant::check`) reads it on demand and stops the app-server a minute
    later.
- **What the app stores.** The app stores files in its folder (`settings.json` written
  atomically through a `.tmp` file). The Discord token is in the Credential Manager.
  `installer-hooks.nsh` removes the listed data on an uninstall with "Delete the
  application data". The core already has `rand` 0.9, `sha2` 0.11, `chrono` and
  `windows-sys` 0.61 (features `Win32_Foundation`, `Win32_Security_Credentials`).
- **serenity 0.12.5** (checked in its source): `CreateCommand::contexts(Vec<InteractionContext>)`
  with `InteractionContext::{Guild, BotDm, PrivateChannel}`. An interaction carries the
  sender as `user` in a server and in a DM alike.

## Goals / Non-Goals

**Goals:**

- One place that owns the owner state: the link, the waiting code and the filter value the
  receive path reads. `/link`, `/status`, the settings and the window only ask it.
- Filtering before the mix, so the gate, the wake engine and the assistant need no change
  to hear only the owner, and a later engine (Whisper and `claude -p`, discussed on
  2026-11-16) gets the same audio for free.
- Nothing rebuilt when the mode or the link changes: the receive path reads one atomic per
  tick, as with the gains.

**Non-Goals:**

- Limiting the bot's commands. `/join` brings the app's own code into a channel and anyone
  may run it (the user's decision of 2026-11-16). The mode limits only speaking.
- Reacting to the owner's presence: no session end and no leave when the owner leaves the
  channel. While they are away nothing passes the filter anyway.
- Protection against the same Windows user running modified code. The app is MIT and
  local. The aim is that the default use keeps to the terms, so getting around them takes
  a deliberate act.
- Moving the Discord token out of the Credential Manager.

## Decisions

### D1. One owner state in the core

A new module `owner.rs` holds:

- `OwnerLink { discord_user_id: u64, discord_name: String, assistant: String, account:
  String, linked_at: DateTime<Utc> }`. `assistant` is the engine id (`codex`); `account` is
  D4's account fingerprint.
- `PendingCode { code: [u8; 8], account: String, expires_at: Instant, wrong_tries: u8 }`.
  At most one code waits; pressing Link again replaces it.
- `OwnerState`, shared as an `Arc`:
  - the link and the waiting code under one `Mutex`;
  - the last reason a link or a code ended (shown in the window);
  - `filter: AtomicU64`, which the receive path and `/status` read:

    | Value | Meaning |
    |---|---|
    | `0` | mode off: everyone passes |
    | `u64::MAX` | mode on, no link: nobody passes |
    | any other value | mode on: only this Discord user id passes |

    Discord ids are snowflakes and never reach `u64::MAX`.

The runtime recomputes `filter` from `discord.owner_only` and the link on every settings
save, link, unlink and link removal. The gate host holds the same `Arc<OwnerState>` and
gives the Discord adapter the value through `VoiceHost::owner_filter`, so the adapter
needs no new constructor argument and its test hosts default to everyone (changed while
building, task 2.4: the design first put the `Arc` into `Shared`).

Alternatives considered:
- **The owner in `settings.json`.** It is a plain file anyone can edit to name another
  account, which the spec forbids ("The owner is set only by a code").
- **One atomic per guild.** The owner is one person for every server, so one value is
  enough.

### D2. The one-time code [decided alone: format, tries, DM]

- **Alphabet.** Crockford base32: `0-9` and `A-Z` without `I`, `L`, `O`, `U`. Eight
  characters give 32^8 = 2^40 codes. The window shows them as `XXXX-XXXX`.
- **Randomness.** The code comes from the OS random source (`rand::rngs::OsRng`).
- **Typed input.** It is normalized before the comparison:
  - upper case, spaces and dashes removed;
  - `O` read as `0`, `I` and `L` read as `1`, as Crockford decoding does.

  It is compared with the waiting code in constant time over the 8 bytes (an XOR fold; no
  new crate).
- **When a code ends.** It lives 10 minutes. 5 wrong codes in total, from anyone, end it.
  A code ends on success, expiry, Cancel or the fifth wrong try. Expiry is checked when
  `/link` arrives and by the window's countdown, with no timer task.
- **State.**

  ```text
               Link (account read)            /link right code
   no code  ------------------------>  waiting  ------------------> linked
      ^                                 |  |  |
      |   expired / Cancel / 5th wrong  |  |  | wrong code: tries + 1
      +---------------------------------+  |  +--> (stays waiting)
      ^                                    | Link again: new code
      +------------------------------------+
  ```

- **Why every wrong code counts, from anyone.** `/link` is visible to everyone in a server.
  With 5 tries, a guess succeeds with a chance of 5 in 2^40.
- **DMs.** `/link` works in a DM so the owner does not have to type a code in a public
  server. The reply is ephemeral anyway.

### D3. `/link` and the `/status` line

- **Registration.** `/link` gets one required string option `code` (length 8 to 12).
  Globally it is registered with `contexts([Guild, BotDm])`, and in the configured guild as
  the other commands are. `/join`, `/leave` and `/status` keep their registration as it is.
- **Handling.**
  - The handler passes `command.user.id`, the account's name (`global_name`, else
    `name`, cleaned by `clean_display_name`) and the typed code to
    `VoiceHost::complete_link`.
  - It answers with an ephemeral message from the returned outcome: linked, wrong code
    (tries left), expired, no code waiting, or ended after 5 wrong tries.
  - The account id is what Discord put in the interaction, so the sender cannot pick
    another account.
- **`/status`** adds the line "Only the owner's voice reaches the assistant here." while
  `filter` is not `0`.
- **Unchanged.** `/join`, `/leave` and the Mute button are not touched.

### D4. The account fingerprint and when a link ends [read from the user's reply of 2026-11-16: the binding acts on speaking only]

- **What the user asked.** Changing accounts means linking again, and the restriction is
  for speaking, not for `/join`. So a link is bound to both accounts, and a link that ends
  only stops the owner's voice from passing. It never refuses a join.
- **The fingerprint.**
  - The Discord side follows from storing the user id: another Discord account is not the
    owner until it links.
  - The ChatGPT side stores `account`, a SHA-256 hex digest of
    `"<account_type>:<email trimmed and lower-cased>"` from the account read when the code
    was issued. An API-key sign-in has no email, so its fingerprint is `"apiKey:"`. The
    email is not stored, only its digest, inside the encrypted file.
- **Where the account is compared.** The runtime compares the fingerprint of every
  signed-in `AssistantStatus` it receives (Check, Link, the read at start, a join's
  account read) with the link's.
  - A different fingerprint removes the link with the reason "the Codex account changed".
    `filter` becomes `u64::MAX` while the mode is on.
  - A signed-out status changes nothing: signing out is not another account.
- **At a join.** `GateHost::prepare_join` makes the same comparison right after
  `Assistant::prepare` succeeds. So the filter is set before the first frame of the new
  call, without waiting for the status event. The join goes on either way.
- **Remove, not suspend [decided alone].** A link removed for another account does not
  come back when the first account signs in again. The user said changing accounts means
  linking again.
- **Another Discord account linking.** A new link replaces an existing one without Unlink
  first. Only someone who sees the window can get a code, so this is the same person.
  The log says "owner changed from A to B".

Alternatives considered:
- **Matching the Discord account's email with the ChatGPT email.** It needs OAuth with the
  `email` scope, and many people use different emails. Rejected in the exploration of
  2026-11-16.
- **Refusing the join on a different account.** The user ruled out limits on `/join`.

### D5. Storage: a DPAPI-encrypted file in the app's folder

- **The file.** `owner-link.bin` in the data folder (next to `voicebro.exe`, or
  `VOICEBRO_DATA_DIR`).
  - Content: the JSON of `OwnerLink` with `"version": 1`, encrypted with
    `CryptProtectData`. The flags are `CRYPTPROTECT_UI_FORBIDDEN`; the optional entropy is
    the constant `b"VoiceBro owner link v1"`; there is no description.
  - Written to `owner-link.bin.tmp`, then renamed, as `settings.json` is.
  - Read once at start. Unlink deletes it.
- **Failures.** If `CryptUnprotectData` fails, or the JSON does not parse, or its version is
  unknown, the app has no link. It logs one line naming the file, and the window shows
  "not linked" with the reason "the link file could not be read". The file is not deleted
  automatically, so the user can still look at it; the next link overwrites it.
- **What DPAPI gives.** The blob opens only for the same Windows user on the same PC, and a
  changed byte fails its integrity check. The entropy keeps a program that calls DPAPI
  without it from opening the blob by chance. It is no secret: the source is public.
- **Windows API.** `windows-sys` needs the feature `Win32_Security_Cryptography` for
  `CryptProtectData`, `CryptUnprotectData` and `CRYPT_INTEGER_BLOB`. `LocalFree`, which
  frees the output blob, is in `Win32_Foundation` in 0.61.
  - **Checked** (task 2.1, windows-sys 0.61.2 source): `CryptProtectData`,
    `CryptUnprotectData` (crypt32.dll), `CRYPT_INTEGER_BLOB { cbData, pbData }` and
    `CRYPTPROTECT_UI_FORBIDDEN = 1` are in
    `windows_sys::Win32::Security::Cryptography`; `LocalFree` (kernel32.dll) is in
    `windows_sys::Win32::Foundation`.
- **Other platforms.** Off Windows, `owner.rs` keeps the link in memory only. The core's
  tests then run the logic without DPAPI, as `MemoryStore` does for secrets. A test on
  Windows writes and reads a real file in a temporary folder.

Alternatives considered:
- **The Credential Manager**, where the token is. It outlives the portable app's folder,
  which breaks "until the app is deleted".
- **AES or an HMAC with a key in the code.** The key would be public in the source, so it
  gives nothing over DPAPI and binds the file to neither the user nor the PC.

### D6. Speaker accounts and the filter in the receive path

- **The directory.** `SpeakerDirectory` keeps `SpeakerEntry { user: Option<u64>, bot: bool,
  name: Option<String> }` per SSRC.
  - The speaking handler records `user` for every event, bots included. It reads the bot
    flag from the member it already fetches, and records the name only for people.
    `member_label` becomes `member_info`, returning the name and the bot flag.
  - The account goes into the directory before the member lookup (`set_user`), and the
    name after it. The lookup can be an HTTP call, and without this the owner's first
    frames would be dropped as an unknown source while it runs (found while building).
  - The fallback runs only for a source with no entry at all, that is one Discord never
    announced; a bot's source has an entry.
  - When the fallback applies, it records the only person's account id with their name.
    `channel_people` returns `(UserId, name)` pairs and `VoiceSession.participants` keeps
    them; the gate still gets names only.
- **The filter.** In the `VoiceTick` branch, `filter` is loaded once per tick:
  - `0`: unchanged path.
  - Otherwise a speaker passes only if `directory.user(ssrc) == Some(filter)`.
    `u64::MAX` matches nobody.
  - The check runs before decoding with opus-rs too, so a dropped speaker costs no decode.
    With libopus songbird has already decoded it.
  - Dropped frames add to `VoiceCounters.ignored_frames`. The first drop of a source per
    join logs `receive: ignored <name or "an unknown source"> (ssrc N): not the owner`.
- **What the host sees.** The mix and the per-speaker list hold only passing speakers, so
  `VoiceHost::on_tick` sees silence from everyone else. `last_user_voice`, the wake feed,
  `speakers_changed` and the replay all follow without a change in `gate.rs`.
- **Locking.** The directory lookup is a short `Mutex` lock per speaker per tick, as the
  name lookup in `try_sole_person` already is. Nothing is held across an await.

Alternative considered: **filtering in the gate.** The gate knows SSRCs and names but not
accounts. Filtering there would also leave the mix built from everyone.

### D7. Window, status and commands

- **Status.** `Status.owner: OwnerStatus { linked: Option<{ name, id }>, pending: Option<{
  code, expires_at_ms, wrong_tries }>, ended: Option<String> }`. `discord.owner_only` comes
  with the settings already in the status. `AppEvent::Owner(OwnerStatus)` is published on
  every change, so the card follows within a second.
- **Where the code goes.** The code reaches only the webview, through the snapshot and the
  event. Task 2.4 checks that no log line prints `Status` or `AppEvent` values.
- **Tauri commands.**
  - `owner_link_start`: async. It runs Check's account read, then issues a code, and
    returns the status or the error "Codex is not signed in".
  - `owner_link_cancel`.
  - `owner_unlink`.
- **The Access card [decided alone].** A new card after "Discord and Codex", id `access`
  for `ui.collapsed_cards`. Its switch is saved on change like the other cards' fields.
  It is not part of the "Discord and Codex" card because that card waits for Save changes,
  and a switch there would behave unlike every other switch.
  - With the switch on and no link, the card says the assistant hears nobody until an
    account is linked.
  - Unlink acts at once, without confirmation [decided alone]. Undoing it takes one Link.
- **The countdown** is computed in the window from `expires_at_ms`. The form-state rule
  holds: the card's link part is status, not form, so a refresh may redraw it, but it never
  touches the switch while it has focus.
- **The mock** gets `?owner=linked`, `__mock.completeLink(name)`, `__mock.expireCode()`
  and `__mock.wrongCode()`.

### D8. Install and uninstall

`installer-hooks.nsh` adds `Delete "$INSTDIR\owner-link.bin"` and `owner-link.bin.tmp` to
the "delete the application data" branch. An update never touches the file, because the
NSIS template replaces only program files.

## Risks / Trade-offs

- **The same Windows user can still get around the link.** Code built from the public
  source can write a link file for any account, or the owner can read the code out to a
  friend. → Accepted (Non-Goals): the aim is that the default use keeps to the terms.
- **Wrong codes can end the owner's code.** Anyone in the server can send 5 wrong codes
  and end it. → The window says why, the owner presses Link again, or uses `/link` in a
  DM.
- **An unannounced owner source while others are in the channel.** If Discord never
  announces the owner's source while someone else is there, the owner is dropped as
  unknown. → It is logged once as an ignored unknown source and counted. A rejoin brings a
  fresh announcement. Announcements come with the first speech after the join, so this
  should be rare.
- **The call stays open in always mode without the owner.** If someone else runs `/join`
  while the owner is not in the channel, or the owner leaves, the always-mode call stays
  open with nothing passing. → Accepted (Non-Goals): nobody's request reaches the
  subscription. `/leave` or the Mute button from anyone closes it, as today.
- **DPAPI loses the link after a password reset.** An administrator resetting the Windows
  password (not the user changing it) makes the user's DPAPI data unreadable. → The link
  reads as unreadable, the window says so, and the user links again.
- **A global command in DMs can lag.** A new global command can take up to an hour to show
  in DMs. → In the configured guild `/link` appears at once, and the user guide says so.
- **The stored name gets stale.** The owner's Discord name is the one from the moment of
  linking. → It is shown only as a label; the id decides. A new link refreshes it.

## Migration Plan

- **Settings.** A settings file from 1.0.1 loads with `owner_only` off, so nothing changes
  until the user turns the mode on.
- **Rollback.** An older build keeps `owner_only` as an unknown field and never reads
  `owner-link.bin`, so every voice reaches the assistant again. Reinstalling this version
  brings the mode back with the same link.
