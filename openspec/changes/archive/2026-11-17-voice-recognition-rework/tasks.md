# Tasks

Groups 1 to 3 do not depend on Codex and come first: they fix the cause the probe found.
Group 4 is about phrases:
- every comparison in one matching form;
- phrases of several words between commas;
- default phrases for every catalog model, checked with the Codex voice.

Group 5 adds the second source and reuses group 4's matching. Its first task reads
the schema and may switch D2 to the data channel; nothing else in the group changes then.
Questions that come up while building are decided under the decision they touch in
design.md, marked **[decided alone]**, so the user can review them at the end.

## 1. Branch and the regression test with the real model

- [x] 1.1 Create `change/voice-recognition-rework` from `main`. Verify that `git status` shows that branch with only this change's folder untracked
- [x] 1.2 Add a Vosk test gated by `VOICEBRO_VOSK_DIR`, next to the corpus test in `wake/vosk/mod.rs`. It feeds `VOICEBRO_WAKE_GAPS_WAV` (default: the corpus' `ru-goodbye.wav`) through `SpeakerState` twice, as a speaker whose frames go missing for 6 ticks every 0.5 s, the way songbird's fill does:
  - once with zeros put where the frames were missing, which is today's behavior;
  - once with the frames back to back, which is D1.

  It prints what Vosk heard and when the phrase was confirmed in each. Verify with the Russian model and phrase "отбой": the back-to-back run confirms the phrase. Record whether the zeros run misses it in design D1, next to the probe table
- [x] 1.3 Move the scratch probe's WAV recipe into `scripts\make-wake-corpus.ps1` as an optional `-Deactivation` set in `target\wake-corpus\ru\` (Irina):
  - the phrase alone, after "ладно", repeated, and inside a sentence;
  - "Отбой, братан." and "Всё, отбой, братан, спасибо.";
  - "Заканчивай." and "Ну, братан, всё.".

  Verify that the script makes the files and that 1.2 runs on each

## 2. Trailing silence counted in receive ticks (D1)

- [x] 2.1 Make the wake feed carry ticks:
  - `WakeFeed::tick(guild, tick)`;
  - `Audio` messages carrying the tick and the gate's `Instant`;
  - `WakeMessage::Tick`;
  - the gate numbering its ticks per guild and calling `tick` after the speakers' frames while it feeds the wake engine.

  Verify gate tests with `CountingFeed` recording ticks:
  - one tick per `process_tick` in wake-word mode;
  - none while muted or in always mode
- [x] 2.2 Replace the worker's wall-clock trailing silence with D1's rule (`SILENCE_AFTER_TICKS = 15`, catch-up to `min(absent, 75)` chunks), keeping wall time only for the 30 s idle drop. Verify worker tests:
  - an absence of 14 ticks followed by a frame feeds nothing, and the frame follows directly;
  - 15 absent ticks feed 15 chunks, then one per tick, capped at 75;
  - four speakers whose messages are processed 300 ms after their ticks, each with a frame in every tick, get no silence;
  - a dropped `Tick` is caught up at the next one;
  - the test of a phrase at the end of a sentence passes on ticks
- [x] 2.3 Run 1.2 with the D1 worker in place of its back-to-back variant, and the corpus with `scripts\run-wake-corpus.ps1 -Languages ru,en`. Measure, with the real Russian model, the time from the end of `ru-goodbye.wav` to the confirmation with Discord's end of speech simulated (5 zero frames, then no frames), before and after D1. Verify:
  - the gaps run confirms the phrase;
  - the corpus results equal those of 2026-11-11 (ru 8 of 8, en 25 of 28, no false hit);
  - the end-of-sentence time differs by under 0.1 s, written into design D1. If it differs by more, write the number into D1 and bring it to the user before going on
- [x] 2.4 Update `docs/ARCHITECTURE.md` 4.4 (the tick message, the 300 ms rule, songbird's fill as the reason) and the trailing silence fact in `AGENTS.md`. Verify that both state the tick count, the 300 ms and the 1.5 s cap

## 3. One deactivation path and its tone (D4)

- [x] 3.1 Make one deactivation function in `gate.rs` with a source label, used by `on_phrase(Deactivate)`. It sets `deactivated_at`, and the 1.5 s activation guard moves from the worker's `quiet_until` into it, with an entry point that takes the instant, for tests. Replace the `bool` of `PhraseSink::on_phrase` with `PhraseOutcome`. Verify tests:
  - an activation 0.8 s after a deactivation does nothing and returns `RightAfterDeactivation`;
  - one 2 s after starts a session;
  - the worker's decision line names each outcome;
  - the existing gate and wake tests pass
- [x] 3.2 Add D4's tone test: an output with an attached provider playing voiced frames, a gate in an active session, a deactivation, then the flushes the Codex session makes. Read packets until the earcon queue is empty. Verify:
  - every sample of the falling earcon is in the packets;
  - no frame of the provider plays after the deactivation;
  - with earcons off, no tone plays.

  If the tone is missing, the cause is found and fixed in this task, and the finding goes into design D4
- [x] 3.3 Update `docs/ARCHITECTURE.md` 4.3 (the shared path, the guard in the gate, the tone) and 4.4 (the outcome type, the guard removed from the worker). Verify that the text matches D4

## 4. Phrases: one matching form, several words between commas, defaults (D3, D6, D7)

- [x] 4.1 Add `phrase_words` to `settings/mod.rs` (D3: Unicode lower-case, `ё` → `е`, non-alphanumerics to spaces, split on whitespace). Verify unit tests:
  - "Отбой братан" and " отбой   братан " give `["отбой", "братан"]`;
  - "Братан всё" gives `["братан", "все"]`;
  - "Заканчивай" gives `["заканчивай"]`;
  - an empty or blank phrase gives no words and is dropped
- [x] 4.2 Make the wake engine use it:
  - `PhraseSet` built from word lists;
  - `find_occurrences` bringing the recognizer's words and text to the same form, with a split word keeping the original word's end time;
  - `usable_phrases` treating a word as known when the model knows its folded or its lower-cased form.

  Verify spotter and worker tests:
  - "отбой братан" is confirmed in "ну все отбой братан спасибо", once;
  - "отбой братан" is not confirmed by "отбой" alone;
  - "Братан всё" matches a recognition "братан все", and "всё" is not listed as unknown when the model knows only "все";
  - the existing spotter tests pass
- [x] 4.3 Give `ModelPhrases` `Option<Vec<String>>` lists with `skip_serializing_if = "Option::is_none"`, add the defaults of D7's table to every `ModelEntry` in `catalog.rs`, and make `WakeSettings::selected_phrases()` return the effective lists:
  - the stored list;
  - else the catalog defaults;
  - else, for a local model, the defaults of its folder's catalog language.

  Verify tests for each scenario of the `app-settings` delta and the `wake-word-detection` "Default phrases of each model" requirement:
  - a save naming one list writes only that one, and the other stays absent;
  - `[]` stays empty;
  - a file without an entry loads the defaults and is not rewritten;
  - an entry written by the earlier build (both lists present) is kept as it is;
  - `local:vosk-model-small-de-0.21` gets the German defaults;
  - a local folder of another language gets none;
  - the existing settings and store tests pass
- [x] 4.4 Compare phrases in their word lists in `Settings::validate`, for the overlap check across both lists, the effective defaults included. Verify tests:
  - "Братан всё" against "братан  все" is refused, naming the model and both lists;
  - "Отбой братан" is saved as typed;
  - a user activation list that contains a default deactivation phrase of the same model is refused, naming both lists
- [x] 4.5 Add the default words to `catalog_models_load_when_enabled` in `wake/vosk/mod.rs`, replacing the test's own word lists. Run it with `VOICEBRO_VOSK_DIR=target\wake-corpus\assets` and `VOICEBRO_WAKE_CATALOG=1`. Verify that all ten models load and know every default word
- [x] 4.6 Switch the phrases of `scripts\make-wake-corpus.ps1` and `scripts\run-wake-corpus.ps1` to D7's defaults. For each language the corpus speaks:
  - the activation alone and at a sentence's start and end;
  - a plain sentence without a phrase;
  - the deactivation alone and at a sentence's end.

  Make the en and ru sets with the Windows voices and the eight others with `-Codex` (about 32 real calls). Run the corpus with and without noise 300. Verify that every default is recognized in at least 3 of its 4 runs, with no false hit. For a default that fails, take the next candidate from D7's reasons, run its language again, and record the replacement in D7, in the `wake-word-detection` delta and in the catalog
- [x] 4.7 Run the user's Russian set of 1.3 through the corpus test with the deactivation list "Отбой братан, Заканчивай, Братан всё" and activation "братан". Verify that each file gives the expected last kind and that no activation follows a deactivation. Record the confirmation times in design D6
- [x] 4.8 Change the phrase fields in the window:
  - `writeControl` joins with ", " (`controlValue` keeps splitting on commas and line breaks);
  - `fillField` shows a model's defaults for a list that is absent, taken from the `wake_models` entries, which now carry `default_activation` and `default_deactivation`;
  - an unchanged field saves nothing;
  - the help text of both fields per D6, without "Empty by default".

  Mirror the defaults in the mock (`apps/desktop/ui-dev`), including `?fresh`. Add scenarios to `check-config-tab.mjs`:
  - "Отбой братан, Заканчивай, Братан всё" saves three phrases and shows them joined with ", ";
  - a first start shows "bro" and "peace out";
  - emptying a field saves `[]` and the default does not come back;
  - switching to the Russian model shows its phrases;
  - the help text.

  Update `check-wake-dialog.mjs` where a first start now has phrases. Verify that both checks pass
- [x] 4.9 Update the docs:
  - `docs/user-guide.md`: phrases separated by commas, several words per phrase, D7's table, and that an emptied field stays empty;
  - `docs/ARCHITECTURE.md` 4.4 (the matching form, the defaults in the catalog, the effective lists) and 4.6 (`ModelPhrases` with absent lists);
  - `AGENTS.md` ("the code ships no default assistant instruction"; the wake phrases now have defaults);
  - the Purpose of `openspec/specs/wake-word-detection/spec.md`, which should say that the phrases are each model's defaults or the user's own.

  Verify that the guide's steps match the window in the mock and that `openspec validate --specs` passes

## 5. The people's transcript as a second source (D2, D3)

- [x] 5.1 Dump the schema with `cargo run -p probe -- codex-schema target\deactivation-probe\schema` and read the params of `thread/realtime/transcript/delta` and `thread/realtime/transcript/done`. Write into design D2 the field that tells the people's pieces from the voice's, or switch D2 to the data channel's `input_transcript.added` if there is none. Verify that D2 names the source and its fields
- [x] 5.2 Add `deactivation_phrases` to `GateSettings` as word lists (`phrase_words` of 4.1) from the selected model's effective lists (`selected_phrases()` of 4.3, defaults included), and reapply gate settings on `wake` changes in `Runtime::apply_settings`. Verify tests:
  - another model's phrases are not taken;
  - a model with no entry gives its default deactivation phrase;
  - a phrase-only save during a session keeps the session and the next transcript uses the new list
- [x] 5.3 Add the transcript match on `phrase_words` and `phrase_occurrences`, with a flag for a match that ends at the last word. Verify unit tests:
  - "Okay, GOODBYE!" matches "goodbye" and is not open;
  - " Good" + "bye" matches "goodbye" and is open;
  - " stop" + "watch" does not match "stop";
  - " Отбой" + ", брат" + "ан" matches "отбой, братан" only after the last piece;
  - an empty list never matches
- [x] 5.4 Add `HeardSink` and `AudioPorts.heard`, with a no-op sink for the stub, the probes and the tests. Implement the sink on `RelayRegistry`, add `RelayGate::on_heard` with D2's hold of an open last word (confirmed by the next piece, `done`, or 300 ms on the gate's tick) through the shared path of 3.1, and wire it in `runtime.rs`. Verify gate tests for every scenario of the `assistant-session-gate` delta:
  - a miss during an answer;
  - the phrase in pieces;
  - a longer word;
  - punctuation and case;
  - both sources;
  - the wake word right after;
  - always mode;
  - another model's phrases.

  Also verify that `cargo build --all-targets` passes
- [x] 5.5 Make the Codex session forward the people's pieces per D2: the turn's joined text after each piece, the final text on `done`, only while `Live`, outside its locks. Extend `testdata/fake-codex` with `fake/transcript {threadId, method, params}`, which sends that notification. Verify tests with the fake for the `codex-assistant` delta:
  - pieces reach the sink joined per turn while live;
  - the voice's pieces never do;
  - nothing after a stop;
  - a sink that ends the session from inside the call does not deadlock
- [ ] 5.6 Extend the live call test with the optional `VOICEBRO_CODEX_CALL_DEACTIVATE_WAV`, fed 1.5 s into the answer, with a recording sink. Run it five times each with Irina's "Отбой, братан." and David's "Goodbye." made per `LOCAL-TESTING.md`. Print, per run, in ms after the phrase's last frame was fed:
  - each forwarded piece;
  - the moment D2's rule would deactivate;
  - the reply's first audio frame.

  Verify that the rule deactivates before the reply's first frame in at least 4 of 5 runs per phrase. Write the numbers into design D2

  Note (2026-11-17), left open because the bar is not met for one phrase: three rounds of
  five calls per phrase (`target\deactivation-probe\call\run-live.ps1`, logs
  `live-runs*.txt` there). With the final rule (the voice silenced while a match is held,
  a turn cut at a pause read with the next one) the voice was silenced before its reply in
  14 of 15 calls for David's "Goodbye." (5, 4 and 5 per round) and in 8 of 15 for Irina's
  "Отбой, братан." (4, 3 and 1): the transcript of the two words came 0.36 to 0.78 s after
  the speech, and the reply often started 0.27 to 0.63 s after it. Design D2 has the table
  and the two changes to the rule that the rounds led to
- [x] 5.7 Describe the second source in `docs/ARCHITECTURE.md`:
  - 4.3: `on_heard`, the hold, the matching;
  - 4.5: `HeardSink` in the `AudioPorts` block, and the forwarding;
  - 5, Control: the arrow from the assistant to the gate.

  Verify that the text matches D2 and D3

## 6. Diagnostics (D5)

- [x] 6.1 Add the decision delay to the worker's decision lines, and the source to the gate's deactivation line. Build both lines in pure functions. Verify unit tests of the line builders with known values
- [x] 6.2 Add near misses to the deactivation `Detector` and `PhraseSpotter`, logged by the worker. Verify spotter tests:
  - 3 partials with the phrase, then a final without it → a near miss of 3;
  - a phrase confirmed by partials or by the final → none;
  - an activation phrase seen but not confirmed → none
- [x] 6.3 Add the bridged-gaps line and the lag line, each at most once per 10 s. Verify worker tests:
  - gaps of 6 and 3 ticks within 10 s give one line with 2 gaps and 120 ms;
  - a message 700 ms old gives a lag line, and a second one 3 s later gives none
- [x] 6.4 Check that recognized speech stays in detail lines. Verify a worker test with a capturing logger: the decision, near-miss, gap and lag lines hold only the phrase and numbers, and the recognized text appears only in the `heard:` detail line
- [x] 6.5 Add the lines to `docs/ARCHITECTURE.md` 4.4 and 7. Verify that the documented wording matches the code

## 7. Checks and acceptance

- [x] 7.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, both window checks and `openspec validate voice-recognition-rework --strict`. Verify that all pass, and put their output in the report

  Note (2026-11-17): `cargo fmt --check` clean. `cargo clippy --all-targets`: no errors;
  6 warnings, none in this change's code (`chunks_exact` in `secrets.rs`,
  `audio/output.rs`, the probe's `credential.rs`, `opus.rs` and `rtc.rs`, and the
  `let…else` of `discord/receive.rs`, the same as before the change). `cargo test`: core
  361, desktop 15, probe 2, all pass. `check-config-tab.mjs` 44 of 44 (5 new phrase
  scenarios), `check-wake-dialog.mjs` 23 of 23. `openspec validate
  voice-recognition-rework --strict`: valid. The gated real-model tests: the gaps test, the
  corpus (140 of 144, no false hit), the deactivation set (16 of 16) and the catalog check
  (10 models, every default word known)
- [ ] 7.2 Run the debug app from a scratch folder (`target\deactivation-test`, wake-word mode, Russian model, activation "братан" and deactivation "Отбой братан, Заканчивай, Братан всё") and join the test channel. Then switch to the English model without setting phrases. Verify in the log:
  - the English defaults "bro" and "peace out" are in effect;
  - "wake word ready" with both phrases and no unknown word;
  - the gate settings with the deactivation phrase;
  - ticks reaching the wake worker.

  The report says that speaking in the channel needs a person

  Note (2026-11-17), done in part, so left open: the debug app (build 1.0.1+62cfbfe.dirty,
  then 1.0.2) ran from `target\deactivation-test` with the Russian model, activation
  "братан" and deactivation "Отбой братан, Заканчивай, Братан всё". The log had "settings:
  the call's transcript ends a session on: Отбой братан, Заканчивай, Братан всё" and "wake
  word ready; activation: братан; deactivation: Отбой братан, Заканчивай, Братан всё", with
  no unknown word ("всё" known through "все"). A save of `wake.model` to the English model
  through the window's DevTools port gave "settings: the call's transcript ends a session
  on: peace out" and "wake word ready; activation: bro; deactivation: peace out"; the
  fields showed "bro" and "peace out" (`target\deactivation-test-shots\config-english-defaults.png`),
  and `settings.json` kept no English entry. Not done: joining the test channel, which
  needs someone to type `/join`, so the ticks reaching the worker in the app were not
  watched (the unit tests cover them), and nobody spoke in the channel.
- [ ] 7.3 With the user, on the tagged release build that contains this change: in a real channel, with a second person and a game running, start sessions with "братан" and say each of "Отбой братан", "Заканчивай" and "Братан всё" over the voice's answers, at least ten times in all. Verify that each ends the session at the first try with the falling tone. Then read in the log, for each:
  - the source;
  - the decision delay;
  - the bridged gaps;
  - any near miss or lag line.

  The report lists what was not covered

  Note (2026-11-17): the user's step, not done. The version is 1.0.2 on the branch for
  that build.

## Workflow follow-up

- When the user says to commit: one commit on `change/voice-recognition-rework` with the code,
  the docs and this change archived (`openspec/changes/archive/<date>-voice-recognition-rework`,
  delta specs merged into `openspec/specs/`), after the checks of 7.1.
- When the user says to merge: `git merge --no-ff --log change/voice-recognition-rework` into
  `main`, then delete the branch.
- For the build of 7.3: bump the version past 1.0.1 in `apps/desktop/Cargo.toml`,
  `crates/voicebro-core/Cargo.toml` and `apps/desktop/tauri.conf.json`, tag `v<version>` on
  `main`, and build with `scripts\build-release.ps1`.
