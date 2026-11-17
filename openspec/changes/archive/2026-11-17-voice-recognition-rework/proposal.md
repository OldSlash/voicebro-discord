# Proposal

## Why

This change reworks how the bot recognizes its phrases. It covers three requests the user
made on 2026-11-16.

### 1. The deactivation phrase does not end the session

The deactivation phrase often does not end the session while the Codex voice is
answering. The user observed three things:

- they sometimes have to say it many times in a row before one counts;
- the voice meanwhile stops, takes the phrase as a question and answers it ("yes, yes,
  goodbye, bro");
- when it finally "works", no falling tone plays.

A quiet test alone works. Full use, in a game with friends, does not.

What the investigation found:

1. **A real call, from Codex's own log** (`%USERPROFILE%\.codex\logs_2.sqlite`, the call at
   12:34 from the portable trial).
   - Codex had the user's phrase as text in two pieces, about 0.2 s after each piece was
     spoken. The pieces arrive as `thread/realtime/transcript/delta` notifications.
   - Its voice stopped when the user started speaking: that is the server's barge-in.
   - It began its reply ("Okay, stop.") 0.4 s after the word, before the user's turn had
     even ended.
   - The app asked Codex to stop the call 1.9 s after Codex had the word. That time
     includes closing the WebRTC connection, which comes first.
   - The app's own log of that session was in the portable folder. A later trial build at
     15:00 deleted that folder, so the exact moment of the deactivation is lost.
2. **The phrase logic is sound on clean speech.** The app's Vosk recognizer and phrase
   spotter were fed recordings by a scratch probe (`target\deactivation-probe`, Windows
   voice Irina). Every occurrence of the phrase was confirmed 0.08 s after it appeared in
   a partial result: the phrase alone, inside a sentence, repeated, under another voice at
   equal level, and under moderate noise.
3. **Short gaps of silence inside speech break it**, and that matches the report. With
   60 to 120 ms of silence spliced into the speech every 0.4 to 0.7 s, a single phrase was
   missed or heard as other words: "ладно отбой" became "тогда", "одна бой" or "плотно
   от". In a run of repeated phrases, some still got through. That is the "say it ten
   times" pattern.
4. **The app inserts those gaps itself.** The wake worker feeds synthetic silence once a
   speaker's frames stop for 60 ms, measured by its own clock. In normal operation frames
   stop mid-word without any audio being lost:
   - songbird's playout buffer, once a late packet empties it, reports the speaker as
     silent until 5 packets (100 ms) are buffered again, then plays the delayed audio;
   - a late receive tick does the same;
   - so does a worker that is busy with other speakers.

   Each time, 60 ms or more of zeros go into the word. Jitter is higher in full use (a
   game on the same machine and network, several speakers) than in a quiet test.
5. **A stop without the falling tone was not a deactivation.** Every deactivation the gate
   applies queues the tone. A flush does not remove it, and tones play while the output is
   muted. So a silent stop was the server's own barge-in, and the session stayed open:
   - it kept sending the channel to the call;
   - its idle timeout restarted with every voice in the channel.

   A second kind of silent stop is in the Codex log: on 2026-11-14 at 21:42 and 21:43,
   Codex closed the call three times with "The generation was stopped due to
   moderation". The app restarts such a call within 2 to 6 s, without a tone.

### 2. Phrases of several words

Phrases are separated by commas, and a phrase may be several words. For example, the
field "Отбой братан, Заканчивай, Братан всё" holds three deactivation phrases. Each of
the user's phrases today is one word.

The engine already spots a run of words. The same scratch probe, with the real Russian
model:

- "братан подъем" was confirmed 0.43 s after the phrase ended, with the question after
  it still recognized;
- "отбой братан" was confirmed 0.23 s after it ended, also inside "всё, отбой, братан,
  спасибо".

One thing stands in the way: **the Russian model writes "е" where people type "ё".** It
writes "все" and "подъем", so "Братан всё" typed with "ё" would never count. The model
knows both spellings, so the vocabulary check does not warn either.

### 3. Default phrases for every model

Every wake model should come with phrases, so that the wake word works at once and a new
user sees what goes into the fields. The defaults are:

- for activation, the language's slang word for "bro" ("братан" in Russian, "bro" in
  English);
- for deactivation, a slang farewell that does not come up in ordinary game talk.

Today the code ships no phrases: the fields start empty and the engine says "no phrases
configured". The rule in `AGENTS.md` that the code ships no wake phrases changes with
this request.

## What Changes

- **No synthetic silence inside speech.** The wake engine counts a speaker's absence in
  receive ticks, not by its own clock. It feeds trailing silence only after 300 ms of
  absence (songbird's 100 ms refill plus up to 200 ms of lateness). Audio that comes back
  sooner continues the same stream with nothing inserted. This should cost no time at the
  end of a sentence: Vosk ends an utterance only after about 0.5 s of silence, and the
  silence owed is fed at once when the 300 ms are reached. A test with the real model
  checks it.
- **The call's transcript is a second way to deactivate.** In wake-word mode, during an
  active session, the people's transcript deltas from the call are joined per turn and
  checked for the selected model's deactivation phrases. A match ends the session as the
  wake engine's deactivation does: the voice stops at once, the falling tone plays, and
  the call stops. In the call above this would have cut the voice before its reply began.
- **Phrases separated by commas, each one or more words.**
  - The phrase fields split on commas. A line break also separates, so a pasted list does
    not merge.
  - A phrase is the words between two commas, separated by spaces.
  - After a save the fields show the phrases joined with ", ".
- **One matching form everywhere.** The wake engine, the transcript check and the save
  validation compare phrases in one form: lower-case, "ё" as "е", and the words in a row
  as whole words. The recognizer's words are brought to the same form. So "Братан всё"
  counts when the model writes "братан все", and "Отбой братан" in one list and "отбой
  братан" in the other are refused as the same phrase.
- **Default phrases for every catalog model**, verified against each model's vocabulary:

  | Model | Activation | Deactivation |
  |---|---|---|
  | English (US) | bro | peace out |
  | Russian | братан | отбой |
  | Czech | brácha | čau |
  | Dutch | maat | doei doei |
  | French | mon frère | à la prochaine, a la prochaine |
  | German | bruder | bis dann |
  | Italian | fratello | arrivederci |
  | Polish | ziomek | do widzenia |
  | Portuguese (Brazil) | irmão | até mais |
  | Spanish | hermano | hasta luego |

  This is the table after the corpus check of 2026-11-17 (task 4.6), which replaced the
  defaults that the corpus spoken by the Codex voice did not recognize; design D7 records
  each replacement.

  - A list the user never set uses the model's defaults, and the fields show them. A list
    the user set, even to empty, stays as set.
  - A local model takes the defaults of its language when the folder name gives a catalog
    language.
  - The corpus with the Codex voice checks each language's defaults. A default that is not
    recognized is replaced, and the replacement is recorded.
  - `AGENTS.md`, the catalog section of `docs/ARCHITECTURE.md`, the user guide and the
    Purpose of the `wake-word-detection` spec say so.
- **The falling tone is part of the deactivation's contract**, and a test checks that it
  plays when the session ends in the middle of an answer.
- **New log lines at info level, without recognized text.** Each one tells the next report
  apart:
  - the source of each deactivation (wake engine or call transcript);
  - the delay from the phrase's end to the decision;
  - a deactivation phrase seen in partial results but never confirmed;
  - gaps in a speaker's audio that were bridged, with the longest one, at most once per
    10 s;
  - the wake worker running more than 500 ms behind.
- No new setting and no new dependency. Beyond the defaults in the fields, the UI change
  is how the phrase fields join their text and their help text. `docs/ARCHITECTURE.md`
  (4.3, 4.4, 4.5, 4.6, 7), `AGENTS.md` and `docs/user-guide.md` describe the result.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `assistant-session-gate`:
  - a new requirement "Deactivation from the call's transcript";
  - "Deactivation is immediate" also requires the falling tone.
- `wake-word-detection`:
  - "Phrases come from the settings": phrases of several words, the matching form, and
    the defaults for a list never set;
  - a new requirement "Default phrases of each model";
  - "Trailing silence": counted in receive ticks, starting after 300 ms, never inside
    speech;
  - "Diagnostics": the bridged gaps, the lag, the unconfirmed phrase and the decision
    delay.
- `codex-assistant`: a new requirement "The people's transcripts reach the gate", only
  while the call is live.
- `app-settings`:
  - "Wake phrases per model": a list never set falls back to the model's defaults instead
    of being empty;
  - a new requirement "Phrases are compared in their matching form".
- `settings-ui`: "Phrases are lists" separates phrases by commas, joins them with ", ",
  and shows a model's defaults.

## Impact

- Code:
  - `crates/voicebro-core/src/wake/worker.rs` and `wake/mod.rs`: the tick-counted
    absence, the threshold, the gap, lag and decision lines, and the effective phrases.
  - `wake/spotter.rs`: near misses, and the recognizer's words in the matching form.
  - `wake/catalog.rs`: the default phrases of each entry.
  - `settings/mod.rs`:
    - `ModelPhrases` lists that are absent when never set;
    - the matching form;
    - validation in that form.
  - `apps/desktop/ui/main.js` and `index.html`: comma-joined phrase fields, the defaults
    shown, new help text.
  - `gate.rs`: tick numbers to the wake feed, matching against the transcript, one shared
    deactivation path, deactivation phrases in `GateSettings`.
  - `assistant/mod.rs`: an engine-independent sink for heard text in `AudioPorts`.
  - `assistant/codex/session.rs`: forwarding the people's transcript deltas.
  - `runtime.rs`: the wiring, and gate settings reapplied when the `wake` section changes.
- Tests and scripts:
  - The live call test gets an optional phrase WAV.
  - A Vosk test, gated by `VOICEBRO_VOSK_DIR`, replays the corpus phrase with
    refill-style gaps.
  - `scripts\make-wake-corpus.ps1` and `scripts\run-wake-corpus.ps1` use the new default
    phrases.
- Settings files: a user who never set a model's phrases gets its defaults. One who did
  keeps their own lists. The file format does not change.
- Not covered:
  - the calls Codex closes for moderation (a separate follow-up);
  - the Vosk model's own accuracy;
  - always mode, which has no deactivation phrase.
- The user checks the result in a real channel, with a second person and a game running.
  They say the phrase over the voice's answers several times; the log shows the source and
  the delay of each deactivation, and the gaps.
