# VoiceBro user guide

How to set up VoiceBro, use it every day and fix the usual problems. For a short overview,
see the [README](../README.md).

## Contents

1. [Create the Discord bot](#1-create-the-discord-bot)
2. [Install VoiceBro](#2-install-voicebro)
3. [Connect the bot](#3-connect-the-bot)
4. [Set up the engine](#4-set-up-the-engine)
5. [Set the wake phrase](#5-set-the-wake-phrase)
6. [Add another language](#6-add-another-language)
7. [The Whisper mode](#7-the-whisper-mode)
8. [Using VoiceBro](#8-using-voicebro)
9. [Settings](#9-settings)
10. [Privacy and your files](#10-privacy-and-your-files)
11. [Updating](#11-updating)
12. [Uninstalling](#12-uninstalling)
13. [Troubleshooting](#13-troubleshooting)

## 1. Create the Discord bot

VoiceBro talks in your channel through a bot that you create once in Discord's
[Developer Portal](https://discord.com/developers/applications).

1. Press **New Application**, enter a name and press **Create**. People in the channel see
   this name.
2. Open **Bot**. Under **Token**, press **Reset Token** and copy the token. Keep it secret:
   whoever has it controls your bot. Leave the three **Privileged Gateway Intents** off:
   VoiceBro does not use them.
3. Open **Installation**. Keep **Guild Install** among the installation contexts. Under
   **Default Install Settings**, give Guild Install the scopes `bot` and
   `applications.commands`, and the five permissions from the table below.
4. Copy the **Install Link**, open it in your browser, choose **Add to server** and pick
   your server.

The bot needs these permissions:

| Permission | What for |
|---|---|
| **View Channels** | Seeing the voice channel and the text channel where you type `/join`. |
| **Connect** | Joining the voice channel. |
| **Speak** | Playing the assistant's answers in the voice channel. |
| **Send Messages** | Keeping the panel that `/join` posts up to date. VoiceBro edits it whenever the state changes. Posting the assistant's answers, if you turn that on ([section 8](#posting-the-answers-to-discord)). |
| **Embed Links** | Showing that panel, which is an embed, and the previews of the links in a posted answer. |

When the bot joins your server, Discord gives it a role with these permissions. A channel's
own permission settings can take them away again: if the bot cannot join or stays silent in
one channel, right-click that channel, choose **Edit Channel → Permissions** and check the
bot's role there. The people who type `/join` need **Use Application Commands**, which
everyone has by default.

## 2. Install VoiceBro

Download one of the two files from the
[latest release](https://github.com/OldSlash/voicebro-discord/releases/latest):

- **The installer**, `VoiceBro_<version>_x64-setup.exe`, installs for your Windows user
  without administrator rights, into `%LOCALAPPDATA%\VoiceBro` unless you choose another
  folder, and adds VoiceBro to the Start menu. On a Windows 10 PC without the Microsoft
  Edge WebView2 Runtime it installs that too.
- **The portable zip**, `VoiceBro_<version>_x64-portable.zip`: extract it into any folder
  and start `voicebro.exe`.

VoiceBro keeps all its files next to `voicebro.exe`, so it needs a folder you can write to.
In a folder such as `C:\Program Files` it shows a message and does not start.

> [!NOTE]
> The releases are not code-signed yet. When Windows shows "Windows protected your PC",
> choose **More info**, then **Run anyway**. To check a file, compare its hash with
> `SHA256SUMS.txt` from the same release:
>
> ```powershell
> Get-FileHash .\VoiceBro_1.0.0_x64-setup.exe -Algorithm SHA256
> ```

## 3. Connect the bot

Start VoiceBro and open the **Config** tab.

1. Paste the token into **Discord bot token** and press **Save changes**. VoiceBro keeps
   the token in the Windows Credential Manager, not in a file, and connects at once and
   at every start.
2. Optionally, put your server's ID into **Discord guild ID** and press **Save changes**.
   The bot's commands then appear in that server right away; otherwise Discord can take up
   to an hour to show them. To copy the ID, turn on **Developer Mode** in Discord
   (**User Settings → Advanced**), right-click your server and choose **Copy Server ID**.

The status bar turns green with **Connected**, and the **Status** tab lists your server.

## 4. Set up the engine

The engine is what writes the answers. Choose it in **Config → Assistant → Engine**:

| Engine | Sign-in | Modes |
|---|---|---|
| **Codex** (the default) | Your ChatGPT account | Realtime voice and Whisper |
| **Claude Code** | Your Claude account, or an Anthropic API key | Whisper only |
| **OpenCode** | The AI providers you set up in OpenCode | Whisper only |

Only Codex has the realtime voice mode. Choosing Claude Code or OpenCode while the realtime
voice mode is on switches to the [Whisper mode](#7-the-whisper-mode), which may ask to
download its files. You can switch the engine at any time, also while the bot is in a
voice channel: the running session ends and the next question goes to the new engine.

Each engine is installed separately and may only search and fetch the web: VoiceBro gives
none of them a tool that reads, writes or runs anything on your PC.

### Codex

The assistant uses OpenAI's Codex CLI with your ChatGPT account. Open the **Status** tab.

**If Codex is not installed,** the tab says so and offers **Install Codex**. The button runs
the Windows Package Manager (winget) in a console window you can watch: it installs the
package `OpenAI.Codex` from OpenAI's GitHub releases, about 200 MB. Answer winget there if
it asks, press a key when it is done, and VoiceBro finds Codex on its own. Without winget,
install Codex with Node.js (`npm install -g @openai/codex`), or download it from
[the Codex releases](https://github.com/openai/codex/releases) and set the path of
`codex.exe` in **Config → Codex executable**.

**Then sign in:** press **Sign in to Codex**, finish the ChatGPT sign-in in your browser
and press **Check Codex**. The **Codex account** tile shows your account. If you already
use Codex on this PC, **Check Codex** finds that sign-in: VoiceBro uses your normal Codex
installation.

Codex runs only while the bot is in a voice channel, and for a minute after **Sign in to
Codex**, **Check Codex** or **Refresh lists**. **Check Codex** also reads the models,
reasoning levels and voices of your account; VoiceBro saves them in `codex-lists.json`, so
the lists in **Config → Codex** are there at the next start without starting Codex. The
first start after installing reads them once on its own when Codex is installed.

### Claude Code

Choose **Claude Code** in **Config → Assistant → Engine**, then open the **Status** tab.

**If Claude Code is not installed,** the tab offers **Install Claude Code**: winget, in a
console window you can watch, installs the package `Anthropic.ClaudeCode` from Anthropic,
about 330 MB. Without winget, use Claude Code's own installer
([code.claude.com/docs/en/setup](https://code.claude.com/docs/en/setup)); VoiceBro finds
`claude.exe` on the PATH, in winget's folder or in `%USERPROFILE%\.local\bin`, or set its
path in **Config → Claude Code → Claude Code executable**.

**Then sign in:** press **Sign in to Claude Code**. A console window runs Claude Code's own
sign-in (`claude auth login`), which opens the Claude sign-in in your browser; when the
window closes, VoiceBro reads what kind of sign-in it is. If you already use Claude Code on
this PC, press **Check Claude Code** instead. VoiceBro never sees your password or token:
it only reads whether Claude Code uses a Claude subscription (with its plan), an API key or
a cloud provider. An `ANTHROPIC_API_KEY` in your Windows environment wins over a
subscription, as it does in a terminal, and the **Claude Code sign-in** tile shows it.

**Check Claude Code** also reads the models your sign-in offers, for **Config → Claude Code →
Model**. A faster model answers sooner; empty is Claude Code's default.

Claude Code runs only while the bot is in a voice channel, one process per channel. It
runs without your own Claude Code settings, `CLAUDE.md` files, hooks and MCP servers, and
saves no conversation to disk. A change of the model or of your instructions applies from
the next `/join`.

#### A Claude subscription is yours alone

If you sign Claude Code in with a Claude subscription (Free, Pro, Max, Team, Enterprise or
another plan of claude.ai), that subscription may be used only by you:
[Anthropic's Consumer Terms](https://www.anthropic.com/legal/consumer-terms) do not let you
make your account available to anyone else, and
[Claude Code's legal page](https://code.claude.com/docs/en/legal-and-compliance) says that
subscriptions are meant for ordinary, individual use. While everyone in the channel is
heard, the others use your subscription.

So VoiceBro shows a notice at the top of the **Claude Code** card on the **Config** tab while
Claude Code runs on a subscription and more voices than yours reach it. The first time you
choose Claude Code, a window asks the same:

- **Hear only me** chooses **Only my Discord account** in the Access card and, if your
  Discord account is not linked yet, shows the link code right away
  ([Only my Discord account](#only-my-discord-account)). Everyone still hears the answers
  and can use the bot's commands.
- **Not now** changes nothing; the notice stays in the Claude Code card.

The window does not come back after you answered it. The notice in the card has **Hide**
next to **Hear only me**: it removes the notice for good, also after a restart, and changes
nothing else. VoiceBro does not enforce the choice:
it is yours. With an Anthropic API key or a cloud provider (Amazon Bedrock, Google Vertex
AI, Microsoft Foundry), [Anthropic's Commercial Terms](https://www.anthropic.com/legal/commercial-terms)
apply instead and VoiceBro shows no notice. This is not legal advice; read the terms of
your own account.

### OpenCode

Choose **OpenCode** in **Config → Assistant → Engine**, then open the **Status** tab.

**If OpenCode is not installed,** the tab offers **Install OpenCode**: winget installs the
package `Anomaly.OpenCode` from the OpenCode project, about 120 MB to download. Without
winget, use Node.js (`npm install -g opencode-ai`) or download OpenCode from
[its releases](https://github.com/anomalyco/opencode/releases) and set the path of
`opencode.exe` in **Config → OpenCode → OpenCode executable**.

**Then set up a provider in OpenCode itself.** VoiceBro has no sign-in button for OpenCode
and never sees its keys: open a terminal, run `opencode auth login`, pick a provider (for
example OpenAI or Anthropic) and sign in or paste its key
([OpenCode's providers](https://opencode.ai/docs/providers)). Then press **Check OpenCode**:
the **OpenCode providers** tile names them, and **Config → OpenCode → Model** lists every
model OpenCode offers as `provider/model`, among them OpenCode's own models (provider
`opencode`), some of which are free and need no provider of yours; their use is limited.
Empty is OpenCode's own default model, which can be one of those free models even when your
provider is connected: to be sure your provider answers, choose one of its models.
`/join` is refused when OpenCode offers no model, or not the one you chose.

OpenCode runs only while the bot is in a voice channel, behind a local server that only
this PC can reach, with a password made for each start. It runs without the instruction
files and skills of other agents (such as your `~\.claude\CLAUDE.md`), and the assistant
may only search and fetch the web. OpenCode's own global setup still applies: if you keep
instructions for OpenCode in `~\.config\opencode\AGENTS.md`, or settings and plugins in
`~\.config\opencode\opencode.json`, OpenCode uses them for VoiceBro's assistant too, as it
does for every session. The conversations VoiceBro made in OpenCode are deleted when the bot
leaves the channel.

**OpenCode's web search** is on. Unlike Codex and Claude Code, whose searches go to their
own provider, OpenCode sends the search queries the model writes to a web search service,
a company other than your model's provider. By default that is Exa, which answers without a
key. If one of the variables `EXA_API_KEY`, `TAVILY_API_KEY`, `FIRECRAWL_API_KEY`,
`PARALLEL_API_KEY` or `TINYFISH_API_KEY` is set in your Windows environment when VoiceBro
starts OpenCode, the first of them in that order chooses the service (Exa, Tavily,
Firecrawl, Parallel or TinyFish) and OpenCode uses that key. When a search fails, OpenCode
answers without it.

## 5. Set the wake phrase

This section is about the default mode, **Realtime voice**, where Vosk listens for the wake
phrase. The first field of **Config → Assistant**, **Assistant mode**, chooses the mode, and
the grey **Recognition** field under it shows "Vosk". In the
[Whisper mode](#7-the-whisper-mode) the same card shows the Whisper phrases in place of the
wake model and its phrases; the Whisper model is in **Settings → Whisper**.

Open **Config → Assistant**.

![The Assistant settings with the English model, the phrases and the download notice](images/wake-settings.png)

- **Activation** decides when the assistant listens. **Wake word** (the default) starts a
  session when someone says an activation phrase and ends it with a deactivation phrase
  or after the **Auto-stop** time. **Always listening** sends everything said in the
  channel to the assistant while the bot is there.
- **Wake model** is the language of your phrases: English (US, the default), Russian,
  Czech, Dutch, French, German, Italian, Polish, Portuguese (Brazil) or Spanish. None of
  them comes with the installer; each is downloaded once, after you agree. Each model
  keeps its own phrases, so switching languages loses nothing.
- **Activation phrases** and **Deactivation phrases** belong to the model chosen below
  them. Separate phrases with commas; a phrase may have several words, for example
  `Отбой братан, Заканчивай, Братан всё` holds three deactivation phrases. A line break
  separates too. Every word must be in the model's vocabulary: words the model does not
  know are listed under the field. Case and punctuation do not matter, and "ё" counts as
  "е", so `Братан всё` matches when the recognizer writes "братан все".
- **Every model comes with default phrases**, so the wake word works at once. The fields
  show them until you change them:

  | Wake model | Activation | Deactivation |
  |---|---|---|
  | English (US) | bro | peace out |
  | Russian | братан | отбой |
  | Czech | brácha | čau |
  | Dutch | maat | doei doei |
  | French | mon frère | à la prochaine, a la prochaine |
  | German | bruder | bis dann |
  | Italian | fratello | arrivederci |
  | Polish | ziomek | do widzenia |
  | Portuguese (Brazil) | irmão | até mais |
  | Spanish | hermano | hasta luego |

  Each was checked with the small model of its language on spoken samples. A model you
  added to the models folder takes the defaults of its language when its folder name says
  which one, for example `vosk-model-small-de-0.21`; any other starts with empty fields.

  Change a default or remove it: what you type replaces the default of that field only.
  An emptied field stays empty; the default does not come back.
- The deactivation phrase also counts when the assistant's own call hears it: during an
  answer the voice call transcribes what people say, and a deactivation phrase in that
  text ends the session even when the wake word missed it. The voice stops at once and
  the falling tone plays.
- When the wake word's files are not on this PC, the notice under **Wake model** in
  **Config → Assistant** says so, and so does **Wake word** on the **Status** tab. It
  lists what is needed: the Vosk recognizer, 15 MB from github.com, once, and the model,
  32 to 53 MB depending on the language, from alphacephei.com. Starting VoiceBro does not
  interrupt you with it.
- Choosing a model that is not on this PC saves it at once; the notice under **Wake model**
  then lists its files with **Download**. No window pops up for a choice or for a switch to
  **Wake word**.
- While the wake word's files are missing, `/join` is refused: the bot stays out of the
  channel and its reply names the missing files and says to press **Download** under
  **Wake model**. At the same time a window, **The wake word is not installed**, asks
  about the same files. Nothing is downloaded until you press **Download**; the window
  shows the progress and closes when the files are in place. Then run `/join` again.
  With **Always listening** Vosk is not used, and `/join` does not need its files.
- **Not now** closes the window and downloads nothing. The notice under **Wake model**
  keeps its own **Download** button, and the window comes back at the next refused
  `/join`.
- **Rather download the files yourself?** The window and the notice name each file and
  link to the official page it is published on. A link opens in your browser:
  - `vosk-win64-0.3.45.zip`, the recognizer, from its release page on GitHub
    (`github.com/alphacep/vosk-api/releases/tag/v0.3.45`), under **Assets**;
  - the model, for example `vosk-model-small-en-us-0.15.zip` for English, from the Vosk
    models page (`alphacephei.com/vosk/models`). The models are not on GitHub; this page
    is where the Vosk project publishes them.

  Press **Open assets folder** and put the `.zip` files there as they are, without
  unpacking them, then press **Download**. VoiceBro checks each file against the
  checksum it knows for it and unpacks it without going online; the window says
  "Checking and unpacking" instead of "Downloading". Only files that are not in the
  folder are downloaded. A file with the right name that fails the check is deleted, and
  the window says so.

Good phrases are easy to recognize and rare in game talk:

- Two words work better than one short one. In our tests `hey bro` and `peace out` were
  always recognized; a lone `bro` at the start of a sentence was sometimes heard as
  another word.
- If a phrase can be written two ways, add both, for example `goodbye` and `good bye`.
- Check what the model hears. The **Console** shows a "heard:" line for everything the
  recognizer understood; if it writes your phrase differently, use its spelling. In our
  tests the small Czech model heard "kámo" as "samo", the small French model heard
  "tchao" as "tiens" and wrote "a la prochaine" without the accent, and the small
  Portuguese model heard "mano" as "humano", so those are not the defaults.
- Keep the deactivation phrase distinct: it cuts off the current answer.

## 6. Add another language

VoiceBro works with any [Vosk model](https://alphacephei.com/vosk/models), also for a
language that is not in the **Wake model** list.

1. Download a **small** model for your language, for example
   `vosk-model-small-ca-0.4` for Catalan.
2. In **Config → Assistant**, press **Open models folder** and extract the zip there, so
   that you get `models\vosk-model-small-ca-0.4\am\final.mdl`.
3. Restart VoiceBro. The model appears in **Wake model**, marked "models folder".

Use the small models: the large ones take minutes to load and gigabytes of memory. A model
you add comes with its own license, shown on the models page.

## 7. The Whisper mode

VoiceBro has two ways to talk to the assistant. Choose one in **Config → Assistant →
Assistant mode**:

| | Realtime voice (the default) | Whisper |
|---|---|---|
| The wake phrase | Vosk listens for it on your PC. | Whisper writes down what is said on your PC and finds the phrase in that text. |
| Your question | Goes to OpenAI's realtime voice call as audio. | Whisper writes it down on your PC; only the text goes to the engine. |
| The answer | ChatGPT's voice from the call. | The engine answers in text, and a voice on your PC reads it into the channel. |
| The engine | Codex only. | Codex, Claude Code or OpenCode ([section 4](#4-set-up-the-engine)). |
| What it downloads | Vosk and a wake model, about 50 to 70 MB. | Whisper and a model, and a voice: about 230 MB on the CPU with the defaults, more with an NVIDIA GPU or a larger model. |

Your instructions, **Speaker names**, **Post answers to Discord**, **Who the assistant
hears**, the mute, the tones and the auto-stop work in both modes and with every engine.

> [!NOTE]
> The Whisper mode is new. The timings below come from tests on one PC with recorded
> speech. It has not been tried yet in a real voice channel with several people while a
> game runs.

### What leaves your PC

In the Whisper mode, speech is recognized on your PC and the voice is made on your PC. Only
text goes to the engine's provider: OpenAI with Codex, Anthropic with Claude Code, and with
OpenCode the provider of the model you chose, plus OpenCode's web search service for the
searches the model makes:

- **between sessions, nothing**: what people say only reaches Whisper, which looks for the
  activation phrase;
- **during a session, the text of every question**, with the speaker's name at its start
  unless **Speaker names** is off;
- **the names of the people in the channel**, with the first question of a session, unless
  **Speaker names** is off;
- **the earlier questions and answers of this run of VoiceBro**, as text, when the engine
  starts a new conversation: at a `/join` after the bot had left the channel, or with Codex
  after you changed its model or reasoning.

With **Always listening**, everything said in the channel goes to the engine as text, the
whole time the bot is in it.

### Where the settings are

The Whisper mode's settings sit with the others: what the bot does is on the **Config** tab;
the Whisper model, the voice and how Whisper runs on this PC are on the **Settings** tab.
Every field saves as soon as you change it, also a model or voice that is not downloaded
yet. All of them but the Whisper phrases, which show only in the Whisper mode, can be
changed in either mode.

- **Config → Assistant**: **Assistant mode** first, then the grey **Recognition** field,
  which shows "Vosk" or "Whisper" and follows the mode. In the Whisper mode the card shows
  the Whisper mode's **Activation phrases** and **Deactivation phrases**, where the
  realtime mode shows the wake model and its phrases. **Activation** (wake word or always
  listening) and the auto-stop stay below, shared by both modes. While the Whisper mode's
  files are not all here, a note under **Assistant mode** says what is not downloaded,
  and **Open Settings** takes you to the card where you download it.
- **Settings → Whisper**, the card after Diagnostics: an **On** or **Off** label that
  follows the mode (switch it in **Config → Assistant**), the **Whisper model**, **Device**,
  **CPU threads**, **Language**, the download notice with **Download**, a line with what is
  in use and the timings of the last answer, and **Open Whisper models folder**.
- **Settings → Local voice**, the card after Whisper: **Voice**, **Speed**, **Preview** and
  the voice's download notice with **Download**. A note on top says whether the voice is in
  use: in the realtime mode it is not, because the Codex call speaks with its own voice, but
  you can choose and download one now.

### Downloads

Nothing of the Whisper mode comes with the installer. What is needed depends on what you
choose:

| File | Size | From |
|---|---|---|
| The Whisper runtime for the CPU | 9 MB | github.com |
| Or, for **NVIDIA GPU**, the Whisper runtime with CUDA 12.4 (it holds the CPU runtime too) | 685 MB, about 1.15 GB unpacked | github.com |
| The voice activity model, which keeps noise away from Whisper | 0.9 MB | huggingface.co |
| The Whisper model you choose | 44 MB to 1.6 GB | huggingface.co |
| The voice engine sherpa-onnx, for every voice but the Windows ones | 8 MB | github.com |
| The voice you choose: Supertonic 3 (ten voices), a Piper voice or Kokoro English (eleven voices) | 129 MB; 67 to 116 MB; 320 MB | github.com |
| The phonemizer piper-phonemize, only for Piper and Kokoro voices | 0.4 MB | github.com |

Nothing is downloaded until you press **Download**, and you can download in either mode.
Each notice lists what is still missing, with each size, and its **Download** fetches only
that:

- **Settings → Whisper**: everything the chosen Whisper model still needs on the chosen
  device: the runtime (the NVIDIA one when you chose the GPU), the model and the voice
  activity model, so you can download it all before you switch;
- **Settings → Local voice**, under the voice: the voice engine, the phonemizer when the
  voice needs it, and the voice.

A download in the realtime mode only installs the files; nothing is loaded until you use
the Whisper mode. The consent counts for this choice only: another model, device or voice
asks again for what it adds.

Choosing a model, a device or a voice never opens a window: it is saved at once, and its
card's notice lists what it still needs. Neither does switching to the Whisper mode: the
note in **Config → Assistant** says what is missing instead.

While files are missing, `/join` in the Whisper mode is refused, and the bot replies "Not
joined: The Whisper mode needs downloads: open VoiceBro → Settings and press Download in
the Whisper and Local voice cards, then run /join again". At the same time a window, **The
Whisper mode is not installed**, lists everything your model, device and voice still need,
with each size and the total; **Download** fetches it all. Starting VoiceBro never opens
the window. **Not now** closes it; the notices keep their **Download** buttons. After the
download, run `/join` again.

**Rather download the files yourself?** As for the wake word (section 5), the window and
the notices name each file and link to the page it is published on. Press **Open assets
folder** and put the files there as they are, without unpacking them, then press
**Download**. A Whisper model must have the file name the window shows: some models are
published as `ggml-model.bin`, so rename the file after the download.

### CPU or NVIDIA GPU

These are in **Settings → Whisper**.

- **Device**: **CPU** (the default) or **NVIDIA GPU (CUDA)**. The GPU needs an NVIDIA
  graphics card with its driver and the larger runtime download, which the card's notice
  lists with **Download**. Whisper on AMD or Intel graphics is not available.
- Without a usable NVIDIA card, Whisper runs on the CPU. The line on the **Whisper** card
  then reads "GPU requested, CPU used" with the reason, and so does the **Whisper device**
  tile on the **Status** tab.
- If Whisper already ran on the CPU since VoiceBro started and you then download the GPU
  runtime, restart VoiceBro to use the GPU.
- The very first answer after you install the GPU runtime can take about half a minute,
  while the graphics driver prepares Whisper's code. The driver keeps the result, so later
  starts are quick.
- **CPU threads**: 0 (the default) picks half of your processor's threads, at most 16. It
  counts only while Whisper runs on the CPU.
- The voice always runs on the CPU.

### Choosing a model

Choose the Whisper model in **Settings → Whisper**, in either mode. A model that is not on
this PC is saved at once, and the card's notice lists its files with **Download** (base
(q8_0) until you choose another). The model decides how well Whisper hears and how long you wait after a question. Whisper
always works on a 30-second window, so a short phrase takes almost as long as a long one.
In our tests on a gaming laptop (Intel Core Ultra 9 275HX with 24 threads, NVIDIA GeForce
RTX 5090 Laptop GPU), one utterance took:

| Model | On the CPU | On the NVIDIA GPU |
|---|---|---|
| tiny (q8_0), 44 MB | 0.25 s | |
| base (q8_0), 82 MB, the default | 0.55 s | |
| small (q8_0), 264 MB | 1.7 to 1.8 s | |
| medium (q5_0), 539 MB | 6 to 9.5 s | |
| large-v3-turbo (q5_0), 574 MB | 11 s | 0.12 to 0.14 s |

- **With an NVIDIA GPU, choose large-v3-turbo (q5_0).** It hears well and takes about a
  tenth of a second. It holds about 0.8 GB of video memory while the bot is in a channel.
- **On the CPU, choose base** for quick answers, or **small** if base mishears you and your
  processor is fast. Medium and the large models are too slow on a CPU for a conversation.
  A slower processor than ours takes longer.
- **English only** models (base.en, small.en, distil-large-v3.5) hear only English.
- **Russian-tuned** models are community fine-tunes of large models for Russian speech; use
  them on a GPU.
- **Your models**: put any ggml Whisper model (a `.bin` file) into the `whisper-models`
  folder (**Settings → Whisper → Open Whisper models folder**) and restart VoiceBro. It
  appears under **Your models**. VoiceBro never changes that folder. A model you add comes
  with its own license.
- **Language** (**Settings → Whisper**): **Automatic** lets Whisper detect the language of
  every utterance. A fixed language is quicker and more exact on short phrases.

The line on **Settings → Whisper** shows the last answer's timings: from the end of the
question to the text, from the text to Codex's first words, and from those to the voice.

### Phrases

In the Whisper mode, the phrase fields of **Config → Assistant** hold the Whisper mode's own
phrases, kept apart from the wake models' phrases; switching back to realtime voice shows
the wake model's phrases again, and both sets are kept. Whisper writes any word, so a
phrase may be any word or several words in a row; there is no vocabulary check. Separate
phrases with commas or line breaks. Case and punctuation do not matter, and "ё" counts as
"е". Until you change a list, the defaults are **bro** and **peace out**, whatever the
language.

How Whisper's phrases work:

- Whisper writes down an utterance once its speaker has paused for 0.7 s. The activation
  phrase counts a moment after you stop talking, not while you speak.
- Say the phrase and the question in one breath: "hey bro, what is the weather in Bangkok?"
  starts the session and asks the question. The phrase alone starts the session with the
  rising tone, and your next sentence is the question.
- **During a session, everything anyone says is a question**, and a new question cuts off
  the answer in progress, as speaking during a realtime call does. Keep side talk out of a
  session, or end it.
- A deactivation phrase anywhere in a sentence, as in "ok thanks, peace out", ends the
  session at once and cuts off the answer. The words before it are not sent.
- With **Always listening**, everything said is a question, and the phrases are not used.
- A voice activity check runs before Whisper, to keep silence and game sounds from turning
  into text.
- Whisper gets your phrases as a hint, so it spells them the way you typed them.

Two words work better than one short one, and the deactivation phrase should not come up
in normal game talk. The **Console** shows a "heard" line for everything Whisper wrote down.

### The voice

Choose it in **Settings → Local voice**.

| Voice | Languages | Notes |
|---|---|---|
| **Supertonic 3**, F1 to F5 and M1 to M5 (the default is F1) | 31, among them all ten wake languages | Reads each answer in the language of the question. The first words come about half a second after Codex's first sentence. |
| **Piper** | One each: English, Russian, Czech, Dutch, French, German, Italian, Polish, Portuguese, Spanish | The quickest. Needs the phonemizer. |
| **Kokoro English** | English | Eleven voices. Needs the phonemizer. |
| **Windows** | The speech languages your Windows has | No download and instant, but robotic. A Windows speech pack adds languages. |

The voice list shows each voice's data license. Supertonic 3 is under BigScience
OpenRAIL-M, which allows commercial use but comes with use restrictions; the phonemizer
that Piper and Kokoro need, piper-phonemize, is GPL-3.0 and a separate download.
`THIRD-PARTY-NOTICES.txt` lists them all.

- **Speed**: 0.5 to 2 times the voice's own pace. It applies from the next answer and to
  **Preview**.
- **Preview** reads a short sample sentence about the weather in London in the voice's
  language (in English for Supertonic 3) and plays it in the window only, never in Discord.
  The voice's files must be on this PC.
- The voice starts while Codex is still writing, after its first sentence. Every message
  Codex writes is read, also a short "let me check that" before a web search.
- Code and tables are not read, and a link is read as its site's name. A posted answer
  ([section 8](#posting-the-answers-to-discord)) keeps them.

In the Whisper mode Codex answers every question itself, so **Model** and **Reasoning** in
**Config → Codex** set how quickly each answer comes: a low reasoning level is the quick
choice. **Voice** and **Answer handoff** there are for the realtime mode only.

### Switching modes

A change of **Assistant mode** applies at once, also while the bot is in a voice channel:
a running session ends with the falling tone, the bot stays in the channel, and the next
session uses the new mode.

- Switching to Whisper loads Whisper and the voice in the background; the line on
  **Settings → Whisper** shows it. If files are missing, no window pops up: the note in
  **Config → Assistant** says what is not downloaded, **Open Settings** takes you to it,
  and the bot hears nothing in the Whisper mode until the files are here. Downloading them
  beforehand, in the realtime mode, avoids the wait.
- Switching back to realtime voice unloads them.
- **Config → Assistant** swaps its rows with the mode: the wake model and its phrases in the
  realtime mode, the Whisper phrases in the Whisper mode. Each keeps its values for the way
  back.

The Whisper model and the voice load when the bot joins a voice channel, so `/join` takes a
moment longer, and they are unloaded half a second after the bot leaves its last channel.
Between joins they are not in memory; only the libraries that run them stay loaded until
VoiceBro closes.

In Discord, `/status` says "Mode: Whisper mode." or "Mode: Realtime voice.", and the panel
that `/join` posts shows the same under **Mode**.

## 8. Using VoiceBro

| In Discord | What happens |
|---|---|
| `/join` | The bot joins your voice channel and replies with a panel that shows the engine, its state and a **Mute assistant** button. |
| Say the activation phrase and a question | A session starts and the assistant answers in the channel. |
| Say the deactivation phrase, or stay quiet | The session ends, at once or after the auto-stop time. |
| `/status` | Shows what VoiceBro is doing, and the mode: realtime voice or Whisper. |
| `/leave` | The bot leaves the channel. |

**Mute** ends the current session at once and sends nothing until you unmute. Use the
**Mute assistant** button in Discord, the button on the **Status** tab, or the hotkey
**Ctrl+Alt+M**, which works system-wide (change it in **Config → Hotkey**).

In the window, **Chat** shows what the assistant heard, said and looked up, each line
labelled with the engine that wrote it, and **Console** shows what the app is doing.

### When two people talk at once

By default the assistant hears the channel as one sound: in the realtime voice mode two
people talking over each other reach it mixed, and in the Whisper mode a new question stops
the answer that is playing and is answered instead.

Turn on **One speaker at a time** (**Config → Assistant**, off by default) to change that:

- **Realtime voice:** the assistant hears one person at a time. Whoever talks over them is
  kept and follows when they pause for about a third of a second, with a short note
  telling the assistant who is speaking now, so it can answer each person by name; nobody
  is mixed in. **Speaker names** set to off (Config → Codex) sends no note. Speech kept
  for someone who waits never cuts an answer that is playing: it follows the answer.
- **Whisper:** a question asked while the engine works on an answer is handed to it
  without stopping it. Codex adds it to that answer, OpenCode takes it up as soon as it can,
  and Claude Code, which answers one question at a time, answers it right after. A question
  asked while the answer is being read waits and is answered after it. Talking over the
  voice no longer stops it; the deactivation phrase or **Mute** does.

In both modes VoiceBro tells people apart by their own Discord stream: two voices that
reach Discord through one microphone (a friend beside you, speakers without headphones)
still go to the assistant together.

### Posting the answers to Discord

When the assistant hands a question to Codex (a web search, a recipe, a price), Codex
writes a full answer with facts, lists and links, and the voice only retells it. VoiceBro
can post that full answer as text in a Discord channel, while the voice is still speaking,
so everyone can read it, open its links and see their previews. Only Codex's final answer
is posted: not its "I'll look that up", not what the voice says and not what people say.
A long answer comes as several messages; a table comes as a block of fixed-width text so
its columns line up. Nobody is pinged, even if the answer contains `@everyone` or a name.

In the [Whisper mode](#7-the-whisper-mode) the engine answers every question itself, so
every answer it finishes is posted, with the engine and the question as its first line. An
answer cut off by a new question or by the deactivation phrase is not posted.

Turn it on in **Post answers to Discord** in the engine's card (**Config → Codex**,
**Claude Code** or **OpenCode**). It is off by default, because a
posted answer is public in that channel and stays there. Choose where the answers go:

- **The voice channel's chat.** Every voice channel has its own text chat, opened with the
  speech-bubble icon next to the voice channel. Nothing more to set up: the answers follow
  the bot into whatever channel `/join` puts it.
- **The channel of /join.** The answers go where you typed `/join`, under the panel. Type
  `/join` in the text channel you want them in. Typed in the voice channel's own chat, this
  is the same as the voice channel's chat.
- **A chosen channel.** One channel for every answer, for example a channel `#answers`.
  1. Create the channel in Discord. If it is private, add the bot's role to it.
  2. In VoiceBro, choose **A chosen channel**, then pick the channel from **Answers
     channel** below it. The list shows the text channels of the servers the bot is in,
     while the bot is connected. Nothing is saved until you pick one.
  3. A channel marked **(can't post)** lacks one of the permissions below for the bot.
     Fix them in Discord; the mark goes the next time you open the **Config** tab.

  Only answers from that channel's server go there; while the bot is in a voice channel of
  another server, nothing is posted at all.

For every destination the bot needs **View Channels**, **Send Messages** and **Embed
Links** in that channel. They are among the five permissions of the install link, but a
channel's own settings can take them away. If the answers do not appear in one channel,
right-click that channel, choose **Edit Channel → Permissions** and check the bot's role.
When Discord refuses an answer, the **Status** tab shows an `answers` error that names
the channel and the reason; the next answer that gets through clears it. A change of the
setting applies from the next answer. An answer Codex is still looking up when someone
types `/leave` is not posted.

### Who the assistant hears

**Config → Access → Who the assistant hears** decides whose voice reaches the wake word and
the assistant. All three choices are optional tools for you as the person who runs the bot;
which one you use is up to you.

- **Everyone** (the default): every member in the bot's voice channel.
- **People who agreed (/optin)**: your linked Discord account and everyone who ran `/optin`
  in Discord and pressed **I agree**. Nobody else's voice starts a session or gets into a
  question, and neither does a music bot's.
- **Only my Discord account**: the account you linked, nobody else. The assistant runs on
  your own subscription, and a subscription is meant for one person; with a Claude
  subscription VoiceBro asks you to choose this
  ([A Claude subscription is yours alone](#a-claude-subscription-is-yours-alone)).

Everything else stays open in every mode: anyone can use `/join`, `/leave`, `/status` and
the **Mute assistant** button, everyone in the channel hears the answers, and posted answers are
posted as before. A change applies at once, also while the bot is in a channel.

The bot is yours, so Discord's rules for bots are yours to keep: Discord's developer terms
ask every bot for a privacy policy that people can reach, and for a person's own consent
before their speech is shared with someone else (here, the engine's provider: OpenAI,
Anthropic, or with OpenCode the provider you chose and its search service). VoiceBro gives
you the tools for both:

1. Write your bot's privacy policy from the template `docs/PRIVACY-POLICY-TEMPLATE.md` in
   VoiceBro's repository, publish it at a public `https://` address (a GitHub Gist works),
   and put that address into **Privacy policy URL** in the Access card and into **Privacy
   Policy URL** of your application in the Discord Developer Portal.
2. Choose **People who agreed**.
3. Ask your friends to run `/optin` in a server with the bot or in a direct message to it.
   Only they see the bot's reply: what is sent after the activation phrase, to whom, what
   is kept, and a link to your policy, with **I agree** and **Cancel**. **I agree** adds
   them to the list in the Access card, with the time.
4. `/optout` takes a person off the list at once and deletes their record; so does
   **Remove** next to them in the card (the bot sends them nothing). `/privacy` shows your
   policy to anyone.

Without a privacy policy URL, `/optin` is refused and the card warns you. The records are
kept in `consents.bin`, encrypted for your Windows user. If VoiceBro changes the consent
text in an update, or you switch to another engine (or, with OpenCode, to a model of
another provider), the old records are removed and people run `/optin` again, since they
agreed to send their speech to someone else. The filter works on each person's own Discord stream: a voice that
someone else's microphone picks up (a friend sitting next to them, or speakers without
headphones) passes with theirs.

New slash commands can take up to an hour to appear in other servers and in direct
messages; in the server set as **Discord guild ID** they appear at once.

### Only my Discord account

To link your account:

1. Sign in to the engine ([section 4](#4-set-up-the-engine)); OpenCode needs none here.
2. In **Config → Access**, press **Link my Discord account**. VoiceBro reads the engine's
   account (your ChatGPT account with Codex, your Claude account with Claude Code) and shows
   a code, for example `K7QF-2M9X`, for 10 minutes.
3. In Discord, from your own account, send `/link` with that code, in any server the bot is
   in or in a direct message to the bot. Only you see the bot's reply. Upper or lower case
   and the dash do not matter.
4. The card shows your Discord name. Choose **Only my Discord account** (or **People who
   agreed**, where your linked account needs no `/optin`).

The code works once. Five wrong codes, from anyone, end it; press **Link** for a new one.
After a first install, `/link` can take up to an hour to appear in direct messages; in the
server set as **Discord guild ID** it appears at once.

The link stays until you press **Unlink**, link another Discord account, sign the engine
into another account, or delete VoiceBro with its data. When Codex or Claude Code is signed
into another account, the card shows "Not linked: the Codex account changed" (or "the
Claude account changed"); link again. Switching the engine keeps the link. While **Only my
Discord account** is chosen and no account is linked, the assistant hears nobody.

## 9. Settings

The **Settings** tab holds the window's own settings and how Whisper runs on this PC; the
**Config** tab holds the bot's.
Settings save as soon as you change them, except the three fields of **Config → Discord
and Codex**, which wait for **Save changes**. Each one has a short explanation under it in
the window. On both tabs, click a card's title to fold it away or open it again; VoiceBro
remembers which cards are folded.

| Where | What you set |
|---|---|
| Settings → Appearance | Theme and font size. |
| Settings → Diagnostics | **Performance monitoring**, **Memory measurement**, **Verbose log**. All are off by default and cost nothing while off. **Opus codec (experiment)**: which Opus implementation encodes and decodes the voice (the assistant toward Discord, your speech toward Codex and its answer, the people in the channel): libopus, the C library VoiceBro has always used and the default, or opus-rs, written in Rust and being tried as its replacement. A change applies at the next `/join` and the next call; the line under the list shows the codec in use. |
| Settings → Whisper | Whether the Whisper mode is on (it is switched in Config → Assistant), the Whisper model, CPU or NVIDIA GPU, the CPU threads, the language, the download of the Whisper files (in either mode), what is in use and the last answer's timings, and the Whisper models folder ([section 7](#7-the-whisper-mode)). |
| Settings → Local voice | The voice that reads the answers in the Whisper mode, its speed, **Preview**, and its download. Not used in the realtime mode. |
| Config → Discord and Codex | The Discord bot token, the server ID, the path of `codex.exe` when it is not found on its own. Press **Save changes**. |
| Config → Access | **Who the assistant hears** (Everyone by default, People who agreed, Only my Discord account), **Privacy policy URL**, the link to your Discord account and the people who agreed with `/optin` ([section 8](#who-the-assistant-hears)). |
| Config → Audio | How loud the assistant hears the channel and how loud it speaks; the short tones at session start and end. |
| Config → Assistant | **Assistant mode** (realtime voice, the default, or Whisper), the grey **Recognition** field and the **Engine** (Codex, Claude Code or OpenCode, [section 4](#4-set-up-the-engine)); wake word or always listening, the phrases, the auto-stop time; the wake model with its download in the realtime mode, the Whisper phrases and the note on what the Whisper mode misses in the Whisper mode; **One speaker at a time** (off by default, [section 8](#when-two-people-talk-at-once)). |
| Config → Codex, Claude Code or OpenCode | The card of the engine in use, titled after it. **Codex:** the model and reasoning effort for looked-up answers (in the Whisper mode, for every answer), the voice of the realtime call; the model, reasoning and voice lists come from the last time VoiceBro read them from Codex, the card says when that was, **Refresh lists** reads them now, and **Refresh the lists at start** (off by default) reads them at every start of VoiceBro. **Claude Code** and **OpenCode:** the program's path when it is not found on its own, and the model. **Every engine:** your instructions for the assistant (one text for all three), whether it gets the names of the people in the channel, **Save chat history**, **Post answers to Discord** and the channel for it ([section 8](#posting-the-answers-to-discord)). |
| Config → Hotkey | The mute hotkey. |

The bottom of the **Config** tab shows where `settings.json` is.

## 10. Privacy and your files

What leaves your PC:

- **Between sessions, nothing.** The wake word is recognized locally, by Vosk or, in the
  Whisper mode, by Whisper.
- **During a session,** the channel's audio goes to OpenAI through Codex, and the
  assistant's answer is played in the channel. In the Whisper mode only the text of the
  questions goes, to the engine's provider (OpenAI with Codex, Anthropic with Claude Code,
  the provider you chose with OpenCode), and the answer is read by a voice on your PC
  ([section 7](#what-leaves-your-pc)). With **Always listening**, a session runs the whole
  time the bot is in the channel.
- **The names of the people in the channel** go to the engine's provider at the start of a
  session, unless you turn this off in **Speaker names** in the engine's card.
- **Questions the assistant looks up** may lead to web searches: Codex's and Claude Code's
  go through their own provider, OpenCode's to its web search service, another company.
- **The engine's answers** are posted in Discord, if you turned on **Post answers to
  Discord**. Everyone who can read that channel sees them, and they stay in its history
  until someone deletes them.
- **The engine's own working folder.** Codex and Claude Code put the path of the folder
  they run in (`codex-workspace\` or `claude-workspace\` inside VoiceBro's folder) into
  their own instructions to the model, and OpenCode does the same with
  `opencode-workspace\`. When VoiceBro is installed under your user folder, that path holds
  your Windows user name, and the model may say it if someone asks where it runs.

Everything VoiceBro stores is in its own folder, next to `voicebro.exe`:

| Item | Contents |
|---|---|
| `settings.json` | Your settings. |
| `owner-link.bin` | The Discord account you linked, encrypted so that only your Windows user on this PC can read it. Only if you linked one. |
| `consents.bin` | The people who agreed with `/optin`: their Discord account, name and the time they agreed, encrypted so that only your Windows user on this PC can read it. Only while someone has agreed. |
| `codex-lists.json` | The Codex models, reasoning levels and voices of your account, from the last time VoiceBro read them. |
| `logs\` | The app log, at most about 2 MB, without what people said. The chat history too (`chat-<date>.log`), if you turned on **Save chat history**; files called `codex-chat-<date>.log` come from versions before 1.3.0. |
| `assets\` | What VoiceBro downloaded: the Vosk recognizer and its models, and for the Whisper mode the Whisper runtime and models and the voices. |
| `models\` | Vosk models you added yourself. |
| `whisper-models\` | Whisper models you added yourself. |
| `webview-data\` | The window's browser data. |
| `codex-workspace\` | An empty folder Codex works in. |
| `claude-workspace\`, `claude\` | An empty folder Claude Code works in, and what VoiceBro hands it: your instructions and an empty list of MCP servers. Only after Claude Code ran. |
| `opencode-workspace\`, `opencode\` | An empty folder OpenCode works in, and the configuration VoiceBro gives it. Only after OpenCode ran. |

Outside that folder are only the Discord token in the Windows Credential Manager (entry
`VoiceBro/discord_token`), and the engines' own data, which they write themselves: Codex
with its sign-in in `~\.codex`; Claude Code with its sign-in in `~\.claude` and
`~\.claude.json` (Claude Code rewrites `~\.claude.json` each time it runs and makes an
empty folder for VoiceBro's working folder under `~\.claude\projects`; VoiceBro's
conversations are not saved there); OpenCode with its providers, keys and conversation
database under `~\.local\share\opencode` (VoiceBro deletes its own conversations there when
the bot leaves the channel; after a crash some may stay).

## 11. Updating

Run the new installer. It replaces the program and keeps your settings, models and token.
For the portable zip, replace `voicebro.exe` and keep the rest of the folder.

## 12. Uninstalling

Uninstall VoiceBro in **Windows Settings → Apps**. To remove your settings, logs, models
(the Whisper models and voices too), the linked Discord account and the stored token as
well, check **Delete the application data** in the uninstaller;
leave it unchecked to keep them for a later install. An installed copy and a portable copy
share the stored token, so deleting the data of one removes the token of both.

The engines stay installed. Remove them with `winget uninstall OpenAI.Codex`,
`winget uninstall Anthropic.ClaudeCode` or `winget uninstall Anomaly.OpenCode` if you no
longer need them.

## 13. Troubleshooting

| Problem | What to do |
|---|---|
| The commands do not show up in Discord | Put your server's ID into **Config → Discord guild ID**, or wait up to an hour. |
| `/join` replies "Codex CLI is not installed" or "not signed in" | Do [step 4](#4-set-up-the-engine); the **Status** tab shows what is missing. |
| `/join` replies "Claude Code is not installed" or "Claude Code is not signed in" | Press **Install Claude Code** or **Sign in to Claude Code** on the **Status** tab ([section 4](#claude-code)), then `/join` again. |
| `/join` replies "Claude Code did not start" | The reason follows in the reply and in the **Console**. Run `claude --version` in a terminal to see that Claude Code itself starts, then `/join` again. |
| `/join` replies "Claude Code answers in the Whisper mode only" or "OpenCode answers in the Whisper mode only" | Choose **Whisper** as the **Assistant mode** in **Config → Assistant**, then `/join` again. |
| `/join` replies "OpenCode is not installed" | Press **Install OpenCode** on the **Status** tab ([section 4](#opencode)), or set the path of `opencode.exe` in **Config → OpenCode → OpenCode executable**, then `/join` again. |
| `/join` replies "OpenCode did not start" | The reason follows in the reply and in the **Console**. Run `opencode --version` in a terminal to see that OpenCode itself starts, then `/join` again. |
| `/join` replies that OpenCode has no provider or no usable model | Set up a provider in OpenCode: `opencode auth login` in a terminal ([section 4](#opencode)), press **Check OpenCode**, then `/join` again. |
| With OpenCode the bot says nothing, and the **Chat** tab shows "OpenCode: the answer failed: Request failed: 401." | The provider no longer accepts OpenCode's stored sign-in, although `opencode auth list` still says "stored". Connect it again in a terminal, for example `opencode auth login openai`, then ask again. |
| With OpenCode the **Chat** tab shows "Free usage limit reached" | The answer went to one of OpenCode's own free models, whose use is limited. OpenCode picks one of them as its default even when your provider is connected: choose a model of your provider in **Config → OpenCode → Model**. |
| The notice "A Claude subscription is for you alone" | Claude Code runs on your Claude subscription while other voices reach it. Choose **Only my Discord account** in **Config → Access**, or sign Claude Code in with an API key ([section 4](#a-claude-subscription-is-yours-alone)). |
| The wake phrase does nothing | Look at **Wake word** on the **Status** tab: it says when the model still waits for its download (press **Download** under **Wake model** in **Config → Assistant**, or put the files there yourself as section 5 describes), when no phrases are set, or which words the model does not know. The **Console** shows a "heard:" line for everything the recognizer understood, so you can pick phrases it hears correctly. |
| The bot joins but does not hear or speak | Check that the bot has **Connect** and **Speak** in that channel, and look for errors in the **Console**. |
| The answers are not posted in Discord | Check **Post answers to Discord** in the engine's card. In the realtime mode only answers Codex looked up are posted, not what the voice answers on its own. With **A chosen channel**, only answers from that channel's server go there. Look for an `answers` error on the **Status** tab: it names the channel and what Discord refused, usually a missing **Send Messages** or **Embed Links** ([section 8](#posting-the-answers-to-discord)). |
| `/join` replies "The Whisper mode needs downloads" | The Whisper model, its runtime or the voice is not on this PC yet. Press **Download** in the window that opens in VoiceBro, or in the notices of **Settings → Whisper** and **Settings → Local voice**, or put the files there yourself ([section 7](#downloads)). Then `/join` again. |
| `/join` replies "The wake word needs downloads" | The Vosk recognizer or the wake model is not on this PC yet. Press **Download** in the window that opens in VoiceBro or under **Wake model** in **Config → Assistant**, or put the files there yourself ([section 5](#5-set-the-wake-phrase)). Then `/join` again. |
| Whisper runs on the CPU although you chose the NVIDIA GPU | The line on **Settings → Whisper** and the **Whisper device** tile on the **Status** tab say why: no NVIDIA graphics card or driver, the GPU runtime not downloaded yet, or the CPU runtime was already in use since VoiceBro started, in which case restart VoiceBro. |
| Answers in the Whisper mode come slowly | Look at the last answer's timings on **Settings → Whisper**. A long time from the end of speech to the text means the Whisper model is too slow for this PC: choose a smaller model in **Config → Assistant**, or the NVIDIA GPU. A long time from the text to the engine's first words is the engine: with Codex choose a lower **Reasoning** in **Config → Codex**, with Claude Code or OpenCode a faster **Model** in its card. |
| In the Whisper mode, side talk cuts off the answers | During a session everything said is a question, and a new question replaces the one being answered. End the session with the deactivation phrase before you talk among yourselves, or use **Mute**. |
| The wake phrase does nothing in the Whisper mode | Look at **Whisper** on the **Status** tab: the model must be loaded. The **Console** shows a "heard" line for everything Whisper wrote down; if it writes your phrase differently, use its spelling, or pick a two-word phrase. Choose your language in **Settings → Whisper → Language** instead of **Automatic** if Whisper mixes languages up. |
| A link in a posted answer has no preview | Discord shows no preview for a link the answer puts in parentheses, as in "(source: link)". The link itself works. |
| The assistant's voice stutters in places | The ping from the PC that runs VoiceBro to the channel's Discord voice server is probably too high: at around 300 ms or more the voice can break up. Set the channel's **Region Override** (**Edit Channel → Overview**) to a region close to that PC. See [Limitations](../README.md#limitations). |
| A window says "The bot left the voice channel" and "opus-rs failed while …" | The Opus codec opus-rs, which you chose in **Settings → Diagnostics → Opus codec (experiment)**, failed, so the bot left that channel instead of playing broken sound. The window names the server, the channel and the error. Run `/join` to bring it back, or choose libopus there to go back to the default codec. Please attach `logs\voicebro.log` (**Open log folder** in that window) to a [GitHub issue](https://github.com/OldSlash/voicebro-discord/issues). |
| VoiceBro does not start and names a folder | Install or extract it into a folder you can write to, such as `%LOCALAPPDATA%\VoiceBro`. |
| Something else | Turn on **Settings → Diagnostics → Verbose log**, repeat the problem, and attach `logs\voicebro.log` (**Chat → Open log folder**) to a [GitHub issue](https://github.com/OldSlash/voicebro-discord/issues). With the verbose log on, the file contains what people said, so read it before you share it. |
