# VoiceBro

VoiceBro is a Windows app that puts an AI voice assistant into your Discord voice channel.
The people in the channel talk to it while they play: someone says a wake phrase, asks a
question, and the assistant answers out loud in the channel. The wake phrase is recognized
offline on your PC; the assistant is ChatGPT voice through the official Codex CLI with your
own ChatGPT sign-in, so no API key is needed.

This guide takes you from nothing to a first spoken answer.

## What you need

- Windows 10 or 11, 64-bit. The window uses the Microsoft Edge WebView2 Runtime, which
  Windows 11 has; the installer adds it on a Windows 10 PC without it.
- A Discord server where you may add a bot.
- A ChatGPT account that can use Codex. VoiceBro was tested with a Plus plan and Codex CLI
  0.167.0.
- About 650 MB of disk space: the app (about 20 MB plus its window data), the wake word
  recognizer and one language model (about 140 MB together) and Codex (about 370 MB).

## 1. Create your Discord bot

VoiceBro runs as your own bot. You create it once in Discord's developer portal
(checked against Discord's developer documentation on 2026-11-11; the portal may move
things around):

1. Open <https://discord.com/developers/applications>, press **New Application**, give it
   a name (the name people will see in the channel) and press **Create**.
2. Open **Bot** in the left sidebar. Under **Token**, press **Reset Token**, confirm, and
   copy the token. Keep it secret: anyone with it controls your bot. You will paste it
   into VoiceBro in step 3. VoiceBro needs no privileged gateway intent, so leave those
   switches off.
3. Open **Installation** in the left sidebar. Under **Installation Contexts** keep
   **Guild Install**. Under **Default Install Settings**, for Guild Install, add the scopes
   `bot` and `applications.commands`, and in the permissions that appear choose
   **View Channels**, **Connect** and **Speak**.
4. Copy the **Install Link** from the same page, open it in your browser, choose
   **Add to server**, pick your server and confirm.

## 2. Install VoiceBro

Download the latest release from the repository's Releases page. There are two files:

- `VoiceBro_<version>_x64-setup.exe`, the installer. It installs for your Windows user
  only, without administrator rights, into `%LOCALAPPDATA%\VoiceBro` unless you choose
  another folder, and adds VoiceBro to the Start menu.
- `VoiceBro_<version>_x64-portable.zip`, the same app without an installer. Extract it
  into any folder you can write to and start `voicebro.exe`.

Choose a folder you can write to: VoiceBro keeps all its files next to `voicebro.exe` and
refuses to start, with a message saying so, from a folder like `C:\Program Files`.

**"Windows protected your PC".** The builds are not code-signed yet, so Windows SmartScreen
warns about an unknown publisher. Press **More info**, then **Run anyway**. To make sure
the file is the one published, compare its checksum with `SHA256SUMS.txt` from the same
release:

```powershell
Get-FileHash .\VoiceBro_1.0.0_x64-setup.exe -Algorithm SHA256
```

## 3. Connect the bot

Start VoiceBro and open the **Config** tab:

1. Paste the bot token into **Discord bot token** and press **Save changes**. The token is
   stored in the Windows Credential Manager, never in a file; the field shows only whether
   one is stored. The bot connects at once and from then on at every start.
2. Optional: put your server's ID into **Discord guild ID** and press **Save changes**.
   The commands then appear in that server at once; without it Discord may take up to an
   hour to show them. To copy the ID, turn on **Developer Mode** in Discord (User Settings
   → Advanced), right-click your server and choose **Copy Server ID**.

The status bar turns to **Connected** and the **Status** tab shows your server.

## 4. Install Codex and sign in to ChatGPT

The assistant runs through OpenAI's Codex CLI and your ChatGPT account. Open the
**Status** tab:

- **Codex is missing:** the tab says so and offers **Install Codex**. The button runs the
  Windows Package Manager (winget) in a console window you can see: package
  `OpenAI.Codex` from OpenAI's GitHub releases, about 200 MB to download. Answer winget
  there if it asks, press a key when it is done, and VoiceBro finds Codex by itself.
  Without winget, install Codex with Node.js (`npm install -g @openai/codex`) or download
  it from <https://github.com/openai/codex/releases> and set the path of `codex.exe` in
  **Config → Codex executable**.
- **Sign in:** press **Sign in to Codex**. Your browser opens the ChatGPT sign-in; finish
  it there, then press **Check Codex**. The **Codex account** tile shows the account. If
  you signed in to Codex before (for example in a terminal), **Check Codex** finds that
  sign-in; VoiceBro uses your normal Codex installation and its sign-in in `~\.codex`.

Nothing of Codex runs until someone uses `/join`, and it stops when the bot leaves.

## 5. Choose the wake word

Open **Settings → Assistant**:

1. **Activation:** *Wake word* (the default) starts a session when someone says an
   activation phrase and ends it at a deactivation phrase or after the auto-stop time;
   nothing reaches the assistant between sessions. *Always listening* sends everything
   said in the channel to the assistant while the bot is there.
2. **Wake model:** the language of your phrases. English (US) and Russian are built in.
   Each model keeps its own phrases, so you can switch languages and back.
3. **Activation phrases** and **Deactivation phrases** for the chosen model, one per line
   or separated by commas. Every word must be in the model's vocabulary; unknown words
   are listed under the field.
4. The first time a model is used VoiceBro shows what it has to download (the Vosk
   recognizer, 15 MB from github.com, and the model, 41 MB for English or 46 MB for
   Russian from alphacephei.com) and downloads nothing until you press **Download**.

Tips for good phrases:

- Two words are recognized more reliably than one short one. In our tests "hey bro" and
  "brother" always worked, while "bro" alone at the start of a sentence was sometimes
  heard as another word.
- Avoid words that come up in normal game talk, especially for the deactivation phrase.
- If the recognizer writes a phrase in two ways, add both: "goodbye" and "good bye".

## 6. Other languages

VoiceBro can use any Vosk model. Download a **small** model for your language from
<https://alphacephei.com/vosk/models> (for example `vosk-model-small-de-0.15` for German),
extract the zip into the `models` folder next to `voicebro.exe` (**Settings → Assistant →
Open models folder**), so that you get `models\vosk-model-small-de-0.15\am\final.mdl`, and
restart VoiceBro. The model appears in the **Wake model** list as "models folder". Use the
small models: the large ones take minutes to load and gigabytes of memory. Models you add
carry their own licenses; check them on the models page.

## 7. Everyday use

- Join a voice channel in Discord and type `/join` in a text channel of that server. The
  bot joins your channel and answers with a panel that shows its state and a **Mute GPT**
  button.
- Say an activation phrase and your question. The assistant answers in the channel. Say a
  deactivation phrase, or stay silent until the auto-stop, to end the session.
- `/status` shows what VoiceBro is doing; `/leave` makes the bot leave.
- **Mute** stops the session at once and sends nothing: the **Mute GPT** button in Discord,
  the button on the Status tab, or the hotkey **Ctrl+Alt+M**, which works while a game is
  in front (change it in **Settings → Hotkey**).
- The **Codex chat** tab shows what the assistant heard, said and looked up; the
  **Console** shows what the app does.

Every setting in the window has a line under it that says what it does, when a change
applies and its default. Settings save as soon as you change them.

## 8. Diagnostic switches

All off by default; nothing of them runs or is written while they are off:

- **Settings → Codex → Save chat history:** keeps the chat in
  `logs\codex-chat-<date>.log`.
- **Settings → Diagnostics → Verbose log:** writes what the recognizer heard, timings and
  call counters into the log file, for finding a problem.
- **Performance monitoring** and **Memory measurement:** the charts on the Performance
  tab and the memory reading in the status bar.

## 9. Where your files are

Everything VoiceBro creates is in its own folder, next to `voicebro.exe`:

| Item | What it is |
|---|---|
| `settings.json` | Your settings |
| `logs\` | The app log (at most about 2 MB) and, if you turned it on, the chat history |
| `assets\` | The Vosk recognizer and the models VoiceBro downloaded |
| `models\` | Models you added yourself |
| `webview-data\` | The window's browser data |
| `codex-workspace\` | An empty folder Codex works in |

Outside that folder are only the Discord token in the Windows Credential Manager (entry
`VoiceBro/discord_token`), your Codex installation and its sign-in in `~\.codex`, which
belong to Codex.

## 10. Updating

Run the new installer: it replaces the program and keeps your settings, models and token.
For the portable zip, replace `voicebro.exe` and keep the rest of the folder. Builds older
than 1.0.0 kept their files next to their own `voicebro.exe`; to keep your settings, copy
that `settings.json` into the new folder.

## 11. Uninstalling

Uninstall VoiceBro from **Windows Settings → Apps**. Check **Delete the application data**
(**Удалить данные приложения** on a Russian Windows) in the uninstaller to remove the settings, logs, models, the window's data and the stored
Discord token as well; leave it unchecked to keep them for a later install. A portable copy
and an installed copy share the stored token, so deleting the data in one removes the
token for both. Codex stays installed; remove it with `winget uninstall OpenAI.Codex` if
you no longer need it.

## 12. Troubleshooting

- **The commands do not show up in Discord:** set the server's ID in **Config → Discord
  guild ID** (step 3), or wait up to an hour.
- **"Not joined: Codex CLI is not installed"** or **"… not signed in":** do step 4; the
  Status tab shows what is missing.
- **The wake phrase is not recognized:** look at **Wake word** on the Status tab. It says
  if no phrases are set, if a download is waiting for your OK, or which words the model
  does not know. The Console shows each "heard:" line, which tells you what the recognizer
  understood; choose phrases it hears correctly.
- **The bot joins but nobody is heard, or the voice stutters:** check that the bot has the
  **Connect** and **Speak** permissions in that channel and look at the Console for errors.
- **Anything else:** turn on **Verbose log**, reproduce the problem, and attach
  `logs\voicebro.log` (**Codex chat → Open log folder**) to a GitHub issue. With the
  verbose log on, the file contains what people said; read it before you share it.

## Building from source

The app is Rust with Tauri 2. You need the MSVC Build Tools with a Windows SDK, CMake (the
Build Tools copy is enough) and the Tauri CLI (`cargo install tauri-cli --version "^2"
--locked`). `cargo build` builds everything; `cargo test` runs the tests. Releases are
built with `scripts\build-release.ps1` from a tagged `main`; `-Trial` builds a test copy
from any commit. The architecture is described in `docs/ARCHITECTURE.md`.

## License

VoiceBro is released under the MIT License (see `LICENSE`). The third-party components in
`voicebro.exe` and the components it downloads are listed with their licenses in
`THIRD-PARTY-NOTICES.txt`, which ships with every release.
