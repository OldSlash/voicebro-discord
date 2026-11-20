## MODIFIED Requirements

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

## ADDED Requirements

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
