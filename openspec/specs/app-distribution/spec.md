# app-distribution Specification

## Purpose

Defines how VoiceBro reaches a user and leaves their machine: what a release contains, how
it is installed, updated and removed, and where the installed app keeps its files.

## Requirements

### Requirement: Installer for the current user
The installer SHALL install VoiceBro for the current user without administrator rights,
into `%LOCALAPPDATA%\VoiceBro` by default with the folder selectable, SHALL add a Start menu
entry, SHALL offer a desktop shortcut on its last page, and SHALL show its pages in Russian
on a Russian system and in English otherwise.

#### Scenario: Default install
- **WHEN** the user runs the installer and accepts the defaults
- **THEN** no administrator prompt appears, `voicebro.exe` is in `%LOCALAPPDATA%\VoiceBro`, and the Start menu has a VoiceBro entry

#### Scenario: Chosen folder
- **WHEN** the user installs into `D:\Games\VoiceBro` and starts the app
- **THEN** the app's settings, logs and downloads are created in `D:\Games\VoiceBro`

### Requirement: Updates keep the data
Installing a newer version over an installed one SHALL replace the program files and SHALL
keep `settings.json`, `owner-link.bin`, `consents.bin`, `logs\`, `assets\`, `models\`,
`codex-workspace\`, `webview-data\` and the stored Discord token.

#### Scenario: Update
- **WHEN** version 1.0.1 is installed over 1.0.0 with phrases set and models downloaded
- **THEN** the app starts with the same settings, models and token and shows 1.0.1

#### Scenario: Update keeps the owner
- **WHEN** a newer version is installed over one with an owner linked
- **THEN** the app starts with the same owner linked and no new code is needed

#### Scenario: Update keeps the consents
- **WHEN** a newer version with the same consent text version is installed over one with two members recorded
- **THEN** the app starts with both members recorded

### Requirement: Uninstall removes the data only when asked
The uninstaller SHALL remove the program files and shortcuts. With "Delete the application
data" checked it SHALL also remove `settings.json`, `owner-link.bin`, `consents.bin`,
`logs\`, `assets\`, `models\`, `whisper-models\`, `codex-workspace\`, `webview-data\`, the
stored Discord token and the then empty folder; unchecked, it SHALL leave them.

#### Scenario: Data kept
- **WHEN** the user uninstalls without checking the box
- **THEN** the folder still holds the settings, the owner link, the consent records, logs and models, and a reinstall into it starts with them

#### Scenario: Data deleted
- **WHEN** the user uninstalls with the box checked
- **THEN** the folder is gone and the Credential Manager has no `VoiceBro/discord_token`

### Requirement: Portable zip
Each release SHALL include a zip holding `voicebro.exe`, `README.md`,
`docs\user-guide.md` with the images the two documents show, `LICENSE` and
`THIRD-PARTY-NOTICES.txt`. Extracted into any folder the user can write to, the app SHALL
run from there and keep its files next to `voicebro.exe`, without an installer.

#### Scenario: Run from a folder
- **WHEN** the user extracts the zip into `D:\Tools\VoiceBro` and starts `voicebro.exe`
- **THEN** `settings.json` and `logs\` appear in `D:\Tools\VoiceBro` and nothing is registered with Windows

#### Scenario: Guide offline
- **WHEN** the user opens `README.md` from the extracted zip and follows its link to the user guide
- **THEN** `docs\user-guide.md` and the images both documents show are in the extracted folder

### Requirement: The app's folder must be writable
At start, before anything else, the app SHALL check that it can write to its folder. When
it cannot, it SHALL show a message that names the folder and says to install or extract
VoiceBro into a folder the user can write to, and SHALL exit without creating anything.

#### Scenario: Started from a protected folder
- **WHEN** `voicebro.exe` is started from a folder the user cannot write to, such as one under Program Files
- **THEN** a message names the folder and what to do, the app exits, and no window, log or settings file is created

### Requirement: Files stay in the app's folder
The app SHALL create files only inside its own folder. Outside it, only the stored Discord
token in the Windows Credential Manager SHALL be written by the app; the user's Codex keeps
its own data in `~\.codex`, Claude Code its own in `~\.claude` and `~\.claude.json`, and
OpenCode its own under `~\.local\share\opencode`, and an engine installed through winget
lives where winget puts it.

#### Scenario: A run leaves nothing outside
- **WHEN** the app runs for a minute, connects to Discord, downloads the wake model after the OK and is closed
- **THEN** no file or folder created by the app is found in `%TEMP%`, `%APPDATA%` or `%LOCALAPPDATA%` outside the app's folder

#### Scenario: OpenCode's configuration
- **WHEN** the bot joins a channel with OpenCode as the engine
- **THEN** the configuration the app gives OpenCode is in the app's folder, and nothing the app wrote is in `~\.config\opencode`

### Requirement: Release artifacts
A release SHALL be built from `main` at the tag `v<version>`, where the version is the one
the app shows. It SHALL consist of the installer, the portable zip and `SHA256SUMS.txt`
with the SHA-256 of both. The release build SHALL refuse an untagged commit, a dirty tree
or a version mismatch; a trial build SHALL be marked as such and SHALL never be handed over.

#### Scenario: Tagged build
- **WHEN** the release script runs on `main` at `v1.0.0` with a clean tree and the app's version 1.0.0
- **THEN** the installer, the zip and `SHA256SUMS.txt` are written, and the checksums match the files

#### Scenario: Untagged build
- **WHEN** the release script runs on a commit without a matching tag
- **THEN** it stops with a message and writes no release artifact

### Requirement: Third-party notices
The installer and the zip SHALL include `THIRD-PARTY-NOTICES.txt` naming, with license and
license text, every crate built into `voicebro.exe`, the native libraries compiled or linked
into it from inside those crates, and the installer's own components. It SHALL name Vosk
and every model of the wake catalog, which the app downloads with the user's OK, and the
Codex CLI, Claude Code and OpenCode, which the app can use but does not ship, with their
licenses.

#### Scenario: Notices complete
- **WHEN** the notices file of a release is compared with the crates of the release build
- **THEN** every crate in the build appears with its license and a license text. A crate whose package ships no license file carries the standard text of one of its licenses, with its authors named as the copyright holders

#### Scenario: A crate without a license text
- **WHEN** the notices script meets a crate in the build whose package ships no license file and none of whose licenses has a standard text in the repository
- **THEN** the script stops with an error that names the crate and writes no notices file

#### Scenario: Native code inside a crate
- **WHEN** a crate compiles or links a library under its own license into `voicebro.exe`, as `libopus_sys` does with Opus and `webview2-com-sys` with Microsoft's WebView2 loader
- **THEN** the notices carry that library's name, version, license and full license text

#### Scenario: Installer components
- **WHEN** the notices of a release are read
- **THEN** they name NSIS with its license text, Tauri's NSIS plug-in with its license, and Microsoft's WebView2 Runtime installer, which the installer downloads only when the runtime is missing

#### Scenario: A license text goes missing
- **WHEN** a license file the notices take a native library's or the installer's text from does not exist, for example after a dependency update moved it
- **THEN** the notices script stops with an error that names the file and writes no notices file

#### Scenario: Wake models named
- **WHEN** the notices of a release are read
- **THEN** they name the Vosk library and each of the ten catalog models with its license: the nine Apache 2.0 models in one group, and the Czech model with its MIT license text and copyright holder

#### Scenario: Catalog and notices differ
- **WHEN** the wake catalog has a model the notices script does not list, or the script lists one the catalog does not have
- **THEN** the notices script stops with an error that names the model and writes no notices file

#### Scenario: The engines named
- **WHEN** the notices of a release are read
- **THEN** they name the Codex CLI (Apache-2.0), Claude Code (proprietary, under Anthropic's terms) and OpenCode (MIT) as installed separately by the user

#### Scenario: Voices named with what they come from
- **WHEN** the notices of a release are read
- **THEN** every catalog voice appears with the license of its data and, when its model card names a model it was fine-tuned from, that model and the license of its data, as the card states them

#### Scenario: CC BY voices attributed
- **WHEN** the notices of a release are read
- **THEN** every catalog voice whose data is under a CC BY license carries its creator, the dataset or voice it comes from, a link to the source and the license name with its link

#### Scenario: The phonemizer's license text
- **WHEN** the notices of a release are read
- **THEN** they carry the full GPL-3.0 text for piper-phonemize and the espeak-ng data that the Piper and Kokoro archives hold, with the link to their source

### Requirement: User guide
`README.md` SHALL say what VoiceBro does, what it needs, where to download it, a quick
start of at most five steps, its limitations, how to build it from source, what it is
built on and its license. `docs/user-guide.md` SHALL take a new user from nothing to a
first answer in detail: the Discord bot, installing, connecting, the engine (Codex with the
ChatGPT sign-in, Claude Code with its sign-in, or OpenCode with a provider), the three
assistant modes, wake phrases and other languages, daily use, every slash command,
settings, privacy and files, updating, uninstalling and troubleshooting.

#### Scenario: A new user follows the guide
- **WHEN** a person without Codex, a Discord bot or VoiceBro follows the README's quick start and the user guide it links to
- **THEN** they reach a first answer in their voice channel, spoken or posted, without needing anything the two documents do not explain, the downloads of the Whisper mode included

#### Scenario: A developer builds from source
- **WHEN** a developer with Windows and none of the tools follows the README's Building from source section
- **THEN** they know which tools and versions to install, and the listed commands build, test and run the app

#### Scenario: Limitations before installing
- **WHEN** a person reads the README before downloading VoiceBro
- **THEN** they learn that the realtime voice works only through OpenAI's Codex with a ChatGPT account, that Claude Code and OpenCode answer only in the Whisper modes (spoken or text answers), and that a high ping from their PC to the Discord voice server can make the voice stutter, with what to do about it

#### Scenario: What it is built on
- **WHEN** a reader wants to know which third-party software VoiceBro is made of
- **THEN** the README names its main components with links and licenses, each Opus codec compiled into the app among them, and says where the full list with every license text is

#### Scenario: A Claude subscription is personal
- **WHEN** a person reads the README or the user guide before choosing Claude Code
- **THEN** they learn that a Claude subscription may be used only by its holder, with links to Anthropic's Consumer Terms and Claude Code's legal page, that VoiceBro's "Only my Discord account" mode lets only the holder's voice through, and that an API key or a cloud provider falls under Anthropic's Commercial Terms instead

#### Scenario: OpenCode's web search
- **WHEN** a person reads the privacy part of the user guide
- **THEN** they learn that with OpenCode the web searches the model makes go to OpenCode's search service, a company other than the model's provider

#### Scenario: Every setting described
- **WHEN** the settings of the Settings and Config tabs of a release's window are compared with the user guide
- **THEN** each one is described under its label as the window shows it, in its tab and card, with its choices or range and its default

#### Scenario: Every command described
- **WHEN** a person reads the commands part of the user guide
- **THEN** it describes `/join`, `/leave`, `/status`, `/link`, `/optin`, `/optout` and `/privacy`, and says which of them also work in a direct message to the bot

#### Scenario: What the app keeps and passes on
- **WHEN** a person reads the privacy part of the user guide
- **THEN** they learn:
  - which files hold what, and which are encrypted;
  - that the log names people and the owner's account id;
  - that the verbose log and the chat history hold speech and answers;
  - that a post in the Whisper modes starts with the transcribed question;
  - what an uninstall with "Delete the application data" leaves;
  - that closing the app or switching the engine in a channel leaves OpenCode's conversation in OpenCode's storage;
  - that Codex runs with the operator's own Codex configuration;
  - that each engine's provider handles the data under its own terms

#### Scenario: Screenshots match the release
- **WHEN** a reader compares the screenshots in the README and the user guide with the window of the release
- **THEN** the tabs, cards, labels and build version they show are those of the release

### Requirement: Release artifacts carry no wake files
The installer and the portable zip SHALL contain no Vosk library, no wake model, no Whisper
runtime or model, and no voice engine or voice. The app SHALL download them only after the
user's OK.

#### Scenario: Release contents
- **WHEN** the installer of a release is unpacked and the zip is listed
- **THEN** neither contains `libvosk.dll`, a `vosk-model-*` folder, `whisper.dll`, a `ggml*.dll`, a `.bin` model, a voice engine library or an `assets\` folder

### Requirement: Application icon
`voicebro.exe`, the main window's title bar and taskbar button, the installer, the
uninstaller and the Start menu and desktop shortcuts SHALL show the VoiceBro icon. The
icon SHALL come in 16, 24, 32, 48, 64 and 256 px, generated from one source image kept in
the repository, and SHALL be transparent outside its rounded square.

#### Scenario: The program file and the window
- **WHEN** the user starts `voicebro.exe` from the portable zip or from an install
- **THEN** the file in Explorer, the window's title bar and its taskbar button show the
  VoiceBro icon, not a generic or placeholder icon

#### Scenario: The installer and the shortcuts
- **WHEN** the user runs the installer and opens the Start menu or the desktop after it
- **THEN** the installer file, its window, the uninstaller and the `VoiceBro` shortcuts
  show the VoiceBro icon

#### Scenario: Small sizes and dark backgrounds
- **WHEN** Windows shows the icon at 16 px on a dark taskbar or at 32 px on a light
  Explorer view
- **THEN** the area outside the rounded square is transparent and no dark rim shows
  around the square

### Requirement: Icon refresh after an install
After every install the installer SHALL tell the Windows shell that `voicebro.exe` changed
and that icons must be read again, so no icon cached for an earlier build at the same path
stays on screen.

#### Scenario: An install over a build with another icon
- **WHEN** the user installs over an earlier VoiceBro whose `voicebro.exe` had another
  icon, at the same path
- **THEN** right after the install, without a sign-out or a manual cache refresh,
  `voicebro.exe` in Explorer, the Start menu and desktop shortcuts and the taskbar
  button show the icon of the newly installed file

### Requirement: The Opus port carries the Opus license
The notices SHALL list opus-rs, the Opus port built into `voicebro.exe`, with the Opus
license text and the Xiph.Org copyright list, because its code is derived from the Opus
reference code. opus-rs ships that text as its own `COPYING`, which the notices take like
any crate's license file.

#### Scenario: opus-rs
- **WHEN** the notices of a build with opus-rs are read
- **THEN** opus-rs is listed with its `COPYING`, the Opus BSD-3-Clause license with the Xiph.Org copyright list

#### Scenario: No other port
- **WHEN** the notices of this change's build are read
- **THEN** they list no ropus or mousiki and no separate Opus text for a port

### Requirement: Notices for the Whisper mode's downloads
`THIRD-PARTY-NOTICES.txt` SHALL name, with their licenses, everything the Whisper mode
downloads after the user's OK:

- whisper.cpp with its MIT text, and NVIDIA's CUDA runtime and cuBLAS libraries of the GPU
  runtime with their terms;
- each Whisper catalog model and the Silero VAD model;
- the voice engine and its runtime;
- each catalog voice with its license and the attribution it requires.

#### Scenario: Notices of a release
- **WHEN** the notices file of a release is read
- **THEN** it names whisper.cpp with its MIT text, the CUDA runtime and cuBLAS with their terms, each Whisper catalog model, the Silero VAD model, the voice engine and its runtime, and each catalog voice with its license and attribution

### Requirement: The notices script checks the Whisper and voice catalogs
The notices script SHALL stop with an error when the Whisper or voice catalog has an entry
that the script does not list, or the script lists one that the catalog does not have.

#### Scenario: Catalog and notices differ
- **WHEN** a Whisper model is added to the catalog without a notices entry
- **THEN** the notices script stops with an error naming the model and writes no notices file

### Requirement: The privacy policy template matches the app
`docs/PRIVACY-POLICY-TEMPLATE.md` SHALL describe what the app does with people's data as
the code does it: what is sent to whom in each assistant mode and engine, and what is
kept on the operator's PC and for how long. It SHALL also say what stays after a person
withdraws and what the engines' providers may do with the data under their own terms. Its
labels SHALL be those of the window.

#### Scenario: The operator fills in the template
- **WHEN** an operator reads the template to publish their policy for 1.3.0
- **THEN** it has a variant for each of the three assistant modes and says that the log names people, that a post starts with the transcribed question, that audio held when a person withdraws can still be sent, that OpenCode's conversation can stay in OpenCode's storage, and that the engines' providers may use the data under their terms, model training included

#### Scenario: Labels as in the window
- **WHEN** a sentence of the template names a setting
- **THEN** it uses the label and choice the window shows, such as Activation → "Always listening" and "Speaker names"

### Requirement: Release notes
Each release SHALL come with release notes in English, written before the build. They
SHALL list what changed for users since the previous public release and what an update
from it does to the settings, and SHALL give what the release needs and its files with
their SHA-256 and how to check them. They SHALL also give the known limitations.

#### Scenario: Notes for 1.3.0
- **WHEN** the release notes of 1.3.0 are read
- **THEN** they cover every change merged between `v1.0.1` and `v1.3.0` that a user can notice, name `VoiceBro_1.3.0_x64-setup.exe`, `VoiceBro_1.3.0_x64-portable.zip` and `SHA256SUMS.txt`, and show for both files the same SHA-256 that `SHA256SUMS.txt` holds

#### Scenario: Settings carried over
- **WHEN** a user of 1.0.1 reads the notes before updating
- **THEN** they learn that their settings, models and token are kept, and where the settings that moved now are
