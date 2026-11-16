# Design

## Context

See proposal.md (Why) for the report and the evidence. This section collects the facts the
decisions rest on. They come from the code at `62cfbfe`, the songbird 0.6.0 source in the
cargo registry, Codex's log database and a scratch probe, all on 2026-11-16.

**The wake worker** (`wake/worker.rs`):

- One thread for every speaker of every guild, fed by the gate through a queue of 256
  messages.
- `feed_trailing_silence` measures quiet as `now - last_audio`. `last_audio` is when the
  worker *processed* the speaker's latest frame, and `now` is its loop time, at least
  every 20 ms.
- From 60 ms of quiet on, it feeds silence for the whole quiet time, at most 1.5 s.
  Anything that holds a speaker's frames back for 60 ms therefore puts zeros into their
  speech. When the frames arrive, they follow the zeros.

**songbird's receive** (`driver/tasks/udp_rx/playout_buffer.rs`, `mod.rs`):

- Each speaker has a playout buffer of `playout_buffer_length` packets (5 by default,
  100 ms).
- A packet that never came is decoded as concealment, and the speaker stays in
  `tick.speaking`.
- When the buffer runs empty, which happens once a packet comes late, the speaker goes
  into fill mode. They are reported in `tick.silent`, with no frame, until 5 packets are
  buffered again. Then the delayed audio plays on. Nothing is lost, but the gate's stream
  for that speaker has a hole of 100 ms or more.
- Ticks run on a tokio timer (`sleep_until(playout_time)`) and catch up after a late one.
  Probe `receive-timing`, idle: interval p99 32 ms, max 37 ms. Never measured while the
  bot speaks or a game runs.

**The scratch probe** (`target\deactivation-probe\vosk-trace`, not in the repository). It
feeds 16 kHz WAVs (Windows voice Irina) through the app's `VoskEngine`, `Recognition` and
`PhraseSpotter`, and prints partial results, final results and confirmed phrases with
their times. Model `vosk-model-small-ru-0.22`, phrase "отбой".

| Input | Result |
|---|---|
| The phrase alone, in a sentence, repeated six times (two speech rates) | Every occurrence confirmed 0.08 s after it first appeared in a partial result |
| Another voice mixed in at half and at equal level | Same, all confirmed |
| White noise of amplitude 2000 | All confirmed |
| White noise of amplitude 5000 | The single phrase missed, repeated ones still confirmed |
| Zeroed spans of 60 to 120 ms every 0.4 to 0.7 s | The single phrase missed or late in most runs ("тогда", "одна бой", "плотно от", "ладно бой"), the phrase in a sentence missed ("от вот", "твой"), repeated phrases mostly confirmed |
| Zeroed 60 ms every 1 s | All confirmed |

**Codex's own log** (`%USERPROFILE%\.codex\logs_2.sqlite`, call of 2026-11-16 12:33:50).
The pieces of the people's speech arrive as `input_transcript.added` on Codex's side and
reach the app as `thread/realtime/transcript/delta`, one for every 200 ms window of audio,
about 0.2 s after it. In that call:

- " От" arrived at 12:34:49.743 and "бой" at 49.981;
- the voice's turn ended at the start of the user's speech;
- the reply " Окей" arrived at 50.553, before the user's turn was done (50.806);
- the app's `thread/realtime/stop` came at 51.844.

The pieces split words wherever a window ends: "Брат" + "анская", "самоир" + "онизиру".
`transcript/done` gives the turn's whole text with its role.

**The output** (`audio/output.rs`):

- `play_earcon` appends to the earcon queue.
- `flush` and `set_muted` clear the frames and the provider, not the earcon queue.
- `fill_packet` mixes the earcon whenever sending is on, and opens sending for it.
- The gate's deactivation path flushes, then queues the falling earcon. The Codex
  session's `session_wanted(false)` and `stop_realtime` flush again.

On reading, no path removes the tone, but no test covers a deactivation during playback
from the provider.

**Phrases today:**

- The phrase fields are textareas. `controlValue` splits them with `/[\n,]/`, so commas
  separate phrases, and `writeControl` shows them one per line.
- The settings store the arrays as entered. `normalize_phrases` trims and lower-cases at
  use, nothing more.
- The spotter compares whole words in a row (`phrase_occurrences`), so phrases of several
  words already work: "hey bro" is in the English corpus.
- The `settings-ui` spec said the fields show phrases "lower-cased after save". The code
  never did that, and the modified requirement drops the words.
- No phrases ship. `catalog.rs` carries none, and `ModelPhrases` has two `Vec` fields with
  a serde default, so an absent list and an empty list look the same.
- The window fills a phrase field from `stored.wake.phrases[model][list]`, or leaves it
  empty.

The scratch probe with phrases of two words (Irina, `vosk-model-small-ru-0.22`):

| Said | Phrase | Result |
|---|---|---|
| "Братан, подъём." (two rates) | "братан подъем" | Confirmed 0.43 s after the phrase ended; the recognizer wrote "подъем" |
| "Братан, подъём, скажи который час." | "братан подъем" | Confirmed, and the question after it recognized |
| "Отбой, братан." (two rates) | "отбой братан" | Confirmed 0.23 s after the phrase ended (one partial said "отбой браток" on the way) |
| "Всё, отбой, братан, спасибо." | "отбой братан" | Confirmed inside the sentence; the recognizer wrote "все" |

`knows_word` returns true for both "подъём" and "подъем", and for "всё" and "все", but the
recognizer writes the "е" form.

**Candidate default words.** The scratch probe asked all ten models in
`target\wake-corpus\assets` through `knows_word`:

- Every word in D7's table is known.
- Unknown, and therefore not used: Czech "brácho" and "mizím", Dutch "toedeloe", "houdoe"
  and "ajuus", German "digga", Polish "ziom", "brachu" and "narka", Portuguese "parça" and
  "flw".

The corpus of 2026-11-11, spoken by the Codex voice, already showed that some words are
weak:

- Czech "kámo" was heard as "kámoš";
- French "pote" was heard as "pâte";
- Portuguese "tchau" was not heard;
- Spanish "adiós" was heard as "a dios".

D7 takes these results into account.

## Goals / Non-Goals

**Goals:**

- The wake engine gets each speaker's speech as it was spoken: no zeros inside it.
- A deactivation phrase the call hears ends the session before the voice replies, even
  when the wake engine misses it.
- Every deactivation is audible (when earcons are on), and the next report can be
  explained from the log without the verbose log.
- Phrases are separated by commas, may be several words, and count the same way wherever
  they are compared.
- Every catalog model works out of the box with its default phrases, and the fields show
  them.

**Non-Goals:**

- The calls Codex closes for moderation. They stop the voice without a tone and are
  restarted. That is a separate follow-up.
- A larger Vosk model or looser matching in Vosk's output.
- Telling the voice model to stay silent on the phrase. The app ships no assistant
  instruction.
- New settings. The only UI changes are the phrase fields' text and help.

## Decisions

### D1. Trailing silence counted in receive ticks, after 300 ms

- The gate numbers its ticks per guild (`u64`) and calls `WakeFeed::tick(guild, tick)`
  once per `process_tick`, after the speakers' frames, while it feeds the wake engine.
  `Audio` messages carry the tick and the gate's `Instant` (for D5's lag).
- Each `SpeakerState` keeps the tick of its latest frame. On a `Tick`, the worker computes
  `absent = tick - last_tick` for each speaker of that guild:
  - below `SILENCE_AFTER_TICKS = 15` (300 ms), nothing is fed;
  - from 15 on, it feeds silence until it has fed `min(absent, 75)` chunks.
- A frame that arrives while `absent` is below 15 continues the stream directly. If
  `absent` was 3 or more, the gap is counted as bridged for D5.
- A dropped `Tick` only delays the silence until the next one, because each message
  carries its tick number.
- Wall time is used only for dropping a recognizer after 30 s without a frame.
- **Why 300 ms:** a fill lasts 100 ms after packets resume, plus however late the packet
  was. 300 ms covers lateness up to about 200 ms.
- **Why this costs no time at the end of a sentence:** Vosk ends an utterance only after
  about 0.5 s of silence. When the 300 ms are reached, the silence owed is fed at once, so
  Vosk's final result comes at about the same moment as before. A task measures this with
  the real model.

**Measured on 2026-11-17** (tasks 1.2, 1.3, 2.3; `vosk_gaps_when_assets_are_provided`,
real `vosk-model-small-ru-0.22`, Irina, holes of 6 ticks every 0.5 s in five phases
against the speech; full output in `target\deactivation-probe\gaps-*.txt`):

| File | Phrase(s) | Zeros in the holes (the old worker) | Back to back | D1 ticks |
|---|---|---|---|---|
| `ru-goodbye.wav` "Спасибо за помощь. Отбой." | отбой | 4 of 5 (one phase heard "от бобби") | 5 of 5 | 5 of 5 |
| `ru-deact-ladno` "Ладно, отбой." | the user's list plus "отбой" | 4 of 5 | 5 of 5 | 5 of 5 |
| `ru-deact-sentence` "Хорошо, отбой, увидимся завтра." | same | 4 of 5 | 5 of 5 | 5 of 5 |
| `ru-deact-finish` "Заканчивай." | same | 4 of 5 | 5 of 5 | 5 of 5 |
| `ru-deact-all` "Ну, братан, всё." | same | 3 of 5 | 5 of 5 | 5 of 5 |
| the four other files of the set | same | 5 of 5 | 5 of 5 | 5 of 5 |

The zeros run misses the phrase in some phases, as the scratch probe found; in every run the
D1 worker heard exactly what the back-to-back stream heard (the test asserts it).

The end of a sentence (the WAV's trailing silence cut, then Discord's 5 zero frames, then
nothing), from the WAV's last frame to the deactivation:

| File | Old worker | D1 | Difference |
|---|---:|---:|---:|
| `ru-goodbye.wav` | 480 ms | 480 ms | 0 |
| `ru-deact-alone`, `-all` | 400, 500 ms | 400, 500 ms | 0 |
| `ru-deact-ladno` | 300 ms | 400 ms | +100 ms |
| `ru-deact-finish` | 280 ms | 400 ms | +120 ms |
| `ru-deact-repeat`, `-sentence`, `-two`, `-two-sentence` | confirmed before the end | same | 0 |

**[decided alone]** Two short utterances are confirmed 100 to 120 ms later than before,
above the 0.1 s the task named; the others are unchanged. The cause: a deactivation still
waiting for its fifth partial result at the end of speech used to get those partials from
the zeros fed 60 ms after the last frame; D1 feeds nothing until the 15th absent tick, so the
earliest such confirmation is 5 zero frames + 300 ms = 400 ms after the speech. The delay is
bounded by 300 − 60 = 240 ms and only hits a phrase that was not confirmed while it was
spoken. I kept D1 as designed instead of bringing it to the user (the user asked for no
questions in this run): lowering the threshold puts zeros back into songbird's refill
holes, which is the fault this change fixes, and the call's transcript (D2) ends the
session on its own clock.

Alternatives:

- Keep the 60 ms and count it in ticks. This fixes worker lag and late ticks, but not
  songbird's fill, which is the most frequent hole.
- Detect the five silence frames a Discord client sends when it stops talking. A
  client's own voice detection flapping during double talk sends them too, so they do not
  tell a real end apart.
- Raise songbird's `playout_buffer_length`. That adds latency to every word the assistant
  hears and still empties on a late enough packet.

### D2. The people's transcript deltas as a second source

**The source.**

- Chosen: `thread/realtime/transcript/delta` from the app-server, the people's pieces
  only. The 2026-11-16 call shows they arrive about 0.2 s after the audio, early enough
  to cut before the reply.
- First task of the transcript group: read in the schema that `probe codex-schema` dumps
  which field tells the people's pieces from the voice's.
- If the delta carries no such field, the data channel's `input_transcript.added` events
  are used instead (`rtc.rs`). They are the people's speech by type.
- **Read on 2026-11-17** (task 5.1, `probe codex-schema target\deactivation-probe\schema`,
  Codex CLI 0.167.0): `ThreadRealtimeTranscriptDeltaNotification` is `{ delta: string,
  role: string, threadId: string }`, all required, "flat transcript delta emitted whenever
  realtime transcript text changes"; `ThreadRealtimeTranscriptDoneNotification` is `{ role,
  text, threadId }`. The field is `role`; the people's speech is `role: "user"`, as the
  existing `done` handling already uses. D2 stays on the app-server's deltas; the data
  channel is not needed. (`ThreadRealtimeItemTranscriptDeltaNotification` `{ delta, itemId,
  threadId }` carries no role and is not used.)

**Joining and handing over.**

- The session keeps the people's pieces of the current turn in order, as a buffer apart
  from the voice's pieces.
- After each piece it hands the joined text to `HeardSink`, together with a turn
  sequence number.
- The `done` of the people's role closes the turn: it hands over the final text and
  clears the buffer.
- It hands over nothing unless the call is `Live`, and only after releasing its own locks,
  because `session_wanted(false)` takes the session's state lock.

**Engine-independent parts.**

- `HeardSink` (`assistant/mod.rs`): `on_heard(guild, turn, text, done)`, carried as
  `AudioPorts.heard: Arc<dyn HeardSink>`.
- `RelayRegistry` implements it and routes to the guild's gate.

**The gate decides** (`RelayGate::on_heard`). It does nothing unless the mode is wake
word, mute is off and a session is active. Otherwise it applies D3:

- A match that does not end at the last word ends the session at once.
- A match whose last word is the last word of the text so far is held for that turn. The
  next piece confirms it if it starts with a space or punctuation, and drops it if it
  starts with a letter or digit, since the word goes on. `done` confirms it. So does the
  gate's tick, once 300 ms have passed without a newer piece.
- Confirming applies the shared deactivation path of D4 with the source "the call's
  transcript".
- **[decided alone] The voice is silenced while a match is held** (added after the first
  live round below). The gate's `output_muted` includes the hold, so the voice's reply that
  starts during the 300 ms is discarded rather than heard; if the word goes on, the hold is
  dropped and the voice plays again, a few hundred ms of it lost. The session still ends
  only when the hold is confirmed, as the spec scenarios say.
- **[decided alone] A turn cut at a pause is read with the next one** (added after the
  second live round). The call's voice detection ended a turn at the comma of "Отбой,
  братан" in 2 of 5 calls (" Отбой" done, then ", братан" as a new turn), so a two-word
  phrase never matched. A turn that ended less than 2 s before is now read with the next
  one, and only a match that ends in the newer turn counts.

**Measured on 2026-11-17** (task 5.6, the live call test with
`VOICEBRO_CODEX_CALL_DEACTIVATE_WAV`; three rounds of five calls per phrase, Irina's
"Отбой, братан." after a Russian question and David's "Goodbye." after an English one, the
phrase fed 1.5 s into the answer; `target\deactivation-probe\call\live-runs*.txt`). Times
are ms after the phrase's last frame was fed. "First" means the rule silenced the voice
before the first frame of its reply (or no reply came):

| Round | Phrase | Piece with the whole phrase | Reply's first frame | Rule first |
|---|---|---|---|---|
| 1 | отбой братан | 362–567 | 270–885, one call without a reply | 4 of 5 |
| 1 | goodbye | 98–940 | 795–1 476 | 5 of 5 |
| 2 | отбой братан | 362–627 (two calls split into two turns) | 316–1 088 | 3 of 5 |
| 2 | goodbye | 265–560 | 472–959 | 4 of 5 |
| 3 | отбой братан | 446–776 | 348–2 234 | 1 of 5 |
| 3 | goodbye | 397–726 | 563–1 287 | 5 of 5 |

Rounds 1 and 2 ran with the first version of the rule (no silencing during the hold, no
second turn); their "Rule first" column applies the final rule to the pieces they recorded
(`transcript_rule_decision` in the test does the same as the gate). Round 3 ran with the
final rule. In all: "goodbye" 14 of 15, "отбой братан" 8 of 15.

**The task's bar of 4 of 5 per phrase is met for "goodbye" and not for "отбой братан".**
The cause is on Codex's side: the people's transcript of the two-word phrase arrived 0.36 to
0.78 s after the speech, and in 7 of 15 calls the voice had already started its reply
(after 0.27 to 0.63 s), once before the transcript held even the first word. No rule on
the transcript can be earlier than the transcript. What the rule does in those calls is cut
the reply 10 to 250 ms after it started, with the falling tone, instead of letting the voice
answer the phrase as in the report. The wake engine, which runs on the audio itself and
with D1 no longer misses the phrase in the gaps, stays the faster path (the spotter put its
deactivations 0.23 s after the phrase's end in the corpus). A single-word phrase such as
the default "отбой" is in the transcript's first piece and would be silenced earlier.

Alternatives:

- Waiting for `done`. In the 2026-11-16 call it came after the reply had started.
- Matching in the Codex session. The assistant would need the wake phrases, and another
  engine would have to repeat the matching.

### D3. One matching form for every comparison

One function in `settings/mod.rs`, `phrase_words(text) -> Vec<String>`, replaces
`normalize_phrases` for matching:

- lower-case (Unicode) and fold `ё` to `е`;
- turn every character that is not alphanumeric (`char::is_alphanumeric`) into a space;
- split on whitespace.

A phrase is its word list, and a phrase with no word is no phrase. Every comparison uses
this form:

- **The wake engine.** `PhraseSet` holds word lists. `find_occurrences` brings the
  recognizer's words to the same form, so a recognized "по-моему" becomes two words with
  the end time of the original word.
- **The vocabulary check.** A word counts as known when the model knows either its folded
  form or its lower-cased form with "ё". Vosk returns true for both in the probe, and
  another model might know only one of them.
- **The save validation** (D6).
- **The transcript match.**

A phrase matches when its words appear in a row as whole words
(`spotter::phrase_occurrences`, made `pub(crate)`).

For the transcript, the phrases are the selected model's deactivation list in this form,
carried in `GateSettings.deactivation_phrases`. `Runtime::apply_settings` reapplies gate
settings when the `wake` section changes too. A phrase change never ends a session. The
vocabulary check does not apply to this list, because Codex's recognizer knows every word.

The gate's `last_wake_phrase` and the log lines show the phrase as the user typed it, so
the user recognizes it.

### D4. One deactivation path in the gate

- `on_phrase(Deactivate)` and the transcript share one function. It ends the session,
  flushes, queues the falling earcon when earcons are on, logs the source, and sets
  `deactivated_at`.
- The 1.5 s guard against an activation right after a deactivation reads
  `deactivated_at`, so it covers both sources. It moves from the worker's `quiet_until`
  into the gate.
- `PhraseSink::on_phrase` returns `PhraseOutcome { Applied, AlreadyInState,
  RightAfterDeactivation, NotListening }`, so the worker's decision line names the real
  reason.
- A test plays an answer through an attached provider, deactivates, then runs the Codex
  session's flushes. It reads the output and checks that the falling earcon's samples are
  all there, and that nothing of the answer plays after the deactivation.
- **Found on 2026-11-17** (task 3.2, `the_falling_earcon_plays_when_a_session_ends_mid_answer`):
  the tone was not missing in the code. Every sample of the falling earcon is in the
  packets after a deactivation in the middle of an answer and two flushes, nothing of the
  answer follows, and with earcons off nothing plays. So a stop without the tone in the
  user's report was not the app's deactivation (proposal, point 5), and no fix was needed
  in the output.

### D5. Diagnostics

All lines are info level and carry no recognized text:

- **Decision lines** gain "N.NN s after the phrase ended (N.NN s recognizing, N.NN s
  queued)". The first part is the spotter's `since_end`, the second the processing lag of
  the message.
- **The gate's deactivation line** names the source: "the wake engine" or "the call's
  transcript".
- **Near miss.** The deactivation `Detector` counts the partial results that held the next
  unreported occurrence since the last confirmation. A final result that confirms nothing
  new while that count is above 0 is reported as a near miss: "[wake] <g>/<ssrc>:
  deactivation «x» was in N partial results but was not confirmed".
- **Bridged gaps**, per speaker and at most once per 10 s: "[wake] <g>/<ssrc>: N gaps in
  the audio bridged in the last 10 s, longest M ms".
- **Lag**, at most once per 10 s: "the recognizer runs N ms behind the audio". It is
  written when the time from the gate's `Instant` to processing exceeds 500 ms.

### D6. Phrases separated by commas, each one or more words

This follows the user's decision of 2026-11-16: the comma is the separator. A phrase is
what stands between two commas, for example "Отбой братан, Заканчивай, Братан всё".

- **Reading the field.** `controlValue` keeps splitting on commas and on line breaks; a
  line break still separates, so a pasted list does not merge into one phrase.
- **Showing the field.** `writeControl` joins a list with ", " instead of line breaks, so
  the field reads the way it is typed.
- **The help text of both fields:**
  - phrases are separated by commas;
  - a phrase may have several words, for example "Отбой братан";
  - every word must be in the model's vocabulary;
  - each model comes with default phrases (D7), which can be changed or removed.
- **Storage.** The settings store each phrase as typed, trimmed. Stored lists need no
  migration, because they are arrays already.
- **Validation** (`Settings::validate`). Two phrases of one model are the same when their
  word lists (D3) are equal. The overlap check therefore catches "Братан всё" against
  "братан все".
- **Interplay of phrases.** An activation phrase may share a word with a deactivation
  phrase, as "братан" and "отбой братан" do. The spotter's rule within one utterance
  already keeps "отбой братан" from also counting as the activation "братан".
- **Cost.** A phrase of two words is confirmed after its last word. The replay still
  starts at the end of the phrase.
- **Measured on 2026-11-17** (task 4.7, `run-wake-corpus.ps1 -Deactivation`, Irina, the
  real Russian model, activation "братан", deactivation "Отбой братан, Заканчивай, Братан
  всё", with and without noise 300): 16 of 16 runs right, no false hit. "Отбой, братан.",
  "Всё, отбой, братан, спасибо.", "Заканчивай." and "Ну, братан, всё." end with the
  deactivation in both runs; "отбой" alone, after "ладно", repeated and in a sentence count
  as nothing, since "отбой" is not in that list. The spotter put every deactivation 0.23 s
  after the phrase's end (confirmed by 5 partial results) and the activation 0.43 s
  (15 partials). "Ну, братан, всё." reports the activation "братан" first and the
  deactivation "Братан всё" after it, both within the utterance: in a session the gate
  ignores the activation (already active) and ends the session on the deactivation, so no
  activation follows a deactivation.
- **[decided alone] A word with an apostrophe or a hyphen.** The matching form splits
  "don't" into "don" and "t", for the phrase and the recognizer alike, so they still match.
  For the vocabulary check (which asks the model about words), a typed token counts as
  known when the model knows it whole ("don't"), folded, or piece by piece
  (`spotter::word_known`), so such a phrase is not dropped as unknown.
- **[decided alone] Version-1 files.** `store::migrate_v1` now moves both version-1 lists,
  the empty one too, so a list the user had left empty stays empty instead of taking the
  model's default.

### D7. Default phrases of each catalog model

**Where they live.** Each `ModelEntry` in `catalog.rs` gets `default_activation` and
`default_deactivation`.

| Model | Activation | Deactivation | Why |
|---|---|---|---|
| English (US) | bro | peace out | Slang farewell. Two words, so a passing "peace" or "out" does not end a session |
| Russian | братан | отбой | The user's own phrases |
| Czech | ~~kámo, kámoš~~ brácha | ~~čus~~ čau | The corpus heard "kámo" as "kámoš", so both (replaced, see below) |
| Dutch | maat | ~~doei~~ doei doei | "maat" was 8 of 8 in the corpus; "doei" is the slang bye (replaced, see below) |
| French | ~~frérot~~ mon frère | ~~tchao~~ à la prochaine, a la prochaine | "pote" was heard as "pâte"; "au revoir" as "à voir" (replaced, see below) |
| German | bruder | ~~tschau~~ bis dann | "digga" is not in the model; "tschau" is rarer than "tschüss", which noise turned into "jobs" (replaced, see below) |
| Italian | fratello | arrivederci | "fra" is too short for the small model; "arrivederci" was 8 of 8 |
| Polish | ziomek | ~~nara~~ do widzenia | "ziom" is not in the model (replaced, see below) |
| Portuguese (Brazil) | ~~mano~~ irmão | ~~tchau~~ até mais | "falou" and "valeu" are everyday words; "tchau" was weak in the corpus and is checked again (replaced, see below) |
| Spanish | ~~compa~~ hermano | ~~chao~~ hasta luego | "adiós" was heard as "a dios" (replaced, see below) |

**The corpus check of 2026-11-17** (task 4.6). The Codex voice spoke each language's set
again with the defaults above (activation alone, at a sentence's start and end, a plain
sentence, deactivation alone and at a sentence's end; 40 new files, the plain ones kept),
and `run-wake-corpus.ps1` ran each file with and without noise 300. The rule: a default
counts when it is heard in at least 3 of its 4 runs (5 of 6 for an activation, which has
three files), with no false hit. The first run was 63 of 96 for the eight Codex-voiced
languages, no false hit; English and Russian passed (en: activation 14 of 16, the known
"Bro, how…" of Zira heard as "drill" and "throw", deactivation 8 of 8; ru 10 of 10). Every
failed default was replaced by the best of two to five candidates, each checked first
against its model's vocabulary (`VOICEBRO_WAKE_CANDIDATES`), then spoken by the Codex voice
into `target\wake-corpus\candidates\` (66 files) and run the same way
(`target\wake-corpus\candidates\run-*.txt`). **[decided alone]** which candidates and
which winner:

| Language | Failed default (heard in) | What the model wrote | Candidates (heard in) | Chosen |
|---|---|---|---|---|
| Czech | kámo, kámoš (3 of 6) | "samo", "dámu", "sám mu" | kámoš alone 4/6, brácha 5/6 | brácha |
| Czech | čus (1 of 4) | "sil s", "šum" | nashle 0/4, čau 3/4 | čau (also a greeting in Czech; it only ends a running session) |
| Dutch | doei (2 of 4) | "dooi", "door" when alone | tot ziens 2/4, doeg 2/4, de mazzel 2/4, tot later 4/4, doei doei 4/4 | doei doei, the slang bye said twice |
| French | frérot (3 of 6) | "frère ans", "raison" | frangin 3/6, frère 4/6, poto 0/6, mon pote 4/6, mon frère 6/6 | mon frère |
| French | tchao (0 of 4) | "tiens", "ciao" | à plus 2/4, à la prochaine 2/4 (alone written "a la prochaine"), both spellings together 4/4 | "à la prochaine" and "a la prochaine" |
| German | tschau (0 of 4) | "ciao", "schau" | tschüss 2/4, bis dann 4/4 | bis dann |
| Polish | nara (2 of 4) | "nora", "nowa" when alone | do widzenia 4/4, pa pa 0/4 | do widzenia |
| Portuguese | mano (1 of 6) | "humano" | irmão 6/6, parceiro 6/6 | irmão, the closer to "bro" |
| Portuguese | tchau (1 of 4) | "chão", missed in a sentence | até mais 4/4, adeus 0/4 | até mais |
| Spanish | compa (4 of 6) | "tampa", "hampa" when alone | hermano 6/6, colega 4/6 | hermano |
| Spanish | chao (2 of 4) | missed in a sentence | hasta luego 4/4, nos vemos 4/4 | hasta luego |

**[decided alone] Two spellings for French.** The small French model writes "a la
prochaine" without the accent when the phrase is said alone, and "à la prochaine" inside a
sentence. The matching form (D3) folds only "ё", so the default list holds both spellings,
as the user guide advises for any phrase that can be written two ways. "a plus" was not
added the same way: "il n'y a plus" is everyday French.

**The final run** (`target\wake-corpus\corpus-run-2026-11-17.txt`): 140 of 144 runs right,
no false hit; every default heard in at least 3 of its 4 runs (per language, activation and
deactivation: ru 6/6 and 4/4, en 14/16 and 8/8, cs 5/6 and 3/4, nl, fr, de, it, pl, pt and
es 6/6 and 4/4). `catalog_models_load_when_enabled`: all ten models load and know every
default word.

**Absent versus empty.** `ModelPhrases` gets `Option<Vec<String>>` lists:

- `None`, or the field absent from the file, means the model's defaults;
- `Some`, even empty, means the user's list.

Serde skips `None` when writing, so a save that names one list writes only that one. The
other stays absent and keeps its default. `WakeSettings::selected_phrases()` returns the
effective lists.

Existing files:

- An entry written by an earlier build has both lists present, so its lists stay the
  user's.
- A model with no entry gets its defaults. For the user that is every model but Russian.

**Local models.** A local model takes the defaults of the catalog entry whose language its
folder name gives (`local.rs` already reads the language). Otherwise it has none, and the
status says "no phrases configured" until the user types some.

**The window.**

- The `wake_models` command already lists the catalog models. Each entry now also carries
  its defaults.
- `fillField` shows the stored list, or the model's defaults when the list is absent.
- Leaving a field unchanged saves nothing, so the defaults stay defaults.
- Editing a field saves that list, and emptying it saves `[]`.

**Checks.**

- `catalog_models_load_when_enabled` checks that every default word is known to its model.
- `scripts\make-wake-corpus.ps1` and `scripts\run-wake-corpus.ps1` switch their test
  phrases to the defaults. The Codex voice speaks the eight other languages' sets again,
  about 32 real calls.
- A default that fails in the corpus is replaced by the next candidate of this table's
  reasons, and the replacement is recorded here.

**Docs and rules.**

- `AGENTS.md` loses "no wake phrases" from the line that says the code ships no defaults.
  The assistant instruction stays the user's.
- `docs/ARCHITECTURE.md` 4.4 loses "the catalog carries no phrases".
- The Purpose of `openspec/specs/wake-word-detection/spec.md` mentions the defaults.
- The user guide shows the table.

Alternatives:

- **Writing the defaults into `settings.json` at load.** That changes a migration
  scenario of `app-settings`, and it writes the file for nothing.
- **Defaults only as placeholder text in the fields.** The wake word would still not work
  until the user types something, which is what the request wants to avoid.

## Risks / Trade-offs

- [A hole longer than 300 ms still gets silence] → The bridged-gaps line shows how long
  real holes are, and the constant can follow.
- [A member's microphone picks up the bot's voice, so the call transcribes the voice's own
  phrase as people's speech] → The wake engine has the same exposure through that member's
  stream today. The result is an early end of the session, as now.
- [A question that contains the phrase ends the session] → This is already true for the
  wake engine (spec scenario "Wake word then deactivation in one sentence").
- [The 300 ms wait on an open last word lets the reply's first syllable through] → In the
  measured call the reply's first piece came 0.57 s after the phrase's last piece. The
  live test prints this margin for every run.
- [Moderation closes still stop the voice silently] → Out of scope, offered as a separate
  change.
- [A user in wake-word mode who left a model's phrases empty on purpose now has sessions
  start on its default] → That is the request. Emptying the field keeps it empty for good
  (D7), and the user guide says so.
- [A default is misheard in noise or by another accent] → The corpus check per language,
  and the near-miss line for deactivations. The user can replace any default.
- [The English "peace out" shows up in game talk] → It ends only an active session, and a
  user can replace it.

## Migration Plan

The settings file format is unchanged. Lists that are absent from a file now mean the
model's defaults instead of empty. Rollback is the previous tag's build, which treats an
absent list as empty again.
