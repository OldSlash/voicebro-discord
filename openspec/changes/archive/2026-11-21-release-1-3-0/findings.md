# Audit findings, 2026-11-20

Three read-only audits of `main` at `b193220`: the docs against the code, data handling,
and licenses. The user decided on 2026-11-20 that the data-handling and license findings
are written down in the docs, the privacy policy template and the notices. They are not
changed in the code, and the voices stay. Line numbers are those of `b193220`.

## Docs (fixed in the docs)

**D-1. `docs/user-guide.md`**

- **Modes.** There are three; the guide knows two.
  - `:340` and `:351` speak of two modes.
  - `:795` lacks "Whisper, text answers".
  - `:597` and `:607` describe `/status` with two modes.
  - `:96-100` and `:348` call Claude Code and OpenCode "Whisper only"; they also run in the
    text answers mode.
- **Commands.** `:602-608` list only `/join`, `/status` and `/leave`. Missing: `/link`,
  `/optin`, `/optout`, `/privacy`, and which commands also work in a DM
  (`discord/handler.rs:37-62`).
- **Settings.**
  - `:794` Audio lacks Earcon volume (default 18 %) and the two gains' labels, range
    (0-400 %) and default (100 %).
  - `:796` lacks Answer handoff (`codex.handoff_mode`: Conversation items, the default,
    Commentary, Tagged channels, Built-in (thinking)).
  - Speaker names has three choices: At call start (default), At call start and when the
    speaker changes, Off (`index.html:469-470`).
  - `:628` places Speaker names in Config → Codex; it is in every engine's card.
  - `:788` names no theme (Dark, Darcula, Light, IntelliJ; default Dark) and no font size
    range (11-20, default 13).
  - The Performance and System tabs are not described.
- **Places.**
  - `:890` sends the user to Config → Assistant for the Whisper model; it is in Settings →
    Whisper.
  - `:247`: the wake model is now *above* the phrases.
  - `:102-104`: an engine switch "may ask to download"; now only the note in Config →
    Assistant lists what is missing.
  - `:887`: the voice may be missing, which does not apply in the text answers mode.
- **Other.**
  - `:663-664` and `:817-819`: posts carry no question; but in the Whisper modes the first
    line holds the question as Whisper wrote it, up to 150 characters
    (`assistant/text.rs:494-501`, `discord/answers.rs:98-106`).
  - `:75`: the `Get-FileHash` example names 1.0.0.
  - `:513`, `:552` and `:567` say "Codex" where any engine answers.

**D-2. `README.md`**

- `:21-46` and `:131-148` lack the text answers mode, and `:133-136` says Claude Code and
  OpenCode answer "only in the Whisper mode".
- `:79-89`: the quick start lets the user pick Claude Code or OpenCode, which means the
  Whisper mode, but covers only the Vosk download, so `/join` is refused.
- `:210-228`: Built with lacks opus-rs 0.1.42.
- `:18`: the alt text says "The Codex chat tab"; the tab is "Chat".
- `:103` and `:208` link to files the zip does not hold.

**D-3. `docs/PRIVACY-POLICY-TEMPLATE.md`**

- `:127` says "Session mode is Always"; the UI says Activation → "Always listening".
- `:119` says "Speaker labels"; the label is "Speaker names".
- There is no text answers variant, and no sentence on the question at the top of a post.
- `:48`: the OpenCode row covers neither a missing model nor the `opencode` provider (Zen).

**D-4. Screenshots.** `docs/images/wake-settings.png` and `docs/images/chat.png` show build
"1.0.0 2026-11-11", the tab "Codex chat", the removed "Wake engine" field and old help
texts.

## Data handling (written into the template and the guide)

- **P-1.** The always-on `logs\voicebro.log` keeps people's names (and the owner's Discord
  id) until it rotates (1 MB plus one old file):
  - the owner link and owner changes (`owner.rs:547-553`);
  - agreed, withdrew and removed (`consent.rs:436,458,460`);
  - every ignored source once per join (`discord/receive.rs:134`);
  - the speaker who stopped an answer (`gate.rs:661`).

  The template (`:37-38`, `:159-161`) says less.
- **P-2.** Closing the app, or switching the engine, while the bot is in a channel with
  OpenCode leaves the guild's session in `~\.local\share\opencode`. `runtime.rs:970-982`
  stops the assistant before Discord, and `assistant/opencode/mod.rs:833-843` kills the
  server without deleting. The template (`:155-158`) and the guide (`:849-850`) say the
  sessions are deleted.
- **P-3.** "Delete the application data" (`apps/desktop/installer-hooks.nsh:27-46`) leaves
  `codex-lists.json`, `claude\` (`instructions.txt` holds the instruction),
  `claude-workspace\`, `opencode\` (`opencode.json` holds the instruction) and
  `opencode-workspace\`, so the folder stays. The guide (`:859-861`) says otherwise.
- **P-4.** After `/optout`, Remove, Unlink or a stricter access mode, audio already held
  can still reach the engine:
  - the utterance being cut, up to 28 s (`stt/cutter.rs`);
  - the 5 s pre-roll (`gate.rs:35`);
  - a floor buffer, up to 15 s (`gate/floor.rs:27`).
- **P-5.** Codex starts with only `--disable shell_tool` (`assistant/app_server.rs:166-168`).
  It runs with the operator's own Codex configuration, and the MCP servers set up there
  are not turned off.
- **P-6.** The verbose log writes recognized speech, answers and names to
  `logs\voicebro.log`, which the guide's "without what people said" (`:834`) leaves out.
  Save chat history writes `logs\chat-<date>.log` with no time limit.
- **P-7.**
  - The engines' providers (OpenAI, Anthropic, the OpenCode provider) handle the data
    under their own terms, model training included.
  - Claude Code and Codex are started without turning off their own telemetry or
    auto-update; OpenCode gets `OPENCODE_DISABLE_AUTOUPDATE`.
  - Claude Code's and OpenCode's web fetch request pages from the operator's PC
    (template `:167-175`).
- **Already fine, and worth saying in the template:**
  - The Discord token is only in the Windows Credential Manager.
  - `owner-link.bin` and `consents.bin` are encrypted for the Windows user (DPAPI).
  - The OpenCode password is new at every start and never written down.
  - OpenCode listens on 127.0.0.1 only.
  - There is no telemetry or update check of the app's own.
  - Downloads use https with a pinned SHA-256.

## Licenses (written into the notices)

- **L-1.** 13 crates have only a license name and a link in `THIRD-PARTY-NOTICES.txt`. Their
  MIT, ISC or BSD-3 license asks for the text with the copyright line:
  - alloc-stdlib 0.3.0 (BSD-3-Clause);
  - convert_case 0.4.0, davey 0.1.5, openmls 0.8.3 and its four helper crates,
    webview2-com and webview2-com-sys 0.39.1 (MIT);
  - serenity-voice-model 0.3.0, songbird 0.6.1 and stream_lib 0.5.2 (ISC).

  `scripts/third-party-notices.ps1:110-130` writes the name and link when the package has
  no license file. README `:230-233` says the notices hold the full texts.
- **L-2.** Crates that offer Apache-2.0 but ship no file (55 crates have no file in all) also
  have no text.
- **L-3.** The Piper voices in `tts/catalog.rs:112-254` are sherpa-onnx conversions of
  piper-voices v1.0.0.
  - Fine-tuned from `lessac`, whose data license (Blizzard 2013) is research only:
    `en_US-libritts_r`, `ru_RU-denis` and `ru_RU-irina` (their MODEL_CARD files in
    `target\whisper-test\assets\`), and alba, siwis, thorsten and davefx (upstream v1.0.0
    cards).
  - `ru_RU-irina`'s dataset license is "Unknown".
  - The other voices are not checked yet.

  The notices give only the data license.
- **L-4.** The CC BY voices libritts_r, alba and siwis have no attribution line (notices
  script `:274-275`).
- **L-5.** piper-phonemize 1.5.9 and the `espeak-ng-data` inside the Piper and Kokoro
  archives are GPL-3.0; the notices give name and source without the text.
- **Already fine:**
  - The crate list comes from `cargo tree`, and no crate was added since v1.0.1.
  - The notices include the license texts of Opus, the WebView2 loader, NSIS, Vosk and the
    ten models, whisper.cpp, the 13 Whisper models, Silero VAD, sherpa-onnx, onnxruntime,
    Supertonic's OpenRAIL-M and Kokoro.
  - The engines are named.
  - The CUDA DLLs are covered under NVIDIA's EULA with its link.
  - `LICENSE` is MIT.
