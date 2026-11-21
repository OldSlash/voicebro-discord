# Security policy

## Supported versions

Only the [latest release](https://github.com/OldSlash/voicebro-discord/releases/latest)
gets security fixes. Before you report, check that the problem is still there in it.

## Reporting a vulnerability

Do not report a vulnerability in a public issue.

Report it privately through GitHub:
[Report a vulnerability](https://github.com/OldSlash/voicebro-discord/security/advisories/new),
also on the **Security** tab of the repository. Only you and the maintainer see the report.
If that form does not open, open an ordinary issue that only asks for a private way to
reach the maintainer, with no details of the problem.

Please include:

- the build stamp from the bottom right corner of the window, for example
  `build 1.3.0+992f382 2026-11-21`, and whether you use the installer, the portable zip or
  a build from source;
- the Windows version;
- the assistant mode and the engine (Config → Assistant), if they matter;
- what an attacker can do, what they need for it (for example a place in the voice
  channel, a Discord account on the server or an account on the PC that runs VoiceBro),
  and the steps to reproduce it.

Never send a Discord bot token or an engine's sign-in. If a log helps, read it first: it
names the people in the channel, and with **Verbose log** on it holds what they said.

## What happens next

VoiceBro is maintained by one person, so answers and fixes come as time allows. The
answer comes in the report itself. Once a fix is released, the advisory is published,
with credit to you unless you ask not to be named.

## Scope

In scope is VoiceBro's own code and what it builds and downloads:

- **The Discord bot token.** It is the only secret VoiceBro stores, and it is kept only in
  the Windows Credential Manager (`VoiceBro/discord_token`), never in a file or the log.
  The engines' sign-ins are kept by Codex, Claude Code and OpenCode themselves; VoiceBro
  never reads them.
- **The owner link and the consents** (`owner-link.bin`, `consents.bin`), encrypted with
  DPAPI for the Windows user.
- **Who the assistant hears** (Config → Access) and the bot's slash commands: being heard
  when the access mode should keep you out, or linking an account as the owner without the
  code.
- **The engines' limits.** Claude Code and OpenCode may only search and fetch the web;
  Codex runs with its shell turned off, a read-only sandbox and every approval request
  declined. A way to make the assistant run a program, or read or change a file on the PC
  that runs VoiceBro, from a voice channel, a Discord message or a web page it reads, is
  in scope.
- **OpenCode's local server**, which listens only on 127.0.0.1 with a new password at each
  start.
- **The downloads** (Vosk, Whisper and the voices), which come over https and are checked
  against a fixed SHA-256 before use.
- **The release files**: the installer, the portable zip and `SHA256SUMS.txt`.

Out of scope:

- Vulnerabilities in Discord, Codex CLI, Claude Code, OpenCode, Vosk, whisper.cpp,
  sherpa-onnx or other third-party software. Report them to their makers, and tell us when
  the way VoiceBro uses them makes one worse.
- MCP servers the operator set up in their own Codex configuration
  (`~\.codex\config.toml`). VoiceBro leaves them on, as the
  [user guide](docs/user-guide.md#10-privacy-and-your-files) says.
- What the model says. An answer that is wrong, rude or talked into something by a prompt
  is not a vulnerability unless it leads to one of the actions in scope above.
- An attacker who already runs code as the same Windows user on the PC that runs VoiceBro.
- The SmartScreen warning: releases are not code-signed yet.

What VoiceBro keeps and sends is described in
[Privacy and your files](docs/user-guide.md#10-privacy-and-your-files).
