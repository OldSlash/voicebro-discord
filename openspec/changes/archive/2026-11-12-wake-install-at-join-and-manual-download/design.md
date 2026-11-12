# Design

## Context

See proposal.md, Why. The relevant state of `main` (`d1248e9`):

- `apps/desktop/ui/main.js` has one install-dialog controller (`renderInstall`,
  `installOpen`, `installClose`, `chooseWakeModel`). `start()` sets `install.armed` once
  the model list and the settings are loaded. After that every status runs
  `renderInstall`, which opens the dialog in wake-word mode and `needs_download`: `start`
  on the first status after arming, `mode` when the kind turns into `needs_download` from
  another kind, and `join` when the count of `discord.voice_channels` grows. `choice` comes
  from the model list's change hook. `install.voiceCount` and `install.lastKind` are
  updated on every status, armed or not.
- The bot connects to Discord at launch whenever a token is stored
  (`settings/mod.rs`: "The bot always connects at start"), and `/join` starts Codex through
  `VoiceHost::prepare_join`. The window lives as long as the app (no tray, no hide), so
  only a WebView reload makes `start()` run while the bot is already in a channel.
- `DownloadItem` (`wake/mod.rs`) has `label`, `host` (taken from the asset's URL) and
  `bytes`. The catalog's `Asset` has `label`, `url`, `sha256`, `directory`, `marker` and
  `size`. The library's `Asset` is `vosk::LIBRARY`.
- `assets::ensure` already uses an archive that is in the folder: when
  `assets\<file name from the URL>` exists it skips the download, checks the SHA-256
  (a mismatch deletes the file and fails), extracts with `tar.exe`, removes the archive and
  checks the marker. `is_installed` checks only the marker file, so an archive the user
  extracted counts as installed at the next `configure`. Neither path is in a spec or a
  test.
- The loader (`WakeManager::start`) logs "downloading the <label> (first use only)" for
  every missing asset before calling the fetch. The state is `Downloading {label,
  percent}` only through the download's progress callback. Hashing and extracting
  publish no state: after a download the window shows 100 % until the load starts, and an
  archive found in the folder leaves the state at `needs_download` until it is extracted.
- The window opens folders through commands (`open_models_folder`, `open_log_folder`)
  that run `explorer.exe`. The only external page it opens is the ChatGPT sign-in, through
  `rundll32 url.dll,FileProtocolHandler` after a host check (`assistant/codex/mod.rs`).
  The main window is built in code (`WebviewWindowBuilder` in `main.rs`) and has no
  new-window handler. Tauri is 2.12.5; its builder offers `on_new_window`.
- Checked on 2026-11-12: `https://github.com/alphacep/vosk-api/releases/tag/v0.3.45`
  answers 200, and its asset list (`/releases/expanded_assets/v0.3.45`, which the page
  loads) names `vosk-win64-0.3.45.zip`. `https://alphacephei.com/vosk/models` answers 200
  and names the archives of all ten catalog models.

## Goals / Non-Goals

**Goals:**

- The dialog comes up when the bot is used or the wake settings are changed, never because
  the app started.
- A user who downloads the files in the browser can install them through the app without
  the app going online, and the window says so in words that do not say "downloading".
- Every link opens in the default browser, and the window never shows a web page or opens
  a window of its own for a link. One mechanism does this; the user asked on 2026-11-12 for
  this one rule and nothing more.

**Non-Goals:**

- Watching `assets\` for new files. A file the user puts there is used when Download is
  pressed or at the next start.
- A file picker or drag-and-drop for archives.
- Changing the Download button's label when archives are found in the folder. The line
  next to the links says what the button does with them.
- Any change to the consent model: an agreement still covers one model, is kept in memory,
  and a press on Download is still the only thing that starts a fetch or an unpack.

## Decisions

### D1. The join is the bot's start; the start trigger goes

`renderInstall` loses the `start` branch. On the first status after arming, a bot that is
already in a voice channel counts as a join (`join`). This only happens after a WebView
reload, and the bot is in use then. After that, `join` opens the dialog when the
voice-channel count grows, `mode` when the kind turns into `needs_download`, and `choice`
comes from the model list, all as today. A join while the dialog is open still only
updates it.

Alternatives: keep the start trigger behind a setting (rejected: the user asked for the
behaviour, not an option); open the dialog at `discord_connect` (rejected: the bot
connects at launch, so that is the app's start again).

### D2. Each asset names its page; `DownloadItem` carries it

`Asset` gets `page: &'static str`, the official page a person downloads the archive from:
`https://github.com/alphacep/vosk-api/releases/tag/v0.3.45` for `vosk::LIBRARY` and
`https://alphacephei.com/vosk/models` for every catalog model. `DownloadItem` gets `file`
(the archive's file name, the last segment of `url`, the same name `ensure` looks for)
and `page`. Local models have no page: they need only the library.

Why a page and not the archive's URL: the user asked for a place to go and look, and a
browser download from the page is what a cautious user trusts. The exact file name next
to the link says what to pick on a page that lists many files.

Alternative: derive the page from the host. Rejected because the GitHub release page is
not derivable from the download URL in a stable way, and an explicit field can be tested.

### D3. One handler hands every link to the default browser

The window renders each link as `<a href="<page>" target="_blank" rel="noopener">`. With
`target="_blank"`, WebView2 turns every way of following the link (a click, Ctrl+click,
Shift+click, a middle click, Enter, the context menu's "open in new window") into a
new-window request instead of a navigation of the app's page. The main window's builder
gets one `on_new_window` handler. For an `http` or `https` URL it opens the URL in the
default browser through `rundll32 url.dll,FileProtocolHandler`, the way the sign-in
opens, and logs it. For any other address it opens nothing and logs it, because the shell
would run a file path handed to it. It denies the WebView window in every case. The page
has no click handler for links, and there is no command and no list of allowed addresses.

Before relying on it, the apply step reads `on_new_window` and what it calls in the Tauri
2.12.5 source. If the check in the real app (D8) finds a way that still navigates the
app's page or opens a window, the work stops and the user is told. No second mechanism
is added without the user's OK.

`open_assets_folder` mirrors `open_models_folder` with `WakeManager::assets_dir()` and
creates the folder when it is missing.

Alternatives: a click handler in the page that calls a command (rejected: the context menu
goes around it, and modifier clicks need more handlers); a list of allowed pages and a
separate navigation guard (proposed first, dropped on 2026-11-12 at the user's request: one
layer); `tauri-plugin-opener` (rejected: a new dependency).

### D4. One manual-download part for the dialog and the notice

`main.js` gets one renderer that fills a container from the items: a lead line "Rather
download them yourself?", then for each item "`<file>` from `<page link>`", the line "Put
the .zip files into the assets folder as they are, then press Download: VoiceBro checks
each against its pinned SHA-256 and unpacks it without going online, and downloads only
what is not there.", and an "Open assets folder" button. `index.html` gets the container
in the dialog (after the total, before the progress) and in the notice (after the list,
before the Download button). Both containers are hidden while the state is `downloading`
or `installing` and shown again in `needs_download` and after a failure. The dialog's
focus still starts on Not now (or Cancel). The links come after it in the tab order only
because they come later in the markup, so a stray Enter still means Not now.

Texts are English like the rest of the UI.

### D5. A new `Installing {label}` state while an archive is checked and unpacked

`assets::ensure` reports two phases through its callback: the download percentage, as
today, and "unpacking", which it calls once before the hash check, whether the archive was
just downloaded or found in the folder. The loader turns the second phase into
`WakeState::Installing { label }` (`installing` in the status). The window shows
"Checking and unpacking the <label>…" in the dialog and in the notice, and the Status tab
shows "checking and unpacking the <label>". The dialog treats `installing` like
`downloading`: one button, Hide, and it closes when the state reaches `ready`,
`no_phrases` or `loading`. The model list is reloaded after either kind ends. The loader
logs "using <file> from the assets folder" instead of "downloading the <label>" when the
archive is already there, and `ensure`'s mismatch error names the file and its route:
"<file> (downloaded | found in the assets folder) does not match the pinned SHA-256
(<digest>); it was deleted".

Why a state and not only a text change: without it, an archive found in the folder
leaves the state at `needs_download` while it is unpacked, so the notice offers Download
again and the Status tab still asks for the OK. Downloads benefit too: the seconds of
hashing and extracting after 100 % now have a name.

Alternative: report 100 % for a found archive (rejected: the window would say
"Downloading" to the very user who chose not to download).

### D6. The mock and the dialog check follow

`apps/desktop/ui-dev/mock.js`: items carry `file` and `page`; `open_assets_folder` is
recorded in `window.__mock.calls`; a new
`window.__mock.placeArchives(ids, library)` makes the next download of those files go
through `installing` only, with no `downloading` step. `check-wake-dialog.mjs` changes the
scenario "start on a clean folder" to expect no dialog and the notice with the links, and
"the bot joins" to expect the dialog without a prior Not now. It adds scenarios for the
links (each `href` is its page and its `target` is `_blank`; a click leaves `location.href`
unchanged and the dialog open, the mock's browser opening a tab instead), "Open assets
folder", the part hidden during a download and shown after a
failure, archives put there by hand (`installing` text, never "Downloading"), a reload
while the bot is in a channel, the notice's links, and the focus on Not now.

### D7. Tests in the core

In `wake/assets.rs`, with the test asset's URL pointing at a `file://` path that does not
exist, so any download attempt fails the test:

- an archive put into the folder is verified, extracted and removed, and the progress
  callback sees "unpacking" and no percentage;
- a wrong archive is deleted, nothing is extracted, and the error names the file and
  "found in the assets folder";
- an extracted folder counts as installed without an archive.

In `wake/mod.rs`, with the fake fetch: a manager with the library archive in the folder
and the model missing fetches only the model; `DownloadItem` has the file name and page
of the library and the English model. In `catalog.rs`: every entry's `page` is the models
page and its `url` ends in `<id>.zip`. In `main.rs`: the handler's decision, tested
without `rundll32`: `https://alphacephei.com/vosk/models` and an `http` address go to the
browser, `file:///C:/Windows/notepad.exe`, `C:\Windows\notepad.exe` and `ms-settings:` do
not.

### D8. Checks in the real app

A release build started with an empty `VOICEBRO_DATA_DIR` under `target\` and the WebView2
debugging port: no dialog at start in wake-word mode; the notice shows both files with
their links; switching to always listening and back opens the dialog (`mode`). With
`vosk-win64-0.3.45.zip` and `vosk-model-small-en-us-0.15.zip` copied into that folder's
`assets\` from the corpus folder, Download makes the log say "using … from the assets
folder" twice and no "downloading" line, and the state ends in `no_phrases`. A wrong file
with the English model's name ends in the error with the file name. Through the debugging
port a link is followed with a click, Ctrl+click, Shift+click, a middle click and Enter:
each opens the page once in the default browser, the log names it, the page stays on the
app and the window count in `/json` does not grow. The context menu's "open in new window"
is checked by hand on the VoiceBro window, or reported as not checked. The join trigger in
a real channel needs the user's Discord client (see Risks).

## Risks / Trade-offs

- [The dialog now comes up while the user is in Discord, often in a game, and the window
  may be behind it] → The Status tab and the notice keep saying what is missing, and the
  dialog waits in the window. The window is not brought to the front: a game must not lose
  focus. The `/join` reply in Discord stays as it is (proposal: not part of this change).
- [The join trigger cannot be run by an agent in Discord: only a member in a voice channel
  can run `/join`] → It is checked against the mock, including a reload with the bot in a
  channel. A real `/join` either runs in the user's Discord client through computer use
  after the user's OK, or is the user's step and the report says so.
- [A cautious user extracts the archives instead of dropping the `.zip`] → An extracted
  folder with the marker counts as installed at Download or the next start (scenario
  "Extracted by hand"), but no hash is checked then. The line next to the links asks for
  the `.zip` as it is, which is the checked route.
- [A user puts a newer archive with another name, or another model, into the folder] → The
  file is not the expected name, so it is ignored and the expected one is downloaded on
  Download. The link line shows the exact file name.
- [A wrong file with the expected name is the user's own download and gets deleted] →
  The same rule as for a download: a file that fails the pinned hash is never kept. The
  error names it and says it was found in the folder.
- [GitHub or alphacephei.com moves a page] → The scenario "Every page offers its archive"
  is a live check in the tasks. A changed page is a one-line change in the catalog.
- [One handler only: a way of following a link that WebView2 treats as a navigation of the
  app's page rather than a new window would get around it] → `target="_blank"` makes the
  usual ways new-window requests, and D8 tries each of them in the real app. If one gets
  around, the work stops and the user decides (D3).

## Migration Plan

No data migration. The status gains the `installing` kind and `DownloadItem` gains two
fields, both read only by the window of the same build. The change ships in a normal
tagged release. Rolling back means installing the previous tag.
