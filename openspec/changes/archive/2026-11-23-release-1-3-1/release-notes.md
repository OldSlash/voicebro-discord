# VoiceBro 1.3.1

VoiceBro runs a Discord bot that lets the people in a voice channel talk to an AI assistant,
started by an offline wake phrase, while they keep playing.

## What's new since 1.3.0

### Buttons on the Discord panel

- The panel that `/join` posts now has three buttons:
  - **Mute assistant** (**Unmute assistant** while muted);
  - **Text answers**;
  - **Leave channel**.
- **Only the linked owner presses them**, whatever **Who the assistant hears** is set to.
  Anyone else gets a reply only they see, and nothing changes. While no Discord account is
  linked, the buttons work for nobody.
- **This changes Mute.** In 1.3.0 anyone in the channel could press it. The **Mute
  assistant** button in the window and the hotkey work as before.
- **Leave channel** does what `/leave` does.
- **Text answers** switches the assistant to the text answers mode for the whole app,
  without the VoiceBro window. The running session ends, the bot stays in the channel, and
  the next answers are posted as text. The press is refused, with the reason, while the
  Whisper files are not on your PC. To switch back to another mode, use **Config →
  Assistant** in the window.
- A panel left over from before a leave or a restart loses its buttons when pressed, and
  says that `/join` posts a new one.

### Linking your Discord account

- **Paste instead of typing.** After **Link my Discord account** in **Config → Access**,
  **Copy** puts one message on the clipboard: the bot's mention, `link` and the code. Paste
  it to the bot in a direct message, or in a channel the bot can read. The bot answers in
  the same chat. `/link` works as before.
- To see that message, the bot now receives message events from Discord. These are not
  privileged intents, so there is nothing to turn on in the Developer Portal. It reads only
  direct messages to it and messages that mention it, only for a link code, and keeps
  nothing of them.

### Assistant modes

- **Realtime voice from Claude Code or OpenCode.** Choosing **Realtime voice** in **Config →
  Assistant** while the engine is Claude Code or OpenCode now switches the engine to Codex in
  the same save, since the voice call works only with Codex. In 1.3.0 that choice was
  greyed out for these engines. Your Claude Code and OpenCode settings stay as they were.

### For those who build from source

- **`voicebro-server`** is a console app that runs the same bot without a window:
  - every assistant mode, engine and access mode, from the same `settings.json`;
  - commands for the token, the downloads, the owner link and the consent records.

  Build it with `cargo build --release -p voicebro-server`, as **Native headless mode** in
  the README describes. It is not part of the downloads. Linux support (glibc, x86-64) is
  added but not tested yet.
- A debug build of the window app no longer opens a console window of its own. Started from
  a terminal, it writes its log there, and closing that terminal leaves it running.
- A voice corpus test plays English recordings through both recognizers, as Discord
  delivers them, with and without noise. The recordings are spoken by synthetic voices whose
  output may be published.

## Updating from 1.3.0

Run the new installer over the old one, or replace `voicebro.exe` in the portable folder.
Your settings, the downloaded models and the stored Discord token are kept.

- **Link your account for the panel's buttons.** If you have not linked your Discord account
  yet, do it in **Config → Access**. Until then nobody can press the panel's buttons, Mute
  included.
- **Update a published privacy policy.** If you published one from
  `docs/PRIVACY-POLICY-TEMPLATE.md`, its part "What the bot receives from Discord" now covers
  the messages the bot receives and why. Bring your copy up to date.

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
| `VoiceBro_1.3.1_x64-setup.exe` | Installing for your Windows user, with a Start menu entry. No administrator rights needed. |
| `VoiceBro_1.3.1_x64-portable.zip` | Running from any folder you can write to, without installing. |
| `SHA256SUMS.txt` | The SHA-256 of both files. |

Check a download in PowerShell and compare the result with `SHA256SUMS.txt`:

```powershell
Get-FileHash .\VoiceBro_1.3.1_x64-setup.exe -Algorithm SHA256
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
