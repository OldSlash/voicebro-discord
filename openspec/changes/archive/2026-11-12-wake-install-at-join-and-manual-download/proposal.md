# Proposal

## Why

Since `wake-install-dialog-and-eu-models` the install dialog for the wake word's files
opens as soon as the window starts in wake-word mode with files missing. On 2026-11-12 the
user asked for two things:

- The dialog should not come up when the app starts. It should come up when the bot is
  about to be used, that is when it joins a voice channel and the assistant starts, or
  while the user is changing the wake settings. In Settings the install button and the
  model-choice dialog are already there.
- A user who would rather not let the app download anything should get, next to the
  Download button, links to where the files come from, so they can download what they
  need themselves.

The bot connects to Discord whenever a token is stored, at launch, and Codex starts only at
`/join`. So "when the bot starts" means the join: the app's start is not a moment the
wake word is needed.

## What Changes

- **No dialog when the app starts.** The window starts quietly in wake-word mode even with
  files missing. The Status tab's Wake word line and the notice under Wake model in
  Settings → Assistant still say what is missing, as today.
- **The dialog opens when the bot joins a voice channel** in wake-word mode while files
  are missing, at every join, whether or not the user dismissed it before. This is the
  join trigger that exists today. Without the start trigger it is now the first time the
  dialog shows.
- **The settings paths stay.** Switching the activation to wake word with files missing
  opens the dialog, and choosing a model that is not on disk asks before saving, as today.
- **Download by hand.** The dialog and the Assistant card's notice list, for each missing
  file, the archive's file name and a link to the official page it is published on: the
  Vosk library's release page on GitHub (`github.com/alphacep/vosk-api`, release v0.3.45)
  and, for a model, the Vosk models page on alphacephei.com. The models are not on GitHub;
  alphacephei.com is where the Vosk project publishes them and where the app downloads them
  from today. Next to the links there is an "Open assets folder" button and one line: put
  the downloaded `.zip` files into that folder as they are, then press Download. VoiceBro
  then checks each file against its pinned SHA-256 and unpacks it without going online, and
  downloads only what is not there.
- **Links open in the default browser.** Every link in the window opens in the user's
  default browser, whichever button or key follows it. VoiceBro itself never shows a web
  page or opens a window of its own for a link. On 2026-11-12 the user asked for this one
  rule and nothing more: no list of allowed pages, no second guard.
- **A file put into `assets\` by hand is used.** The asset store already uses an archive
  that is in the folder instead of downloading it. This change makes that a specified
  behaviour with tests. A file whose hash does not match is deleted and the error names
  it, as for a download.

Not part of this change: telling the Discord channel at `/join` that the wake word is not
ready (the `/join` reply stays as it is), bringing the window to the front at a join, and
noticing files the user put into `assets\` without a press on Download or a restart.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-window`: the install dialog no longer opens when the window starts; it opens when
  the bot joins a voice channel and when the activation is switched to wake word, with
  files missing; the dialog offers the download by hand; every link in the window opens in
  the default browser.
- `settings-ui`: the download notice offers the download by hand.
- `wake-model-assets`: each missing file names its archive and the official page it is
  published on; an archive the user put into `assets\` is verified and unpacked on
  Download instead of downloaded.

## Impact

- `crates/voicebro-core/src/wake/catalog.rs` (each asset gets the page it is published on),
  `wake/vosk/mod.rs` (the library's page), `wake/mod.rs` (`DownloadItem` gets the archive's
  file name and the page), `wake/assets.rs` (tests for an archive placed by hand).
- `apps/desktop/src/main.rs` (command `open_assets_folder`; the main window hands every
  link to the default browser),
  `apps/desktop/ui/index.html`, `main.js`, `styles.css` (the start trigger removed; the
  manual-download part in the dialog and the notice), `apps/desktop/ui-dev/mock.js` and
  `check-wake-dialog.mjs` (the start scenario inverted, the join scenario without a prior
  Not now, the links, the button).
- Docs: `docs/ARCHITECTURE.md` (4.4 the asset store and `DownloadItem`, 4.8 the notice and
  the dialog's triggers), `docs/user-guide.md` (when the dialog appears, how to download by
  hand, the troubleshooting row), `README.md` (quick start step 4), and the dialog check's
  line in `LOCAL-TESTING.md`.
- No new dependencies. The installer and the zip do not grow.
