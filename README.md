<h1 align="center">VoiceBro</h1>

<p align="center">
  <b>An AI voice assistant for your Discord voice channel.</b><br>
  Say a wake phrase, ask out loud and hear the answer while you keep playing.
</p>

<p align="center">
  <a href="https://github.com/OldSlash/voicebro-discord/releases/latest">Download</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/user-guide.md">User guide</a> ·
  <a href="#building-from-source">Building from source</a> ·
  <a href="#license">License</a>
</p>

<p align="center">
  <img src="docs/images/chat.png" width="820" alt="The Codex chat tab: two players ask questions and the assistant answers, once after a web search">
</p>

## Features

- **Hands-free.** An offline wake word starts a session, so you never leave the game.
  English and Russian are built in; other languages are one download away.
- **A real voice.** ChatGPT's voice through the official Codex CLI, with your own ChatGPT
  account. No API key.
- **Looks things up.** Questions that need research go to Codex, which can search the web,
  and the answer comes back by voice.
- **Built for gaming.** Mute with a system-wide hotkey, from the app or from a button in
  Discord. While nobody uses `/join`, nothing of the assistant runs.
- **Keeps to itself.** The wake word is recognized on your PC and nothing reaches the
  assistant between sessions. The app keeps all its files in its own folder.

## Requirements

- Windows 10 or 11, 64-bit
- A Discord server where you can add a bot
- A ChatGPT account with access to Codex

## Download

Get the latest version from [Releases](https://github.com/OldSlash/voicebro-discord/releases/latest):

| File | For |
|---|---|
| `VoiceBro_<version>_x64-setup.exe` | Installing with a Start menu entry. No administrator rights needed. |
| `VoiceBro_<version>_x64-portable.zip` | Running from any folder without installing. |

> [!NOTE]
> Releases are not code-signed yet, so Windows SmartScreen may warn about an unknown
> publisher. Choose **More info**, then **Run anyway**. `SHA256SUMS.txt` in each release
> lets you check the files.

## Quick start

1. **Create a Discord bot** in the [Developer Portal](https://discord.com/developers/applications):
   copy its token and invite it to your server with the scopes `bot` and
   `applications.commands` and the permissions View Channels, Connect, Speak, Send Messages
   and Embed Links.
   ([Step by step](docs/user-guide.md#1-create-the-discord-bot))
2. **Connect it.** Start VoiceBro, paste the token into **Config → Discord bot token** and
   press **Save changes**.
3. **Set up Codex.** On the **Status** tab press **Install Codex** if it is missing, then
   **Sign in to Codex** and finish the ChatGPT sign-in in your browser.
4. **Choose a wake phrase.** In **Settings → Assistant**, pick the wake model for your
   language, enter a phrase such as `hey bro`, and press **Download** when VoiceBro asks for
   the speech model.
5. **Talk.** Join a voice channel, type `/join`, say your phrase and ask a question.

The [user guide](docs/user-guide.md) covers each step in detail, as well as other
languages, settings, privacy, updating, uninstalling and troubleshooting.

## Building from source

VoiceBro is written in Rust with [Tauri 2](https://v2.tauri.app). It builds on Windows.

### Prerequisites

| Tool | Version | Notes |
|---|---|---|
| [Rust](https://rustup.rs) | stable, 1.89 or newer | The default MSVC toolchain (`x86_64-pc-windows-msvc`). |
| [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) | 2022 | The **Desktop development with C++** workload, which brings MSVC, a Windows SDK and CMake. |
| [Tauri CLI](https://v2.tauri.app/reference/cli/) | 2.x | Only for the installer: `cargo install tauri-cli --version "^2" --locked` |

Opus is built with CMake. The Build Tools install CMake but do not put it on `PATH`; do that
in the PowerShell session you build from:

```powershell
$env:PATH += ";C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
```

### Build, test and run

```powershell
git clone https://github.com/OldSlash/voicebro-discord.git
cd voicebro-discord
cargo build                     # the whole workspace
cargo test                      # the tests; no Discord account or Codex needed
cargo run -p voicebro-desktop   # start the app (debug build)
```

The app keeps its files next to its executable, here `target\debug\`. Set
`VOICEBRO_DATA_DIR` to another folder to keep test data apart.

### Installer and portable zip

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1 -Trial
```

This writes a test installer, the portable zip and their checksums to
`target\release\dist-trial\`. Without `-Trial`, the script builds a release only from
`main` at a tag `v<version>` that matches the app's version.

### Repository layout

| Path | Contents |
|---|---|
| `crates/voicebro-core` | The app's core: Discord voice, the wake word, the ChatGPT call through Codex, settings. No UI. |
| `apps/desktop` | The Tauri app: the window, its commands and the installer configuration. |
| `apps/probe` | Developer probes that check the external APIs live. |
| `scripts/` | The release build, the third-party notices and test helpers. |
| `docs/` | The architecture description and the user guide. |
| `openspec/` | Specifications and the record of changes. |

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before changing code.

## License

VoiceBro is released under the [MIT License](LICENSE).
Copyright (c) 2026 Aleksandr Bogdanov.

It builds on other open-source work:

- The Rust crates compiled into `voicebro.exe` are listed with their licenses in
  `THIRD-PARTY-NOTICES.txt`, which comes with every release (`scripts\third-party-notices.ps1`
  writes it).
- The [Vosk](https://github.com/alphacep/vosk-api) speech recognizer and its models
  (Apache-2.0) are downloaded by the app, and only after you agree.
- [Codex CLI](https://github.com/openai/codex) (Apache-2.0) is installed separately and stays
  under its own license.

VoiceBro is an independent project and is not affiliated with Discord or OpenAI.
