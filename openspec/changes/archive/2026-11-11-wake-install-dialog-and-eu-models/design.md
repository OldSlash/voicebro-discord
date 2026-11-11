# Design

## Context

See proposal.md, Why. The relevant state of `main` (`a01ceb1`):

- `WakeManager::configure` (`wake/mod.rs`) returns `NoPhrases` as soon as the selected
  model has no phrases. Only with phrases does it start the loader thread, and that thread
  checks the files: `engine.assets(&source)` (the Vosk library plus the catalog asset, or
  the library alone for a local model) filtered by `assets::is_installed`. Missing files
  without consent end in `NeedsDownload { items }`. So a fresh install shows "no phrases
  configured" and nothing about the missing files.
- Consent is in memory: `consent: Option<(engine, model)>`, set by `download()`. The
  method works only in the `NeedsDownload` or `Error` state and always means the selected
  model. The Tauri command `wake_download` takes no argument.
- The window (`apps/desktop/ui/main.js`) has the inline notice `#wake-download`, drawn from
  the wake state by `renderWakeDownload`. Every Settings control saves on `change` through
  `saveControl`. `trackOpenList` keeps a list from being rebuilt while it is open. A Discord
  or wake event makes the window call `refresh()` at once, and the status carries
  `discord.voice_channels` per guild.
- `migrate_v1` (`settings/store.rs`) keeps the model a version-1 file names and gives the
  Russian model to a file that names none, with or without phrases. Version 1 could only
  name the Russian model.
- `catalog.rs` has two entries. The URLs follow
  `https://alphacephei.com/vosk/models/<id>.zip`. The archive extracts to a folder named
  after the id, and `am/final.mdl` proves a complete extraction. Hashes and sizes were
  measured from a download, because alphacephei publishes neither.
- Windows on this machine has speech voices for English (David, Zira, Mark) and Russian
  (Irina, Pavel) only. The probe `rtc-call` already makes a real call to the Codex voice
  and writes what it says as a 48 kHz mono WAV, but its instruction is fixed (the "Banana"
  canary). The corpus test (`vosk_corpus_when_assets_are_provided`) reads 16 kHz WAVs only;
  the app's own 48 → 16 kHz downsampler is in `audio/dsp.rs`.
- The user's failing case was an installed trial over a folder that held a version-1
  `settings.json` from the 0.1.0 build `dc28357`.

## Goals / Non-Goals

**Goals:**

- The wake state tells the truth about missing files in every case, so both the window and
  the Status tab can rely on it.
- One dialog element, driven by the wake state, serving start, join, mode switch and model
  choice.
- Eight new catalog entries whose hash, size, folder and loadability were checked on this
  machine before they are written.

**Non-Goals:**

- Bundling anything into the installer or the zip.
- Swedish, Greek, Ukrainian or any model above about 60 MB; a separate languages tab.
- Downloading a model the user has not selected, or ahead of time.
- Telling the Discord channel that the wake word is not ready; the `/join` reply stays as
  it is.
- Installing Windows language packs for speech voices: that is a system-wide install. The
  Codex voice speaks the new languages instead (D10).

## Decisions

### D1. The file check comes before the phrase check, synchronously

In wake-word mode `configure` resolves the selected model (`ModelSource::from_id`), lists
its assets and checks them on disk in the calling thread, before it looks at the phrases.
Missing files without consent give `NeedsDownload` at once. Missing files with consent
start the loader thread, which downloads them and then loads the model if the model has
phrases, or sets `NoPhrases` if it has none. When everything is on disk, the existing
order applies: no phrases gives `NoPhrases`, phrases start the load. An unknown model or a
missing local folder stays the loader's `Error`.

The missing files are computed once in `configure` and handed to the loader thread, so
the engine is asked for its asset list once per start. A load that ends after a newer
`configure` arrived (a generation counter tells) applies the newest settings instead of
publishing its own result. Before this change a `configure` during a load was dropped;
with the check now in `configure`, a choice made while the previous model loads would
otherwise be overwritten by that load's "ready".

Why synchronous: the check is two file stats. The window reads the state right after a
save and at its first status read, so the start dialog and the model-choice flow need no
waiting or polling. Alternative: keep `NoPhrases` and add a `missing` list to the status.
Rejected, because the same fact would have two signals, and the Status tab and the notice
already understand `NeedsDownload`.

### D2. Consent and download name the model

`WakeManager::download(model_id)` and the command `wake_download { model }` take the model
id. The call is refused with a message unless that id is the selected model and the state
is `NeedsDownload` or `Error`. It records the consent for `(engine, model)` and
reconfigures. The window saves the choice first and then asks for the download, so a
settings change in between cannot download a model the user did not agree to.

Alternative: allow a download of any model in the background. Rejected: it adds a second
download pipeline and state the user never asked for.

### D3. `wake_requirements(model)` answers "what would this choice need"

A new `WakeManager::requirements(model_id) -> Result<Vec<DownloadItem>, String>` and the
command `wake_requirements { model }` return the missing items (library and/or model) for
any catalog or local id, without touching the settings. The model-choice hook uses it.

Alternative: put the library state into `ModelInfo`. Rejected: the library is shared by all
models, and the window would have to combine the two itself.

### D4. One `<dialog>` in the window

`index.html` gets one `<dialog id="wake-install">` with a title, a text line, the item list
(label, MB, site, then a total), a progress line and two buttons. It is opened with
`showModal()`, so it is modal, traps focus and closes on Escape (Escape means Not now, or
Cancel). The focus starts on Not now (Cancel), not on Download: the window may come to the
front while the user types elsewhere, and a stray Enter must not agree to a download (the
first trial run on 2026-11-11 started a download that way when Download had the focus). It uses the theme tokens, like the cards do. `main.js` owns one small controller:

- `open(reason, model, items)`, where the reason is `start`, `join`, `mode` or `choice`. A
  call while the dialog is open only updates it.
- Opening triggers:
  - `start`: the first status after load, in wake-word mode, in `needs_download`.
  - `mode`: the wake state moves from any other kind into `needs_download` while no
    choice dialog is open. This covers the switch to wake word, a changed model through
    Config, and a removed asset.
  - `join`: the count of `discord.voice_channels` grows while the state is
    `needs_download`.
  - `choice`: the `wake-model` change hook (D5).
- While the state is `downloading` the dialog shows "Downloading the <label>: N %" and one
  button, Hide. When the state leaves `downloading` for `ready` or `no_phrases` the dialog
  closes and the model list reloads (as today). On `error` it shows the message with Try
  again (`wake_download` again) and Close.
- Not now, Close, Hide and Escape store nothing. The state stays `needs_download`, so only
  a new trigger opens the dialog again, and the inline notice stays as it is.

Texts: title "The wake word is not installed"; body "VoiceBro listens for your wake phrase
with the <display name> model, which is not on this PC yet. Download it now?"; the items as
in the notice, then "<N> MB in total"; buttons "Download" and "Not now". In the choice case
the second button is "Cancel", with the line "Cancel keeps <stored model's display name>."

### D5. The model choice is checked before it is saved

The `change` listener of `#wake-model` calls `wake_requirements(value)` before
`saveControl`. An empty answer saves as today. Otherwise the list stays marked busy (so no
status event rebuilds it) and the dialog opens with the `choice` reason. Download runs
`saveControl` and then `wake_download { model }`. A rejected save shows the field error,
closes the dialog and downloads nothing. Cancel puts the select back on
`stored.wake.model`, marks the field clean and frees the list. While the dialog is open the
list is not reachable with the mouse (the dialog is modal), but a script or a keyboard path
can still change it: the hook then puts it back and saves nothing, and the dialog's
Download ignores a click while it is hidden or disabled. The first real-app run showed
why: a script changed the list under a running download and clicked the hidden button,
and the dialog showed "is not the selected wake model" next to the old download. The session-mode switch
keeps saving at once; its dialog comes from the `mode` trigger, as the user asked to be
told, not stopped.

### D6. Migration: no phrases means English

`migrate_v1` sets `wake.model` to `catalog::default_entry().id` when the document has no
non-empty phrase list, and keeps today's behaviour when it has phrases. Files that are
already version 2 are not touched: a version-2 file with the Russian model and no phrases
cannot be told apart from the user's own choice.

### D7. Catalog entries are measured, not copied from the page

For each new model the apply step downloads `<id>.zip` once with `curl.exe` into
`target\wake-corpus\assets\` (the folder the corpus test already uses, kept out of git),
records the byte size and SHA-256, and lists the archive with `tar -tf` to confirm the top
folder name and `am/final.mdl`. If a model differs (the `-rhasspy` Czech model is the one
to watch), its entry carries its real folder and marker. The entries:

| Id | Language | Display name | License |
|---|---|---|---|
| `vosk-model-small-cs-0.4-rhasspy` | `cs` | Vosk small Czech 0.4 | MIT |
| `vosk-model-small-nl-0.22` | `nl` | Vosk small Dutch 0.22 | Apache-2.0 |
| `vosk-model-small-fr-0.22` | `fr` | Vosk small French 0.22 | Apache-2.0 |
| `vosk-model-small-de-0.15` | `de` | Vosk small German 0.15 | Apache-2.0 |
| `vosk-model-small-it-0.22` | `it` | Vosk small Italian 0.22 | Apache-2.0 |
| `vosk-model-small-pl-0.22` | `pl` | Vosk small Polish 0.22 | Apache-2.0 |
| `vosk-model-small-pt-0.3` | `pt-BR` | Vosk small Portuguese (Brazil) 0.3 | Apache-2.0 |
| `vosk-model-small-es-0.42` | `es` | Vosk small Spanish 0.42 | Apache-2.0 |

They go after English and Russian in this order (by English language name). The asset
labels follow the existing ones ("Vosk German model").

### D8. A live check that every catalog model loads

A new test in `wake/vosk`, `catalog_models_load_when_enabled`, is skipped silently unless
both `VOICEBRO_VOSK_DIR` and `VOICEBRO_WAKE_CATALOG=1` are set, so a corpus run does not
load ten models. It loads every catalog model from that folder through the real library
(a missing one fails), checks that the model knows a greeting word of its language
(`hello`, `привет`, `ahoj`, `hallo`, `bonjour`, `hallo`, `ciao`, `cześć`, `olá`, `hola`)
and the corpus phrases of D10, checks that it does not know a nonsense word (so the
vocabulary check is real, also for the flat-layout Portuguese model), and prints the load
time of each. The word list lives in the test, not in the catalog: the catalog carries no
phrases.

### D9. Notices follow the catalog

`scripts\third-party-notices.ps1` lists the catalog models in its Vosk section, grouped by
license: one Apache 2.0 text, and the MIT text of the Czech model, which is kept in
`scripts\notices\`, taken from the archive or from the source the models page links. It
reads the `directory: "vosk-model-…"` lines of `catalog.rs` and stops with the differing
id when they do not match its own list.

### D10. Spoken samples in the new languages come from the Codex voice

A new probe command, `codex-say <trigger.wav> <out.wav> <sentence>`, reuses the `rtc-call`
call (both now run one `run_call` with their own instruction and answer check). It sends a
short English trigger ("Go ahead.", made by the corpus script with David) with an
instruction to read the given sentence aloud verbatim, in its language, and say nothing
else. It writes the voice's audio as a
48 kHz mono WAV and prints the voice's transcript. It fails when the transcript does not
contain the sentence's words, so a paraphrase never becomes a test file. `VOICEBRO_PROBE_VOICE`
picks the voice. The canary call of `rtc-call` stays as it is.

`scripts\make-wake-corpus.ps1` gets a `-Codex` switch that makes four files per new
language into `target\wake-corpus\<lang>\`: the activation phrase at the start of a
question, the activation phrase at the end of a sentence, a plain sentence without either
phrase, and a sentence ending in the deactivation phrase. That is 32 calls, a small share
of the account's voice budget (the handoff tests made 20). The voices alternate between
`juniper` and `cove` by language, so both voices are heard. The phrases are the test's own,
checked against each model's vocabulary before the files are made:

| Language | Activation | Deactivation |
|---|---|---|
| Czech | ahoj kámo | konec |
| Dutch | hallo maat | tot ziens |
| French | salut pote | au revoir |
| German | hallo kumpel | tschüss |
| Italian | ciao amico | arrivederci |
| Polish | cześć stary | do widzenia |
| Portuguese | olá amigo | tchau |
| Spanish | hola amigo | adiós |

A phrase with an unknown word is replaced by a known one in the table and in the script
before any call is made.

The corpus test learns to read 48 kHz mono WAVs and passes them through the app's own
`audio::dsp` downsampler, the path Discord audio takes. The Codex files are then used as
recorded, and the 16 kHz Windows-voice files work as before.

### D11. The installed app, in the user's failing case

The trial installer is installed silently into `target\install-test\VoiceBro` (as in
`LOCAL-TESTING.md`). Beforehand the folder gets a version-1 `settings.json` built from the
defaults of commit `dc28357` (read with `git show`, not written from memory). The installed
`voicebro.exe` then starts with the WebView2 debugging port. The English model must be
selected and the dialog open at start. An update install over the same folder must keep
the settings, and a silent uninstall must leave the data.

### D12. `/join` in the real test guild

Only a member in a voice channel can run `/join`, and the bot cannot run it for itself. The
join trigger is therefore checked in the user's Discord desktop client through computer
use, and only after the user's OK at that step, since it acts as the user: join `General`
in the test guild, run `/join`, read the VoiceBro window through the debugging port (the
dialog must be open), run `/leave` and leave the channel. Without the OK it stays the
user's step. Speech in the channel still needs a person: the agent cannot feed audio into
the user's microphone.

### D13. A repeatable check of the dialog against the mock

`apps/desktop/ui-dev/check-wake-dialog.mjs` (tracked) starts the mock server, opens it in
a headless Edge with a DevTools port and a profile under `target\`, and runs every scenario
of the two app-window requirements and of "Choosing a model that is not installed" through
`window.__mock`. It prints one line per scenario and exits non-zero on the first failure.
It uses Node 24's built-in `WebSocket` and needs no package. `LOCAL-TESTING.md` and
`AGENTS.md` give the command.

## Risks / Trade-offs

- [The Codex voice reads a sentence with an accent or a slip] → `codex-say` checks the
  transcript; a clip whose transcript does not match is made again, at most twice, and
  the language's result notes it.
- [Synthetic speech is cleaner than game chat] → The corpus also runs with noise 300, as
  for English and Russian; the real channel with a second person stays the acceptance
  test.

- [The dialog on every start in wake-word mode until the user downloads] → This is what
  the user asked for. Not now closes it, and always-listening mode never shows it.
- [libvosk 0.3.45 cannot load a newer model (`es-0.42`, `pt-0.3`)] → D8 runs before the
  window work. A model that does not load is left out, and the user is told before the
  change goes on.
- [A model is replaced at the same URL] → The hash mismatch already stops with the file's
  name. A new pin is a one-line change.
- [The Czech model's MIT text cannot be found with a copyright holder] → Stop and ask the
  user whether to drop Czech. The notices must not carry an incomplete license.
- [Portuguese is the Brazilian model with weak accuracy (32.6 % WER on Common Voice)] →
  The list says "Portuguese (Brazil)" and the user guide says to prefer short, distinct
  phrases.
- [An older build reading a settings file that names a new catalog model] → It loads (load
  does not validate) and its wake engine shows "unknown model". Selecting English or
  Russian fixes it. No migration is needed.
- [The `join` trigger depends on the status reaching the window] → A Discord event calls
  `refresh()` at once, so the dialog opens within the event's round trip, not after the
  5 s poll.

## Migration Plan

No data migration beyond D6. The change ships in a normal tagged release. Rolling back
means installing the previous tag; settings stay readable (see Risks).
