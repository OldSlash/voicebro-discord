# Task: consent, privacy and one speaker at a time

Status: a task description, not started (2026-11-18). It replaces the earlier draft
`TASK-PER-SPEAKER-TURNS.md`. It describes behavior and how the parts of the app talk to
each other, not code. Facts about the current code were read from it; the points marked
**probe** are not checked yet.

## Goals

1. **Options for the operator, not duties.** Whoever runs their own bot can choose to hear
   only people who agreed, publish a privacy policy and point people to it. Nothing of this
   is forced: like the owner link, it is a setting, and using it is the operator's
   responsibility. VoiceBro gives the tools so that nobody has to build them.
2. **Consent:** `/optin` to agree, `/optout` to withdraw. The app remembers who agreed;
   `/optout` deletes that person's record.
3. **Privacy:** `/privacy` shows the operator's policy; a template for it ships in
   `docs/PRIVACY-POLICY-TEMPLATE.md`.
4. **One speaker at a time toward Codex,** in both modes: people's speech reaches Codex one
   person after another, each labeled with their name, never mixed.

No new engine, no paid external service, no add-on: everything stays on Codex with the
user's existing sign-in.

## The parts involved

| Part | Today | In this task |
|---|---|---|
| Discord gateway (serenity) | `/join`, `/leave`, `/status`, `/link`, the Mute button | adds `/optin`, `/optout`, `/privacy` and the two buttons of `/optin` |
| Voice receive (songbird → `discord/receive.rs`) | every 20 ms each speaker's frame separately; drops non-owners in owner-only mode, then mixes | drops by the access mode, then hands each speaker's frame on |
| Access state (`owner.rs` + a new consent store) | owner link, one-user filter | owner link plus the list of people who agreed; one filter for the three modes |
| Gate (`gate.rs`) | wake word per speaker; in the voice mode sends the mix to the call | in the voice mode picks one speaker at a time for the call and buffers the others |
| Codex session, voice mode (`codex/session.rs`) | WebRTC call; speaker notes through `thread/realtime/appendText` | a note at every change of speaker |
| Codex session, Whisper mode (`codex/text_session.rs`) | each finished utterance is a text turn; a new one interrupts the running answer | a new utterance joins the running turn or waits for the next one; nothing is interrupted |
| Window (Config → Access) | owner-only switch, link | access mode, privacy policy URL, the list of people who agreed |
| Files in the app's folder | `settings.json`, `owner-link.bin` | adds `consents.bin` |

## Part 1: access, consent and privacy

### Access mode

One setting, "Who the assistant hears", with three values:

- **Everyone** (default): as today without owner-only.
- **People who agreed**: the linked owner and everyone with a consent record.
- **Only my Discord account**: as today's owner-only mode.

A settings file with today's `discord.owner_only: true` loads as "Only my Discord
account". A change applies from the next 20 ms frame, without a rejoin, as the owner-only
switch does now.

### The consent store (`consents.bin`)

- One record per person: Discord user id, display name, the time they agreed, the version
  of the consent text, the assistant engine.
- Encrypted for the Windows user like `owner-link.bin`; never in `settings.json`.
- A record is deleted by `/optout` or by Remove in the window. All records are dropped when
  the consent text's version or the engine changes, because the consent names who receives
  the speech; people then run `/optin` again.

### `/optin`

1. A person runs `/optin` in a server with the bot or in a direct message to it.
2. The bot checks the privacy policy URL in the settings. Without one it replies that the
   operator has not published a policy yet and records nothing.
3. Otherwise it replies only to that person with the consent text: what is sent after the
   activation phrase (the voice in the voice mode, the text in the Whisper mode), to whom
   (OpenAI, through Codex), what is stored, how to withdraw, and the policy link. Below it
   are two buttons: "I agree" and "Cancel".
4. "I agree" adds the record, rewrites `consents.bin`, rebuilds the filter and tells the
   window. From the next frame that person's voice passes in "People who agreed". The
   reply says so and, when the mode is another one, says which mode is on.
5. "Cancel" changes nothing.

### `/optout` and Remove

The record is deleted, the file is rewritten, the filter is rebuilt and the window is
told. From the next frame the person's voice is dropped in "People who agreed". What they
said before is already sent and cannot be recalled; the reply says so. A person with no
record gets a reply that there was nothing to remove.

### `/privacy`

Replies only to the sender with the policy URL, or with a note that none is published.
The URL is a setting in the Access card; the window warns while "People who agreed" is on
and the URL is empty.

### What else follows the mode

- **The voice filter.** Every 20 ms the receive path takes the current filter (a snapshot
  that is replaced whole on each change) and drops the frames of every speaker it does not
  let through, before anything listens to them. The "only person in the channel" rule for
  sources Discord never announced works for all three modes.
- **Names sent to Codex.** At activation the gate sends the names of the people in the
  channel. In the two limited modes it sends only the names the filter lets through. Today
  it sends every name, owner-only mode included.
- **`/join` and `/status`.** In "People who agreed" both say that only people who ran
  `/optin` are heard and point to `/privacy`. The owner-only line stays as it is.
- **The template.** `docs/PRIVACY-POLICY-TEMPLATE.md` gets a paragraph per access mode, and
  a note that a voice picked up by an agreeing person's microphone reaches the assistant
  with theirs.

## Part 2: one speaker at a time toward Codex

### Voice mode (the WebRTC call)

The call has one audio track toward Codex, paced in real time.

1. **The floor.** The first person to speak in a session has the floor: their frames go to
   the call live, as now, with no delay.
2. **Others wait in a buffer.** Whoever speaks while someone else has the floor is not
   mixed in. Their frames are kept in their own buffer, up to 15 s per person; anything
   beyond that is dropped and logged.
3. **Passing the floor.** When the speaker with the floor is silent for about 300 ms, the
   floor goes to the person who has waited longest. Before their audio the session sends a
   note "Now speaking: <name>" through `thread/realtime/appendText` (developer role, which
   adds context and does not start a reply; it already works with a ChatGPT sign-in). Then
   their buffer goes to the call at the normal pace; if they are still talking, their live
   speech follows the buffer until they pause.
4. **Alone in the session:** nothing changes. Only overlapped speech is delayed, by the
   length of the overlap.

**Probe** before building: when does the model end a turn on silence? Our floor pause has
to be shorter than its pause, or the model answers the first person before the second
one's buffer arrives. Also check that the note lands between the two voices. Both are
measured with the live call test fed two overlapping speakers.

### Whisper mode (Codex text turns)

Each person's utterances are already cut separately and turned into text on this PC. What
changes is what happens when an utterance arrives while Codex is busy:

- **No turn is running and the voice is quiet:** `turn/start` with "<name>: <text>", as now.
- **A turn is running:** the utterance joins it through `turn/steer` (in the app-server
  schema; **probe** how Codex answers). If that turn has just ended, it becomes the next
  turn instead.
- **The answer is being spoken:** the utterance waits and starts the next turn. Its answer
  is spoken after the current one ends.
- Nothing interrupts a running answer any more. The deactivation phrase, Mute and `/leave`
  still stop it at once.

## Tests

- Unit tests: the consent store (encrypted round trip, delete, clear on a version change),
  the filter in the three modes, the floor and the buffers on recorded per-speaker frames,
  the Whisper queue and steer decisions against the fake Codex.
- The window checks in headless Edge: the Access card in each mode, the URL warning, the
  list and Remove. The mock gets an agreed-people list and a way to simulate
  `/optin` and `/optout`.
- Live: the call test with two overlapping speakers (the probes above), a Whisper text turn
  with two overlapping questions.
- In a real Discord channel with two people and a game running: `/optin`, `/optout`,
  `/privacy`, each mode, two people talking over each other in both modes. Nothing counts
  as verified before that.

## Order

Two OpenSpec changes, independent of each other: `voice-consent` (Part 1, its folder is
created and empty) and a second one for Part 2. Part 1 goes first: it needs no probes.

## Not in this task

- Who may use `/join`, `/leave`, `/status` and Mute: open to everyone, as now.
- Another engine, the API-key paths and the Realtime API.
- Deleting a person's lines from saved chat history files; the policy template tells the
  operator to handle that on request.
