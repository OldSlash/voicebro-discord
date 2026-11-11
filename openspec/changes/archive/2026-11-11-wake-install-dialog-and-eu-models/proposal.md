# Proposal

## Why

On 2026-11-11 the user installed the 1.0.0 trial build into a folder that still held a
`settings.json` from the 0.1.0 run of 2026-11-09. The window then had the Russian wake
model selected, a model that is not on the PC. Nothing said so until phrases were typed,
and even then the only sign was a notice inside Settings → Assistant. The change
`wake-languages` was meant to make a missing model obvious to the user; it did not. The
user also asked for more languages than English and Russian, at least the main EU
languages, chosen from the wake model list. The user decided on 2026-11-11:

- Nothing is bundled. The installer stays small (6.5 MB today); the Vosk library and every
  model download only after the user agrees.
- English is the default model.
- A dialog in the app says that the wake word's files are not installed and offers to
  install them, before the bot is used and whenever a missing model is chosen.

## What Changes

- **English is what a fresh start selects, also after a migration.** A version-1 settings
  file without phrases now gets the English model; only a version-1 file with phrases
  keeps the Russian model, so those phrases keep working. Version 1 only ever had the
  Russian model, so the model it names says nothing about the user's choice.
- **The install check no longer waits for phrases.** In wake-word mode the app checks at
  once whether the selected model and the Vosk library are on disk. Missing files put the
  wake engine into the download state even when no phrases are set yet; the "no phrases
  configured" state means the files are there. After the user agrees, the files download
  even without phrases, so that the phrase check against the model's vocabulary works
  when the user types them.
- **An install dialog.** A modal dialog in the window names the missing files, their sizes
  and sites, and offers Download or Not now. It opens when the window starts in wake-word
  mode with files missing, when the bot joins a voice channel while they are still
  missing, and when the user switches to wake-word mode. Download shows the progress in
  the dialog and closes it when the files are in place; a failure shows the error with
  Try again. Not now closes it and leaves the inline notice in the Assistant card.
- **Choosing a model that is not installed asks first.** Picking such a model in the Wake
  model list opens the same dialog before anything is saved. Download saves the choice and
  starts the download; Cancel puts the list back on the previous model and saves nothing.
- **Eight more languages in the catalog.** The small Vosk models of EU languages with a
  permissive license: German (`vosk-model-small-de-0.15`), French (`-fr-0.22`), Spanish
  (`-es-0.42`), Italian (`-it-0.22`), Portuguese (`-pt-0.3`, Brazilian), Dutch (`-nl-0.22`),
  Polish (`-pl-0.22`), all Apache 2.0, and Czech (`-cs-0.4-rhasspy`, MIT), 31 to 50 MB each,
  each with its URL, pinned SHA-256, size and license. English stays first and the default,
  Russian second, the rest by language name. Left out: Swedish (the only model is 289 MB),
  Greek (1.1 GB only), and the EU languages Vosk has no model for (Bulgarian, Croatian,
  Danish, Estonian, Finnish, Hungarian, Irish, Latvian, Lithuanian, Maltese, Romanian,
  Slovak, Slovenian). Other languages stay possible through the models folder.
- **"Built in" is no longer claimed.** The README, the user guide and the window's help say
  that the models are downloaded after the user's OK, not built in.
- **The notices name every catalog model.** `THIRD-PARTY-NOTICES.txt` lists all ten models
  with their licenses (with the MIT text for the Czech model), and the notices script
  stops when its model list and the catalog differ.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `wake-model-assets`: the download state no longer depends on phrases; consent and
  download per named model; the catalog of ten models (renamed from "Built-in models"); the
  local-model example uses a language outside the catalog.
- `wake-word-detection`: "no phrases configured" only when the selected model's files are
  on disk.
- `app-settings`: a version-1 file without phrases migrates to the English model.
- `settings-ui`: the wake model list with ten catalog models; choosing a model that is not
  installed opens the install dialog; the inline notice also without phrases.
- `app-window`: the install dialog and when it opens.
- `app-distribution`: the installer and the zip carry no wake files; the notices name every
  catalog model and its license.

## Impact

- `crates/voicebro-core/src/wake/catalog.rs` (eight entries, order), `wake/mod.rs` (the
  missing-file check in `configure` before the phrase check, `download` for a named model,
  a download without phrases, the requirements of any model), `settings/store.rs`
  (`migrate_v1`).
- `apps/desktop/src/main.rs` (a `wake_requirements` command, `wake_download` with the model
  id), `apps/desktop/ui/index.html`, `main.js`, `styles.css` (the dialog, the select hook,
  the help line), `apps/desktop/ui-dev/mock.js` (ten models, the requirements, a join).
- `scripts/third-party-notices.ps1` (every catalog model, the MIT text, the catalog check);
  `scripts/notices/` for the Czech model's license text if its archive ships none.
- Docs: `docs/ARCHITECTURE.md` (4.4 catalog and states, 4.6 migration, 4.8 the dialog),
  `README.md`, `docs/user-guide.md` (languages, the dialog, a local-model example outside
  the catalog), `AGENTS.md` (the Vosk stack line), `LOCAL-TESTING.md` (the assets of the
  new models).
- Tests: `apps/probe` gets `codex-say`, which has the Codex voice read a sentence in any
  language; `scripts\make-wake-corpus.ps1` makes spoken sets for the eight new languages
  with it; the corpus test reads 48 kHz WAVs through the app's downsampler; a new live test
  loads every catalog model; `apps/desktop/ui-dev/check-wake-dialog.mjs` runs the dialog
  scenarios against the mock; the installed trial is tested over a 0.1.0 settings file.
- No new dependencies. The release artifacts do not grow.
