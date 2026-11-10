# Next changes after the first real-channel test (2026-11-09)

The user ran build `0.2.0+19bcf29` in the test guild on 2026-11-09 between 21:45 and 21:51,
then the first version (GPTVoice, `D:\SideProjects\gptvoice-discord`, branch
`feature/codex-voice-assistant`) at 21:55 for comparison. The verdict: the voice works badly
(the assistant misunderstands, answers mid-sentence, speaks with stutters), the settings form
loses what is chosen, and the window looks poor. The first version is the reference for the
voice quality and for the UI.

This file lists the changes that follow, with the evidence for each. Each becomes an OpenSpec
change (or a part of one); nothing here is implemented yet.

Evidence used below:

- `target\release\logs\voicebro.log` and `target\release\logs\codex-chat-2026-11-09.log`
  (this app, the user's session).
- `%LOCALAPPDATA%\GPTVoice\logs\codex-chat-2026-11-08.log` and `codex-chat-2026-11-09.log`
  (the first version), `%LOCALAPPDATA%\GPTVoice\config.json` (its settings).
- The first version's sources: `src-tauri/src/audio.rs`, `src-tauri/src/codex.rs`,
  `src/browser/media-bridge.js`, `tauri-ui/*`.
- Six screenshots of the first version's window (Codex chat, Console, Status, System,
  Settings, Config) and two of this app (Chat, Settings).

---

## 1. Voice path: what the assistant hears and how it sounds

### Evidence

| Measure | First version | This app |
|---|---|---|
| Average user transcript length | 116 bytes (298 turns, 2026-11-08) | 72 bytes (10 turns) |
| Delegated tasks / web searches by the agent | 99 / 48 (2026-11-08); the 21:55 test searched and answered right | 6 / 0 |
| Input gain (Discord to the assistant) | 3.39 (user setting) | none (1.0) |
| Output gain (assistant to Discord) | 2.52 (user setting) | none (1.0) |
| Voice actually used | the server default (cedar is not available with a ChatGPT sign-in, so v1 sent no voice) | juniper |

In this app the user's sentences arrived cut into pieces and the model answered the pieces
(a one-word "uh-huh" or "yes" after half a sentence such as "and I would like... the plane
lies"). The agent never searched and answered
from memory, confusing The Forest with Sons of the Forest; the first version searched
("Sons of the Forest rope gun location") and answered correctly. Vosk's own transcripts of
the same Discord audio (`[wake] heard:` lines) cover the full sentences, so the audio from
Discord was complete; it was lost after the gate.

### Causes found in the code

1. **The send path drops speech.** `crates/voicebro-core/src/assistant/codex/rtc.rs`,
   `send_loop` (line 393): a thread on its own 20 ms clock takes only the newest frame from
   the queue and discards the older ones; when no frame has arrived yet it sends nothing.
   The gate's frames come from songbird's tick on a tokio task, which jitters (p99 about
   32 ms on Windows), so every tick with two frames loses one and every tick with none
   leaves a hole. The first version fed the call through an AudioWorklet FIFO
   (`GptVoiceInputProcessor` in `media-bridge.js`): every frame played in order, up to
   80 ms queued, silence only on underrun.
2. **The receive path has no jitter buffer.** `read_remote_track` (line 423) decodes RTP
   packets in arrival order and pushes them straight into the 160 ms output queue: no
   reordering by sequence number, no loss concealment (Opus FEC or PLC), and bursts
   overflow the queue and drop frames. In the first version the browser's WebRTC stack
   (NetEQ) did all of this and the output reached Discord as a steady 20 ms stream.
3. **No gains.** The first version multiplied the audio toward the assistant by the input
   gain and the audio toward Discord by the output gain (`audio.rs` lines 657 and 1194,
   clamped to 0..4). The user runs 3.39 and 2.52; this app sends the assistant audio about
   3.4 times quieter and plays the answer about 2.5 times quieter.
4. **The agent does not search.** This app opens the thread with `approvalPolicy: never`;
   the first version used `approvalPolicy: untrusted` (`codex.rs`, `thread_start_params`,
   line 1906). Everything else is the same: `sandbox: read-only`, `ephemeral: true`,
   `--disable shell_tool`, no web-search setting anywhere; the first version's agent searched
   with these parameters. The user's decision: web search is allowed and the model is not to
   be restricted.
5. **Voice.** This app sends `juniper`; the first version effectively used the server
   default.
6. **Secondary.** The Opus encoder runs mono VOIP at complexity 5 with the default bitrate;
   the browser's encoder is configured higher. The log carries about 700
   `RTCP decryption failed` warnings in 2.5 minutes from songbird and 49
   `opus_decode InvalidPacket` errors in the second after joining (before the DAVE key is
   in place).

### What to do

- Rewrite the send path: a FIFO of the gate's frames, every frame sent in order, a small
  prefill (40 to 60 ms), silence on underrun, continuous RTP timestamps.
- Add a receive jitter buffer: order by sequence and timestamp, conceal gaps with Opus FEC
  (next packet) or PLC, and feed the output queue at a steady 20 ms from a dedicated thread.
- The input and output gains: see change 7 (a required change of its own; it ships
  together with this one).
- Copy the first version's thread parameters (`approvalPolicy: untrusted`); web search
  stays on and nothing restricts the agent. Verify with a live delegated question that
  needs current facts: the chat log shows a web search item.
- Make the voice choice match the first version's behaviour (or show the default voice as a
  choice), and log the voice actually used.
- Raise the encoder settings toward the browser's (bitrate, complexity) after the two fixes
  above, measured, not guessed.
- Decide whether the RTCP warnings and the decode errors after joining are harmless; filter
  them from the log if so.
- Write the call's counters to the log when a call ends: frames sent, packets received,
  late, lost, concealed, underruns, queue drops.

### Verification

- A local end-to-end test without Discord: feed a WAV through ticks with Discord-like jitter
  into the engine, record the answer; no frame of the question lost, the transcript equal to
  the spoken text, the answer audio without gaps (counters at zero loss on a clean network).
- The same question to this app and to the first version; transcript length and the
  agent's tool use compared.
- The real-channel test (the user's step).

---

## 2. Settings form: values reset and saving is unclear

### Evidence and causes

- Every 5 s (`setInterval(refresh, 5000)` in `apps/desktop/ui/main.js`) and after every app
  event (Discord, wake, gate, hotkey, settings) the whole form is rewritten from the stored
  settings. The wake engine and model lists are rebuilt from scratch each time
  (`applyWakeLists`), which resets the selection and closes an open list; the Codex model,
  effort and voice lists are rebuilt on every assistant event (`loadLists`). The only guard
  is "skip the focused field", and for the Codex lists it guards the hidden text input, not
  the visible list.
- There is no "changed, not saved" state. Checkboxes save at once, other fields only with
  their own Save button; the "Saved." note stays forever.

### What to do

- Separate the form from the status: the timer and the events refresh only status fields;
  the form is filled once and after a save, and lists are built once.
- One saving model for every field, as in the first version: settings save automatically
  when changed (the first version's note: "Audio and browser settings save automatically
  when changed"); the Config tab saves with one "Save changes" button and "Reset". Each
  field shows saving, saved or the validation error.

### Verification

- A mock of the Tauri commands (`ui/dev/`) so the UI opens in a browser outside the app;
  the agent checks it there: a choice survives 30 s of events, an invalid value shows its
  message, a saved value survives a restart.

---

## 3. Window layout: the same interface as the first version

### Target

The first version's window, as its screenshots show: a tab bar with
**Console, Codex chat, Status, Performance, System, Settings, Config**, cards with
two-column form grids, and a status bar at the bottom with the connection state, guild
count, voice channel count, uptime and memory.

- **Status:** "Connected guilds" on the left; a grid of fact tiles on the right (Discord,
  bot account, voice, Discord channel, assistant engine, wake word, mute, Codex account,
  call state, hotkey).
- **System:** Application (version, process id, uptime, runtime), Operating system
  (platform, architecture, CPU cores, memory), Native host memory meter (Rust process and
  WebView2).
- **Settings:** cards Appearance (theme, font size), Audio (input and output gain sliders
  with percentages, earcons, earcon volume), Assistant (engine, activation mode,
  activation and deactivation phrases with the vocabulary note), Codex (model, reasoning
  effort, voice, instructions, speaker labels), Hotkey.
- **Config:** Discord bot token (masked, with the help text for getting one), guild id with
  the help text for copying it, auto-connect, Codex executable; "Save changes", "Reset", and
  the stored configuration shown read-only with its file location.
- Left out: everything about the ChatGPT browser (browser executable, ChatGPT URL, browser
  profile, browser status, audio capture mode); this app has no browser.
- All strings in English.

### How

- Write the first version's look (structure, styles, themes) anew and wire it to this app's
  commands and status snapshot. Plain HTML, CSS and JS as in the first version; no
  framework.
- Install the `frontend-design` skill from `anthropics/skills` locally (both
  `.agents/skills/` and `.claude/skills/`, per `AI-REFERENCE.md`) for the layout work.
- No Playwright. The UI is checked through the Tauri-command mock from change 2, opened in a
  browser.

### Verification

- Side-by-side screenshots of every tab against the first version's; the user's review.

---

## 4. Codex chat and Console tabs as in the first version

### Evidence

This app's Chat tab stayed empty during the call and the chat entries were printed as raw
JSON into the log pane. Cause: `ChatEntry` has a field `kind`, and the event enum uses `kind`
as its tag (`#[serde(tag = "kind")]` in `events.rs`); the webview receives `kind: "agent"`
instead of `kind: "chat"` and falls into the default branch.

### Target

- **Codex chat** as in the first version: toolbar Copy, Open log folder, filters "Names and
  notes", "Codex agent work", "Call events", "Auto-scroll"; one row per entry with time,
  a coloured label and the text: `Call` (call events), `Context → GPT` (names and notes sent
  to the voice model), `Task → Codex` (delegated task), the speaker's name (user
  transcript), `GPT` (voice answer), `Codex tool` (agent tool use, for example web search),
  `Codex` (agent message). Rows of the voice model and of delegated tasks highlighted.
- **Console** as in the first version: the app log with Clear, Copy, Copy selection, search
  with a match count, auto-scroll.

### What to do

- Fix the tag collision (rename the event tag or the entry field) and add a test that every
  event serializes with a unique `kind`.
- Build the two tabs: the first version's chat view (the owner's code) and a new console.

---

## 5. Performance tab and statistics

### Target

The first version's Performance tab: a time window selector (30 s, 60 s, 5 min), Refresh,
a health dot with a quality label, two live charts ("Audio timing", "Media packets") and stat
cards. And its per-turn latency report in the log, for example
`[latency:<guild>] turn 4 (response): first Discord speech -> ChatGPT audio 4343 ms`,
including turns where speech interrupted the assistant.

### What to do

- Measure each turn's latency as the first version does: per turn, first and last speech toward the
  assistant to the first answer audio, interruptions; logged and kept for the tab.
- Stat cards from this app's own counters instead of the browser ones: Discord ticks and
  speaker frames, frames sent to the call, packets received, late, lost and concealed
  packets, jitter buffer depth, output queue drops and underruns, wake engine drops, call
  state, Codex process, memory of the Rust process and WebView2.
- Charts over the chosen window from the same counters, sampled once a second.

---

## 6. Memory: measured with the wrong metric

### Evidence

Measured on 2026-11-09 at 22:02 with both apps running, wake word loaded, same machine:

| Process | Private bytes (commit) | Working set | Private working set (Task Manager "Memory") |
|---|---|---|---|
| First version `gptvoice-tauri.exe` (release) | 1213 MB | 219 MB | 179 MB |
| This app `voicebro.exe` (release 0.2.0) | 689 MB | 195 MB | 169 MB |

The 434 MB in `docs/MVP-PLAN.md` ("Measurements") was private bytes of a debug build and
the conclusion written there ("well above the 150–200 MB the stack decision expected") is
wrong: by the same metric as Task Manager this app uses slightly less than the first
version. There is no leak to fix from this data.

### What to do

- `scripts\measure.ps1` reports the private working set (what Task Manager shows) next to
  private bytes, and measures the release build only.
- Correct the measurement table and the conclusion in `docs/MVP-PLAN.md`.
- Show the memory of the Rust process and WebView2 in the status bar and the System tab, as
  the first version does.
- Measure again during a call (the user's step, with the new Performance tab).

---

## 7. Audio settings: how loud the assistant hears and how loud it speaks (required)

### Why

The first version has two volume controls on its Settings tab, card "Audio": "Mic to
ChatGPT" (the user runs 339 %) and "ChatGPT to Discord" (252 %). This app has neither, so the
assistant hears the channel about 3.4 times quieter than in the first version and its answer
plays about 2.5 times quieter. The user requires both settings.

### What the first version does

- `src-tauri/src/audio.rs`: `input_gain` multiplies every sample sent toward the assistant
  (line 657), `output_gain` every sample played into Discord (line 1194); both are clamped to
  0..4 and stored as atomics, so a change applies at once without reconnecting.
- `src-tauri/src/config.rs`: `AUDIO_INPUT_VOLUME`, `AUDIO_OUTPUT_VOLUME`, default 1.0.
- The first version's Settings: range sliders 0 to 4 with step 0.01, the
  label shows the value times 100 as a percentage, the setting saves automatically on change.

### What to do

- Settings: a new section `audio` with `input_gain` (what the assistant hears: Discord to
  the assistant) and `output_gain` (how loud the assistant speaks: the assistant to
  Discord), each a multiplier 0.0 to 4.0, default 1.0, validated in `Settings::validate`.
  The earcon switch and volume stay in the `session` section (no schema migration); the
  Audio card shows them next to the gains, as on the first version's card.
- Input gain: applied to the frames the gate sends to the assistant (after the mix, before
  the call), per sample, clamped to the i16 range. The wake engine keeps the unscaled audio,
  as in the first version.
- Output gain: applied to the assistant's audio on its way to Discord, per sample, clamped.
  The earcons keep their own volume and are not scaled by it.
- Both apply live: a saved change reaches the running call within one tick, no rejoin.
- UI: an "Audio" card on the Settings tab with two sliders ("What the assistant hears",
  "How loud the assistant speaks"), each with its percentage, plus the earcon switch and
  volume; saved automatically on change, as in the first version.
- Statistics (with change 5): the peak level toward the assistant and toward Discord over
  the last second, and a counter of samples clipped by the gain, so the user can tell when
  a gain is too high.
- Docs: `docs/ARCHITECTURE.md` (settings schema, audio path), `AGENTS.md` if a command or a
  fact changes.

### Verification

- Unit tests: a known frame times 2.0 gives twice the samples, values past the i16 range
  clamp and count as clipped, 0.0 gives silence, 1.0 leaves the frame unchanged; the wake
  feed stays unscaled.
- In the running app: moving a slider changes the level within a second during a call
  (the user's check in the channel); the saved value survives a restart.

---

## 8. Port the technical decisions the rewrite missed (required)

### Why

Compared on 2026-11-09: the first version's code (`discord.rs`, `audio.rs`, `codex.rs`,
`gate.rs`, `lib.rs`, `tauri-ui/*`, `media-bridge.js`) and the documents the rewrite started
from (`REWRITE-SPEC.md`, `REWRITE-NOTES.md`, `STACK-DECISION.md` in the first version's
repository, and `discord_voice_assistant_research_summary.md`) against this app. The tables
list every gap; the items marked "change 8" are this change's work, the others are done by
the change named.

### Audio and the assistant

| # | Gap | First version or document | This app | Where it goes |
|---|---|---|---|---|
| A1 | Send path drops frames | FIFO, every frame in order | newest frame only | change 1 |
| A2 | No receive jitter buffer or loss concealment | browser WebRTC (NetEQ) | packets decoded in arrival order | change 1 |
| A3 | No input and output gains | 0 to 4, user runs 3.39 and 2.52 | none | change 7 |
| A4 | Thread `approvalPolicy` | code: `untrusted`; `REWRITE-SPEC.md` 4.4 says `never` | `never`, taken from the document, not from the working code | change 1 |
| A5 | Mute ends the session | `gate.rs`: `wants_session = !muted && …`; mute ends the session and the call, no budget spent while muted, a new call on unmute | mute keeps the call open and sends silence | change 8 |
| A6 | Mute and unmute tones | four earcons: `Muted`, `Unmuted`, `WakeStart`, `WakeEnd` | two: `Activate`, `Deactivate`; mute is silent | change 8 |
| A7 | Mixing simultaneous speakers | sum divided by the number of speakers | sum clamped to i16; two loud voices clip | change 8 |
| A8 | Preparation at `/join` | `start_guild`: starts the app-server, checks the sign-in (refuses with a clear message when signed out), opens the thread; the first activation only places the call | the thread opens on the first activation; no sign-in check at `/join` | change 8 |
| A9 | Latency per turn | `[latency:<guild>] turn N: first Discord speech -> ChatGPT audio N ms`, interruptions | none | change 5 |
| A10 | Recognizer results in the UI | the Console shows the whole app log; `REWRITE-SPEC.md` 5.4.17: show what the recognizer heard | `[wake] … heard:` and phrase decisions go to the file only; the window shows only a few event lines | change 8, shown by the Console of change 4 |

### Discord

| # | Gap | First version or document | This app | Where it goes |
|---|---|---|---|---|
| D1 | Unknown speaker fallback | an SSRC without a name and exactly one non-bot member in the channel: that member (`REWRITE-NOTES.md`, `REWRITE-SPEC.md` 4.5, `discord.rs` `fallback_tried`) | not ported | change 8 |
| D2 | Slash command registration | in the configured guild and globally, both | one or the other; with an empty guild id only globally (up to an hour to appear) | change 8 |
| D3 | Start at launch | Discord starts automatically after a valid token is saved and on every launch | an `auto_connect` setting; the user's file has `false` (written by build 0.1.0) and an empty guild id, so the bot does not connect on launch | change 8 |
| D4 | Small Discord cache | `STACK-DECISION.md` 5.6: only guilds, voice states and members of the current channel | serenity's default cache | change 8 |

### Footprint

| # | Gap | First version or document | This app | Where it goes |
|---|---|---|---|---|
| F1 | When the Codex process starts | at `/join` (`REWRITE-SPEC.md` R18: nothing heavy loaded while idle) | at app start (about 70 MB private while nobody uses the bot) | change 8 |
| F2 | Memory metric | — | private bytes of a debug build | change 6 |

### What to do (change 8)

- **Codex process at `/join` (F1, A8).** Nothing of Codex runs at app start. `/join` starts
  the app-server if it is not running, reads the account, refuses the join with a clear
  message in Discord and in the status when Codex is not signed in, and opens the guild's
  thread. No voice call exists before an activation: in wake-word mode the call opens only
  after the activation phrase (in always mode the session is wanted from the join, as
  before). The app-server stops when the bot leaves the last voice channel, and at exit.
  "Sign in" and "Check" on the Status tab start the app-server on demand.
- **Mute as in the first version (A5).** Mute ends the session and its call (no voice
  budget spent while muted); the input to the assistant is silence and its output is
  discarded and flushed until then. In always mode unmute wants a session again and opens a
  new call; in wake-word mode the next activation does.
- **Mute tones (A6).** Port the first version's `Muted` and `Unmuted` earcons next to the
  session start and end tones, with the same volume setting and the same rule (they play
  even while muted).
- **Mixing (A7).** Mix simultaneous speakers as the first version: the sum divided by the
  number of speakers in the tick, so overlapping voices do not clip. Check with a
  two-speaker recording that neither voice is clipped or lost.
- **Recognizer results visible (A10).** The recognizer's `heard:` lines, the phrase
  decisions and the dropped-audio warnings reach the window: the app log is forwarded to
  the UI in full (Info and above from this app's crates), not only the event lines; the
  Console tab of change 4 shows it.
- **Unknown speaker (D1).** Port the first version's fallback: a speaking source without a
  name, tried once per source, gets the name of the only non-bot member in the channel; the
  log says it was assigned that way.
- **Slash commands (D2).** Register `/join`, `/leave`, `/status` in the configured guild (at
  once) and globally (for every other server), as the first version does.
- **Start at launch (D3).** As the first version: Discord starts after a valid token is saved
  and on every launch, with no switch; the Config tab shows "Auto-start enabled". The stored
  `auto_connect: false` from build 0.1.0 is ignored and dropped from the schema.
- **Small Discord cache (D4).** Configure serenity's cache for guilds, voice states and the
  members needed for names only, per `STACK-DECISION.md` 5.6.
- **Docs.** `docs/ARCHITECTURE.md` (gate and mute, Codex lifecycle, Discord) and `AGENTS.md`
  in the same change.

### Verification (change 8)

- Unit tests: mute ends the session and the session is wanted again on unmute in always
  mode; the mute tones play while muted; the two-speaker mix equals the average and does
  not clip; the speaker fallback names the only member and not when there are two.
- Local run: after app start no `codex.exe` is running; it appears at `/join` and is gone
  after `/leave` and at exit (the `/join` itself is the user's step; the start and stop of
  the process is checked by driving the runtime without Discord).
- The window shows the recognizer's `heard:` lines during a session.
- The real-channel test (the user's step).


## 9. The voice toward a far Discord voice server (follow-up, not ordered yet)

Found after `discord-voice-delivery` (2026-11-10). The user accepted that change on the near
server; this part is for a later OpenSpec change of its own.

### Evidence

The probe `discord-delivery 30 --opus` plays a tone made locally (no OpenAI leg at all)
through the app's Opus output with FEC. The user listened in the channel to two runs:

| Voice server | User's ping | Heard | Discord's reports on our stream |
|---|---|---|---|
| `finland14057` | about 300–380 ms | broken up all the time | up to 18.8 % lost in a second; 62 feedback packets (RTCP 205, likely NACKs) |
| `c-sin19-5f0d7996` (Singapore) | about 30 ms | clean | 0.0 % in every report; no feedback packets |

Both runs passed through all 1502 packets, and our send jitter was 0.6–1.2 ms.

### What this means

- The packets are lost after they leave the bot, on the way to Discord's server. Discord
  reports the loss itself.
- A buffer on our side cannot bring back a packet lost after it was sent. The buffer that
  could wait for late packets is in the listener's Discord client, which we do not control.
  Our voice buffer only absorbs what comes before it: the OpenAI server's stalls.
- Opus in-band FEC rebuilds one lost packet from the next one. It does not cover losses in
  a row, and at almost a fifth of packets per second they come in rows.

### Measure first, then pick (each one is a probe before code)

1. **Does the loss depend on our bitrate?** We send 128 kbit/s stereo (songbird's rate),
   while the Codex voice is about 50 kbit/s mono. Run the far server with the tone at
   128 kbit/s, 64 kbit/s and a mono-coded stream at the Codex voice's rate, and compare
   Discord's reported loss. If it falls with the rate, set the rate from the measured loss
   too.
2. **Can we answer Discord's NACKs?** songbird ignores RTCP 205. Find out whether Discord's
   server takes a resent packet, and whether a resend arrives in time at a round trip of
   300 ms or more.
3. **Redundancy deeper than one packet**, if 1 and 2 do not help: what songbird's send path
   allows without bypassing its DAVE encryption.
4. **Without code:** the channel's region override in Discord. It is the user's setting, and
   it already gives a clean voice at a near server.

### Verification

The same probe on the far server, with the user listening: Discord's reported loss and how
the tone sounds, before and after, for each step taken.

---

## Order

Each section is one OpenSpec change on its own branch. The branches are chained (each
starts from the tip of the previous one) and nothing is merged into `main` until the app
works end to end; then they are merged one by one in this order (see `AGENTS.md`, Git flow):

| Order | Section | OpenSpec change and branch |
|---|---|---|
| 1 | 1. Voice path | `voice-path-quality` |
| 2 | 7. Audio settings | `audio-gain-settings` |
| 3 | 8. Missing decisions | `first-version-parity` |
| 4 | 3. Window layout | `ui-layout-port` |
| 5 | 2. Settings form | `settings-form-behavior` |
| 6 | 4. Codex chat and Console | `chat-and-console-tabs` |
| 7 | 5. Performance tab | `performance-statistics` |
| 8 | 6. Memory | `memory-measurement` |

The voice path, the gains and the missing decisions come first: they decide whether the
assistant can be used at all. The layout port comes before the form, chat and statistics
work so that those are built once, in the new layout.

## Decisions

- 2026-11-09, the user: web search is allowed; the model gets full capabilities and is not
  restricted. Copy the first version's approach rather than re-inventing it: it is the
  project's own code.
- 2026-11-09, the user: the audio settings (what the assistant hears, how loud it speaks)
  are required; they are change 7.
- 2026-11-09, the user: port the Appearance card too (themes Dark, Darcula, Light, IntelliJ
  and font size), as part of change 3.
- 2026-11-09, the user: the Codex process starts at `/join`, not at app start; before an
  activation no voice session runs. The gaps of section 8 are a required change.

- 2026-11-09, the user: each section is its own OpenSpec change and branch; the branches are
  chained and merged into `main` one by one once the app works end to end. Disputed points
  are decided by the agent and reported afterwards.
- 2026-11-10, the user: `discord-voice-delivery` is accepted on the near voice server (ping
  about 30 ms). The breakups on a far server (ping about 300–380 ms) are a later task,
  section 9.

### Decided by the agent (2026-11-09, to report to the user)

- Gains start at 100 % in a settings file without them, as in the first version; the user
  sets 339 % and 252 % once on the Audio card.
- Mute ends the session and its call, as in the first version; unmute in always mode opens a
  new call.
- Simultaneous speakers are averaged, as in the first version.
- The Codex app-server starts at `/join` and also stops when the bot leaves its last voice
  channel (the first version kept it until exit); Sign in and Check start it on demand.
- Discord always starts when a token is stored; the `auto_connect` switch is removed.
- The UI stays plain HTML, CSS and JS as in the first version; no Playwright.
