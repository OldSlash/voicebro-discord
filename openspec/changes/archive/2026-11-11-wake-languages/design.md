# Design

## Context

See proposal.md for the motivation. What the code does today:

- `wake/catalog.rs` has one `ModelEntry` (`vosk-model-small-ru-0.22`) with a `&'static`
  `Asset` (URL, SHA-256, directory, marker `am/final.mdl`, size); `default_entry()` is
  `ENTRIES[0]`. `wake/vosk/mod.rs` has the library asset `LIBRARY` (`vosk-win64-0.3.45`,
  15 MB). `WakeEngine::assets(entry)` returns `[LIBRARY, entry.asset]`;
  `WakeEngine::load(assets_dir, entry)` loads the library from
  `assets_dir/<LIBRARY.directory>` and the model from `assets_dir/<entry.asset.directory>`.
- `WakeManager::configure(settings)` (`wake/mod.rs`) stops the engine outside wake-word
  mode (`Off`) or without phrases (`NoPhrases`), sends new phrases to a running worker of
  the same model, and otherwise spawns `voicebro-wake-load`, whose `start()` calls
  `assets::ensure()` for every missing asset (download, hash, extract) and then loads. The
  download happens without asking.
- `WakeSettings` is `{ engine, model, activation_phrases, deactivation_phrases }`;
  `Settings::validate` checks the phrase overlap and that `wake.model` is in the catalog
  with the chosen engine. `CURRENT_VERSION` is 1; `SettingsStore::load` deserializes with
  defaults for missing fields and bumps the version, with no field moves.
- `Settings::with_patch` merges a patch key by key (objects recursively, arrays replaced).
  The window builds patches with `patchFor(path, value)`, splitting `data-path` on `.`;
  model ids contain dots (`…-0.22`), so a path through a model id cannot be a `data-path`.
- The window gets the catalog once through `wake_models` (`apps/desktop/src/main.rs`) and
  builds the engine and model lists from it (`buildWakeLists` in `main.js`).
- The corpus test `vosk_corpus_when_assets_are_provided` loads `catalog::default_entry()`
  from `VOICEBRO_VOSK_DIR`.

Facts checked on 2026-11-11: `vosk-model-small-en-us-0.15.zip` is 41,205,931 bytes with
SHA-256 `30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498`; it has no
`graph/words.txt`, its symbol table is inside `graph/Gr.fst` and holds `bro`, `brother`,
`buddy`, `hey` but not `bruh`. Windows here has the voices `Microsoft David Desktop` and
`Microsoft Zira Desktop` (en-US).

## Goals / Non-Goals

**Goals:**

- No download without the user's OK, enforced in the core, not only in the window.
- A model the user extracts into `models\` is selectable after a restart, with no hash or
  catalog entry needed.
- Evidence that "bro", "brother" and "hey bro" activate with the English model and that
  look-alikes do not, on synthetic speech; the real voice in a channel stays the user's
  check.

**Non-Goals:**

- Downloading other languages from inside the app.
- Several models at once, or detecting the speaker's language.
- Packing models into the installer (the user chose downloads on first use).
- Removing downloaded catalog models from the UI.

## Decisions

### D1. Phrases live in a map keyed by model id

`WakeSettings` becomes `{ engine, model, phrases: BTreeMap<String, ModelPhrases> }` with
`ModelPhrases { activation: Vec<String>, deactivation: Vec<String> }`, both
`#[serde(default)]`. `PhraseSet::from_settings` reads `phrases[model]` (empty when absent).
Keyed by model id, not by language: two English models (US and Indian) can hold different
phrases, which is what "each dictionary has its own phrases" means. A `BTreeMap` keeps the
file's order stable. The overlap check runs per model; the same phrase in two models is
fine.

Alternative considered: keep the flat lists as "the selected model's phrases" and store the
others aside. Rejected: a model switch would have to move lists around in one save, and a
failed save could lose them.

### D2. Version 2 migration happens on the JSON value

`CURRENT_VERSION` becomes 2. `SettingsStore::load` calls `migrate_v1(&mut Value)` before
deserializing when the stored version is below 2: it reads `wake.model` (or uses
`vosk-model-small-ru-0.22`, the default of version 1, and writes it into `wake.model`),
moves `wake.activation_phrases` and `wake.deactivation_phrases` into
`wake.phrases.<model>.activation/deactivation` when either is non-empty, and removes both
old keys. The Russian id is written literally in the migration, not taken from
`default_entry()`, which becomes English. The migrated document is written by the next
save (the start writes a file only when none exists), so until then each start migrates
the same version-1 file again, with the same result.

### D3. The catalog gains English; local models are a second source

The catalog gets `vosk-model-small-en-us-0.15` (display "Vosk small English (US) 0.15",
language `en-US`, Apache-2.0, marker `am/final.mdl`) as `ENTRIES[0]`, so
`default_entry()` is English; Russian stays as the second entry. English as the default of
a new settings file is the agent's choice for a public release; the user's own file keeps
Russian through the migration.

Local models: `wake/local.rs` scans `paths::models_dir(data_dir)` (`<data dir>\models`) once
at start (`Runtime::start` creates the folder first) and returns `LocalModel { folder,
language }` for each subfolder with `am\final.mdl` and `conf\mfcc.conf`; others are logged
as skipped. The language is the `xx` or `xx-yy` after `vosk-model-` (and an optional
`small-`) in the folder name, or empty. Ids are `local:<folder>`; a folder name with `/`,
`\`, `..` or `:` is skipped.

A model the manager loads is described by a small owned value:

```rust
pub enum ModelSource {
    Catalog(&'static ModelEntry),
    Local { folder: String },
}
```

`WakeManager` resolves `settings.wake.model` to a `ModelSource` (`ModelSource::from_id`:
catalog first, then a `local:` id). `WakeEngine::assets` takes the source (catalog: library
plus the model archive; local: the library only), and `WakeEngine::load(assets_dir,
model_dir)` takes the assets directory (where the engine keeps its library) and the model's
directory, so the engine no longer knows where a model came from. A missing local folder is
`WakeState::Error` with the folder named.

Validation stays pure: `Settings::validate` accepts a catalog id with the matching engine,
or `local:<plain folder name>` with engine `vosk`; whether the folder exists is the engine's
error at load, so the phrases of a temporarily missing model are not rejected.

### D4. Consent is a state of the wake manager, held in memory

A new state `WakeState::NeedsDownload { items: Vec<DownloadItem> }` with `DownloadItem {
label, host, bytes }`. In `start()`, when an asset is missing and the manager has no
consent for it, the loader thread publishes this state and ends; nothing touches the
network. `WakeManager::download()` (Tauri command `wake_download`) records consent for the
current `(engine, model)` and runs `configure` again with the last settings it was given
(kept in `Inner`); `start()` then downloads as today, with the same progress events. The
consent is not stored: once the files are on disk no question is asked again; after a
restart with files still missing, the notice comes back. `download()` is also the retry
after a failed download: it is accepted in the `NeedsDownload` and the `Error` state.

A model switch while a download runs does not cancel it (as today); the next `configure`
after the loader ends applies the new model.

Alternative considered: a confirmation dialog in the window that calls `save_settings`
only after the OK. Rejected: the engine would still download on any other path to the same
settings (a file edit, a restart), and the core would not enforce the rule.

### D5. The window: the model list, phrases of the selected model, the notice

- `wake_models` returns the catalog entries with `installed` (the marker exists) and
  `bytes`, and the local models with `source: "local"`. The display names carry no size;
  every size shown is decimal megabytes from the bytes, rounded (`wake::megabytes`), so
  the list, the notice and the log agree. The list's labels: "en-US — Vosk small English
  (US) 0.15 · downloaded", "ru — Vosk small Russian 0.22 · to download, 46 MB",
  "de — vosk-model-small-de-0.15 · models folder".
- The phrase textareas lose their `data-path`; they get `data-kind="phrases"` and a
  `data-list="activation|deactivation"`. Reading takes `stored.wake.phrases[model]?.[list]`,
  saving sends `{ wake: { phrases: { [model]: { [list]: value } } } }` built as an object,
  not through `patchFor`. Their labels read "Activation phrases — English (US)". A model
  change refills them from `stored` once the model save is accepted; the rule that a
  focused or edited field is not overwritten still holds.
- The notice sits under the Wake model control and renders from `status.wake.state` when
  its kind is `needs_download`: one line per item ("Vosk library — 15 MB from github.com"), a
  Download button calling `wake_download`, then the progress line from `downloading`, and
  the error with the button again from `error`.
- "Open models folder" next to the control calls a new `open_models_folder` command (the
  same shape as `open_log_folder`). The help line: "Other languages: extract a small Vosk
  model from alphacephei.com/vosk/models into the models folder and restart the app."
- `apps/desktop/ui-dev/mock.js` learns `wake.phrases`, the new `wake_models` fields,
  `wake_download` (walks the states `needs_download` → `downloading` → `ready`) and
  `open_models_folder`.

### D6. The corpus test names its model; test files stay in the project

`VOICEBRO_WAKE_MODEL` names the model folder inside `VOICEBRO_VOSK_DIR` (for example
`vosk-model-small-en-us-0.15`); without it the test uses the catalog's default, now
English. The commands in `AGENTS.md` and `LOCAL-TESTING.md` pass it explicitly for the
Russian corpus.

Test inputs live inside the project, not in a system temp folder: the English model and
the Vosk library for the tests go to `target\wake-corpus\assets\`, the WAVs to
`target\wake-corpus\en\` (both ignored by git with `target\`), made by a script
`scripts\make-wake-corpus.ps1` with David and Zira at 16 kHz mono, like the Russian ones
with Irina:

| File | Text | Expected with activation "bro, brother, hey bro", deactivation "goodbye" |
|---|---|---|
| `en-hey-bro` | "Hey bro, what time is it in London?" | Activate |
| `en-bro-start` | "Bro, how do I craft a sword?" | Activate |
| `en-brother` | "Brother, tell me a joke." | Activate |
| `en-bro-end` | "That was a good game, bro." | Activate |
| `en-lookalike` | "I broke my bow, he brought the brown boat, nice bros." | none |
| `en-plain` | "Let's go to the castle and wait for the others." | none |
| `en-goodbye` | "Thanks for the help. Goodbye." (inside a session) | Deactivate |

Each file runs with each voice, with and without the noise level 300. A miss or a false hit
is reported, not tuned away in this change; if "bro" alone turns out unreliable, the report
says so and the README (change `release-packaging`) recommends "hey bro" or "brother".

## Risks / Trade-offs

- [Synthetic speech is cleaner than a real voice over Discord] → the corpus only shows the
  phrases can be spotted; the user's check in the channel decides.
- [A one-syllable phrase can fire on similar sounds] → the look-alike files measure it;
  the help line already asks for words that do not come up in normal game talk.
- [A large local model (1–2 GB) loads for a long time and takes gigabytes of memory] → the
  help line and the README say to use small models; nothing stops a user who insists.
- [The settings migration is one-way] → stated in the proposal; the previous file's
  content survives inside the new structure.
- [Consent is not stored] → a user who declines sees the notice again after each restart
  while wake-word mode has phrases; switching to always mode removes it.

## Migration Plan

Loading a version-1 file migrates it (D2); the next save writes version 2. The user's own `settings.json` (Russian model, Russian phrases) keeps working without a
change. Rollback: restore the previous `settings.json` together with the previous build.
