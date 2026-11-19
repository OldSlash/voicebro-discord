## MODIFIED Requirements

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

### Requirement: Third-party notices
The installer and the zip SHALL include `THIRD-PARTY-NOTICES.txt` naming, with license and
license text, every crate built into `voicebro.exe`, the native libraries compiled or linked
into it from inside those crates, and the installer's own components. It SHALL name Vosk
and every model of the wake catalog, which the app downloads with the user's OK, and the
Codex CLI, Claude Code and OpenCode, which the app can use but does not ship, with their
licenses.

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

#### Scenario: The engines named
- **WHEN** the notices of a release are read
- **THEN** they name the Codex CLI (Apache-2.0), Claude Code (proprietary, under Anthropic's terms) and OpenCode (MIT) as installed separately by the user

### Requirement: User guide
`README.md` SHALL say what VoiceBro does, what it needs, where to download it, a quick
start of at most five steps, its limitations, how to build it from source, what it is
built on and its license. `docs/user-guide.md` SHALL take a new user from nothing to a
first answer in detail: the Discord bot, installing, connecting, the engine (Codex with the
ChatGPT sign-in, Claude Code with its sign-in, or OpenCode with a provider), wake phrases
and other languages, daily use, settings, privacy and files, updating, uninstalling and
troubleshooting.

#### Scenario: A new user follows the guide
- **WHEN** a person without Codex, a Discord bot or VoiceBro follows the README's quick start and the user guide it links to
- **THEN** they reach a spoken answer in their voice channel without needing anything the two documents do not explain

#### Scenario: A developer builds from source
- **WHEN** a developer with Windows and none of the tools follows the README's Building from source section
- **THEN** they know which tools and versions to install, and the listed commands build, test and run the app

#### Scenario: Limitations before installing
- **WHEN** a person reads the README before downloading VoiceBro
- **THEN** they learn that the realtime voice works only through OpenAI's Codex with a ChatGPT account, that Claude Code and OpenCode answer only in the Whisper mode, and that a high ping from their PC to the Discord voice server can make the voice stutter, with what to do about it

#### Scenario: What it is built on
- **WHEN** a reader wants to know which third-party software VoiceBro is made of
- **THEN** the README names its main components with links and licenses and says where the full list with every license text is

#### Scenario: A Claude subscription is personal
- **WHEN** a person reads the README or the user guide before choosing Claude Code
- **THEN** they learn that a Claude subscription may be used only by its holder, with links to Anthropic's Consumer Terms and Claude Code's legal page, that VoiceBro's "Only my Discord account" mode lets only the holder's voice through, and that an API key or a cloud provider falls under Anthropic's Commercial Terms instead

#### Scenario: OpenCode's web search
- **WHEN** a person reads the privacy part of the user guide
- **THEN** they learn that with OpenCode the web searches the model makes go to OpenCode's search service, a company other than the model's provider
