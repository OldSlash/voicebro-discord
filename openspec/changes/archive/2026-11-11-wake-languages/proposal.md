# Proposal

## Why

The first public release needs the wake word to work for people who do not speak Russian.
Today the model catalog has one entry (`vosk-model-small-ru-0.22`), the activation and
deactivation phrases are one pair of lists for whatever model is selected, and the Vosk
library and model download without asking the first time wake-word mode has phrases. The
user decided on 2026-11-11: English and Russian are built in, each model has its own
phrases, nothing downloads without the user's OK, and other languages come from a models
folder that the app reads at start.

## What Changes

- **English in the catalog.** A second entry, `vosk-model-small-en-us-0.15` (40 MB, Apache
  2.0, SHA-256 pinned). English becomes the default model of a new settings file; existing
  files keep the model they name.
- **Phrases per model.** The settings hold the activation and deactivation phrases of each
  model under `wake.phrases.<model id>`; the engine spots the phrases of the selected model
  only. One model is active at a time. **BREAKING** for the settings file: schema version 2.
  A version-1 file's two lists move under the model it names (the Russian model when it
  names none, the default then); the old fields are removed.
- **Download only after the user agrees.** When the selected model or the Vosk library is
  missing, the engine stops in a new state that lists what is needed, from where and its
  size; the Assistant card shows this notice with a Download button, and the download
  starts only when the user presses it. Progress and errors stay as today.
- **A models folder for other languages.** `models\` next to the executable is created at
  start. Each subfolder holding a Vosk model (`am\final.mdl` and `conf\mfcc.conf`) is listed
  in the wake model control as `local:<folder>`, with the language taken from the folder
  name when it follows Vosk's naming. Such a model needs only the Vosk library. An "Open
  models folder" button and the help line say where to put a model, that small models are
  the ones to use and that a restart picks a new folder up.
- **The UI follows the selected model.** The phrase fields show and save the phrases of the
  model selected in the Wake model control, with its name in their labels; switching the
  model shows its own phrases (empty for a new one).
- **English is tested.** The Vosk corpus test can name the model to load. English corpus
  WAVs made with the Windows voices David and Zira check that "bro", "brother" and
  "hey bro" activate and that look-alikes ("broke", "brought", "brown", "bros") do not. The
  test files live inside the project (`target\wake-corpus\`, not in git). The Russian
  corpus runs again.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `wake-model-assets`: the download waits for the user's OK; the English catalog entry;
  local models from `models\`.
- `wake-word-detection`: the phrases come from the selected model's entry.
- `app-settings`: phrases per model, schema version 2 and its migration, validation of
  catalog and local model ids.
- `settings-ui`: the wake model list with local models and install state; the phrase fields
  follow the selected model; the download notice and the models folder button.

## Impact

- `crates/voicebro-core/src/wake/catalog.rs` (the English entry, the default), a new
  `wake/local.rs` (scan of `models\`), `wake/mod.rs` (`WakeState::NeedsDownload`, consent,
  local models, phrases of the selected model), `wake/vosk/mod.rs` (load from a model
  directory; the corpus test's `VOICEBRO_WAKE_MODEL`).
- `crates/voicebro-core/src/settings/mod.rs` and `store.rs` (`wake.phrases`, version 2,
  migration, validation), `paths.rs` (`models_dir`), `runtime.rs` (creates `models\`,
  passes the local models to the wake manager).
- `apps/desktop/src/main.rs` (`wake_models` with local entries and install state,
  `wake_download`, `open_models_folder`), `apps/desktop/ui/index.html`, `main.js`,
  `styles.css` (Assistant card), `apps/desktop/ui-dev/mock.js`.
- Docs: `docs/ARCHITECTURE.md` (1, 2, 4.4, 4.6, 4.8), `AGENTS.md` (stack line, wake corpus
  commands), `LOCAL-TESTING.md` (English voices and corpus, test files inside the project).
- No new dependencies. A build older than this change reading a version-2 file sees no
  phrases.
