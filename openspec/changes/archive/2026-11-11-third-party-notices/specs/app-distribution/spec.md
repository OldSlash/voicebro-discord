# Spec Delta

## MODIFIED Requirements

### Requirement: Third-party notices
The installer and the zip SHALL include `THIRD-PARTY-NOTICES.txt` naming, with license and
license text, every crate built into `voicebro.exe`, the native libraries compiled or linked
into it from inside those crates, and the installer's own components. It SHALL name Vosk
and its models, which the app downloads with the user's OK, and the Codex CLI, which the
app needs but does not ship, with their licenses.

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
