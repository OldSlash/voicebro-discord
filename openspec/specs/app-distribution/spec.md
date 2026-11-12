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
keep `settings.json`, `logs\`, `assets\`, `models\`, `codex-workspace\`, `webview-data\`
and the stored Discord token.

#### Scenario: Update
- **WHEN** version 1.0.1 is installed over 1.0.0 with phrases set and models downloaded
- **THEN** the app starts with the same settings, models and token and shows 1.0.1

### Requirement: Uninstall removes the data only when asked
The uninstaller SHALL remove the program files and shortcuts. With "Delete the application
data" checked it SHALL also remove `settings.json`, `logs\`, `assets\`, `models\`,
`codex-workspace\`, `webview-data\`, the stored Discord token and the then empty folder;
unchecked, it SHALL leave them.

#### Scenario: Data kept
- **WHEN** the user uninstalls without checking the box
- **THEN** the folder still holds the settings, logs and models, and a reinstall into it starts with them

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
its own data in `~\.codex`, and Codex installed through winget lives where winget puts it.

#### Scenario: A run leaves nothing outside
- **WHEN** the app runs for a minute, connects to Discord, downloads the wake model after the OK and is closed
- **THEN** no file or folder created by the app is found in `%TEMP%`, `%APPDATA%` or `%LOCALAPPDATA%` outside the app's folder

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
Codex CLI, which the app needs but does not ship, with their licenses.

#### Scenario: Notices complete
- **WHEN** the notices file of a release is compared with the crates of the release build
- **THEN** every crate in the build appears with its license text, or with its license name and source link when it ships no license file

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

### Requirement: User guide
`README.md` SHALL say what VoiceBro does, what it needs, where to download it, a quick
start of at most five steps, its limitations, how to build it from source, what it is
built on and its license. `docs/user-guide.md` SHALL take a new user from nothing to a
first answer in detail: the Discord bot, installing, connecting, Codex and the ChatGPT
sign-in, wake phrases and other languages, daily use, settings, privacy and files,
updating, uninstalling and troubleshooting.

#### Scenario: A new user follows the guide
- **WHEN** a person without Codex, a Discord bot or VoiceBro follows the README's quick start and the user guide it links to
- **THEN** they reach a spoken answer in their voice channel without needing anything the two documents do not explain

#### Scenario: A developer builds from source
- **WHEN** a developer with Windows and none of the tools follows the README's Building from source section
- **THEN** they know which tools and versions to install, and the listed commands build, test and run the app

#### Scenario: Limitations before installing
- **WHEN** a person reads the README before downloading VoiceBro
- **THEN** they learn that the voice works only through OpenAI's Codex with a ChatGPT account, and that a high ping from their PC to the Discord voice server can make the voice stutter, with what to do about it

#### Scenario: What it is built on
- **WHEN** a reader wants to know which third-party software VoiceBro is made of
- **THEN** the README names its main components with links and licenses and says where the full list with every license text is

### Requirement: Release artifacts carry no wake files
The installer and the portable zip SHALL contain neither the Vosk library nor any wake
model; the app SHALL download them only after the user's OK.

#### Scenario: Release contents
- **WHEN** the installer of a release is unpacked and the zip is listed
- **THEN** neither contains `libvosk.dll`, a `vosk-model-*` folder or an `assets\` folder

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
