# Decisions of 2026-11-17

The user asked an agent to carry out the two open OpenSpec changes one after the other,
`voice-recognition-rework` first and `whisper-mode` second, without asking questions, and to
write down how each open point was decided. This file is that record. Each change's own
`design.md` (archived with the change) holds the same decisions marked **[decided alone]**
next to the decision they belong to, with the measurements.

## How the work is split

- **Two branches, merged one after the other.** `change/voice-recognition-rework` starts at
  `main`. `change/whisper-mode` starts at the tip of `change/voice-recognition-rework`, not
  at `main`: the Whisper mode reuses the phrase matching, the default phrases and the
  tick-counted feed of the first change (its design says "whichever change lands first
  defines the function, and the other reuses it"), and both changes touch the gate, the
  settings and the window. Branched from `main`, the second merge would conflict in those
  files and duplicate the matching. Merged in order, `--no-ff` as `AGENTS.md` says, each
  merge is clean.
- **One commit per branch**, as the git flow asks: the code, the docs and the change
  archived. No AI attribution line, nothing pushed, nothing merged.

## voice-recognition-rework

What it does: the deactivation phrase works while the voice answers, phrases may be several
words between commas, "ё" and "е" match, and every wake model comes with default phrases.

### Decided while building

1. **Trailing silence after 300 ms of absence, counted in receive ticks (D1), kept although
   two short phrases are confirmed 100–120 ms later.** Measured with the real Russian
   model: with songbird-like holes of 6 ticks every 0.5 s, the old worker (zeros in the
   holes) missed the phrase in 1 to 2 of 5 phases on 5 of the 9 Russian samples ("отбой"
   heard as "от бобби"); the new worker heard exactly what the uninterrupted stream heard
   in every run. At the end of a sentence 7 of 9 samples are confirmed at the same moment
   as before, but "Ладно, отбой." and "Заканчивай." 100 and 120 ms later (400 ms after the
   speech instead of 280–300 ms), above the 0.1 s the task named. The task said to bring
   that to the user; since this run allows no questions, I kept the design: a lower
   threshold would put zeros back into songbird's refill gaps, which is the fault being
   fixed, and the call's transcript ends the session on its own clock.
2. **The people's transcript is read from the app-server's `thread/realtime/transcript/delta`
   with `role: "user"`.** The Codex CLI 0.167.0 schema has the `role` field, so the data
   channel was not needed (D2).
3. **The falling tone needed no fix.** A test plays an answer, deactivates mid-answer and
   runs the Codex session's two flushes: the whole earcon plays and nothing of the answer
   follows. The silent stops of the report were therefore not the app's deactivation.
4. **The default phrases after the corpus check** (D7). The table of the plan was checked
   with the Codex voice speaking every language; defaults heard in fewer than 3 of 4 runs
   were replaced with the best candidate (66 candidate recordings):

   | Model | Activation | Deactivation | Replaced |
   |---|---|---|---|
   | English (US) | bro | peace out | — |
   | Russian | братан | отбой | — |
   | Czech | brácha | čau | kámo/kámoš ("samo"), čus ("sil s") |
   | Dutch | maat | doei doei | doei ("dooi") |
   | French | mon frère | à la prochaine, a la prochaine | frérot ("raison"), tchao ("tiens") |
   | German | bruder | bis dann | tschau ("schau", "ciao") |
   | Italian | fratello | arrivederci | — |
   | Polish | ziomek | do widzenia | nara ("nora") |
   | Portuguese (Brazil) | irmão | até mais | mano ("humano"), tchau ("chão") |
   | Spanish | hermano | hasta luego | compa ("tampa"), chao |

   Final corpus: 140 of 144 runs right, no false hit, every default heard in at least 3 of
   its 4 runs. Czech "čau" is also a greeting; it is the only Czech candidate that passed,
   and it only ends a running session.
5. **French gets both spellings** of its farewell, because the small French model writes
   "a la prochaine" without the accent when the phrase is said alone. The matching form
   folds only "ё", as designed.
6. **Words with an apostrophe or a hyphen** ("don't", "по-моему") count as known to the
   model when it knows the whole word, its folded form, or each piece, so such a phrase is
   not dropped as unknown; matching splits them the same way on both sides.
7. **Version-1 settings files** now keep an empty list as empty when they are migrated,
   instead of letting it fall back to the model's default.
8. **New log lines** beyond the plan: the gate's phrase list for the call's transcript at
   start, at each join and on each change ("the call's transcript ends a session on: …"),
   so a report shows which phrases were in effect.
9. **The decision delay handed to the gate** is now the recognizer's part plus the time the
   audio waited in the queue, so the replay after an activation starts at the phrase's
   real end when the worker runs behind.
10. **Version 1.0.2** for every crate, so a build from `main` after the merge can be told
    apart.

### Not done here

- Task 7.3, the acceptance in a real Discord channel with a second person and a game: the
  user's step.
- Task 7.2 only in part: the debug app was started in a scratch folder with the Russian
  model and the user's three deactivation phrases ("wake word ready" with all of them, no
  unknown word, the gate's transcript list in the log), then switched to the English model
  without phrases ("bro" and "peace out" in effect and shown in the fields). Joining the
  test channel needs someone to type `/join`, so the ticks reaching the worker in the app
  were not watched; the unit tests cover them.

### The call's transcript as the second source, measured live

Thirty real calls (three rounds of five per phrase) said the phrase 1.5 s into the voice's
answer. Two decisions came out of them:

- **The voice is silenced as soon as the transcript holds the phrase**, also while the gate
  still waits 300 ms to see whether the last word goes on ("stop" + "watch"). If it does
  go on, the voice plays again. Without this the wait let the reply through.
- **A turn that the call cut at a pause is read with the next one.** Codex ended a turn at
  the comma of "Отбой, братан" in 2 of 5 calls, so a two-word phrase never matched.

Result with the final rule: the voice was silenced before its reply in 14 of 15 calls for
"Goodbye." and in 8 of 15 for "Отбой, братан.". The task asked for 4 of 5 per phrase; the
Russian phrase misses it because Codex transcribed the two words 0.36–0.78 s after the
speech while its voice often began to reply after 0.27–0.63 s. In those calls the reply is
cut 10–250 ms after it starts, with the falling tone, instead of the voice answering the
phrase. The local wake engine (Vosk), fixed by point 1 above, remains the faster path. Task
5.6 stays open with this note.
