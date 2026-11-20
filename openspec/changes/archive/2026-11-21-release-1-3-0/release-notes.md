# VoiceBro 1.3.0

VoiceBro runs a Discord bot that lets the people in a voice channel talk to an AI assistant,
started by an offline wake phrase, while they keep playing.

## What's new since 1.0.1

### Assistant modes and engines

- **Whisper mode.** A second way to talk to the assistant: Whisper turns speech into text on
  your PC (CPU or NVIDIA GPU), the engine answers the text, and a local voice reads the
  answer into the channel. Only text leaves your PC.
- **Whisper, text answers.** A third mode: the same as the Whisper mode, but each answer is
  posted as text in Discord instead of being spoken.
- **Three engines.** Besides Codex, the answers in the Whisper modes can come from Claude
  Code, with your Claude sign-in or an API key, or from OpenCode, with the AI providers you
  set up in it. Each is installed separately and may only search and fetch the web. Switch
  in Config → Assistant, also while the bot is in a channel.
- **A Claude subscription is yours alone.** When you pick Claude Code on a subscription,
  VoiceBro explains why and offers **Hear only me**. The notice lives in the Claude Code
  card, and **Hide** puts it away for good.

### Who the assistant hears

- **Who the assistant hears** (Config → Access): everyone in the channel (the default),
  only people who agreed with `/optin`, or only your own Discord account, linked once with a
  one-time code (`/link`).
- **New commands:** `/link`, `/optin`, `/optout` and `/privacy`. All four also work in a
  direct message to the bot.
- **Consent records and the owner link** are kept in VoiceBro's folder, encrypted for your
  Windows user.
- **A privacy policy template** for the bot's operator: `docs/PRIVACY-POLICY-TEMPLATE.md`.

### Talking to it

- **One speaker at a time** (optional): people who talk over each other reach the assistant
  one after another instead of mixed, and in the Whisper mode a new question joins the
  running answer instead of cutting it.
- **Stop the answer when someone starts talking** (optional, Whisper mode): the spoken
  answer stops once someone has talked for 0.6 s.
- **Wake phrases** are separated by commas and may have several words. Every wake model
  comes with default phrases, and the deactivation phrase now works while the assistant is
  speaking.
- **Answers posted to Discord** (optional): the engine's full answers, with their links and
  previews, in the channel of your choice, without pinging anyone. In the Whisper modes each
  post starts with the question it answers.

### Voices and recognition

- **Local voices** for the Whisper mode: Supertonic 3 (ten voices, 31 languages, the
  default), 17 Piper voices, Kokoro English and the voices Windows has, with **Preview**.
- **Thirteen Whisper models**, from tiny to large-v3, Russian-tuned models among them.
  Whisper and the voice load at `/join` and unload after the last `/leave`, and the GPU's
  memory is given back.

### The window

- The bot's settings are on the new **Config** tab. Your PC's settings (appearance,
  diagnostics, Whisper, the local voice) are on **Settings**.
- Every card folds from its title, and every on/off setting is a large switch.
- Models and their downloads are in their cards. A download window opens only when a
  `/join` was refused for missing files.
- The Codex model and voice lists are kept between starts, so they are there without
  starting Codex.
- The **Codex chat** tab is now **Chat**. A saved chat history goes to
  `logs\chat-<date>.log`.

### Docs and licenses

- The user guide covers the three modes, every setting with its default and all seven
  commands. Its privacy part says what VoiceBro keeps on your PC and what it passes on.
- `THIRD-PARTY-NOTICES.txt` now carries the full license text of every crate in
  `voicebro.exe`, and for every downloadable voice where its recordings come from and the
  voice it was fine-tuned from.

## Updating from 1.0.1

Run the new installer over the old one, or replace `voicebro.exe` in the portable folder.
Your settings, the downloaded models and the stored Discord token are kept:

- The bot's settings move from the Settings tab to the Config tab.
- Your instructions for the assistant stay in the engine's card, and are now one text for
  every engine.
- Your wake phrases stay. The field shows them separated by commas.

## Requirements

- Windows 10 or 11, 64-bit.
- A Discord bot of your own in a server where you can add it.
- One engine:
  - a ChatGPT account with access to Codex, for every mode;
  - Claude Code with a Claude account or an Anthropic API key, for the Whisper modes;
  - OpenCode with an AI provider set up in it, for the Whisper modes.
- For Whisper on the GPU: an NVIDIA graphics card.

## Downloads

| File | For |
|---|---|
| `VoiceBro_1.3.0_x64-setup.exe` | Installing for your Windows user, with a Start menu entry. No administrator rights needed. |
| `VoiceBro_1.3.0_x64-portable.zip` | Running from any folder you can write to, without installing. |
| `SHA256SUMS.txt` | The SHA-256 of both files. |

Check a download in PowerShell and compare the result with `SHA256SUMS.txt`:

```powershell
Get-FileHash .\VoiceBro_1.3.0_x64-setup.exe -Algorithm SHA256
```

The files are not code-signed yet, so Windows SmartScreen may warn about an unknown
publisher: choose **More info**, then **Run anyway**.

## Known limitations

- The realtime voice works only through OpenAI's Codex with a ChatGPT sign-in. Claude Code
  and OpenCode answer only in the Whisper modes.
- At a high ping from your PC to the channel's Discord voice server, around 300 ms or more,
  the voice can stutter. Set the channel's Region Override to a region close to your PC.
- On a CPU only the smaller Whisper models answer quickly.
- Most Piper voices were fine-tuned from a voice whose recordings are licensed for research
  only, and one Piper voice's recordings have no stated license.
  `THIRD-PARTY-NOTICES.txt` lists each voice.
