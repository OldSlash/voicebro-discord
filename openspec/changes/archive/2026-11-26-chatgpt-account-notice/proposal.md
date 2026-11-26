# Proposal

## Why

Codex, the default engine, runs on the user's own ChatGPT account. OpenAI's Terms of Use
(effective 2026-01-01) do not let a user make an account available to anyone else, and
OpenAI's Service Terms (section 8, "Voice Conversations", updated 2026-11-13) allow
ChatGPT's voice for non-commercial use only and not distributed or repackaged as a
standalone recording or sound file. VoiceBro already advises the owner-only access mode for
a Claude subscription (changes `claude-and-opencode-engines`, `claude-notice-hide`) but says
nothing about a ChatGPT account. On 2026-11-26 the user asked for the same advice for
Codex: a notice in the Codex card, no dialog, nothing enforced (the code is open, anyone
can change it), and the same in the README.

The old wake corpus's eight other catalog languages (Czech, Dutch, French, German, Italian,
Polish, Portuguese, Spanish) are made by `probe codex-say` through live Codex calls: 48
calls with a ChatGPT sign-in, each checked against the voice's transcript and repeated when
it differs. The app's own catalog already has Piper voices for all eight, with licenses
that allow publishing their output, as the English voice corpus uses. The user asked to
make these sets with free voices instead.

## What Changes

- **A ChatGPT account notice at the top of the Codex card** (the engine card while Codex is
  the engine) while Codex's sign-in is a ChatGPT account (any plan) or not known yet, the
  access mode is not "Only my Discord account", and the user has not hidden it. It says
  that a ChatGPT account may be used only by its holder, that in the realtime voice mode
  ChatGPT's voice is for non-commercial use and may not be recorded and published, links
  to OpenAI's Terms of Use and Service Terms, and offers "Hear only me" and "Hide", as the
  Claude notice does. With an API key, Amazon Bedrock or Google Cloud sign-in it does not
  show. There is no dialog.
- **A new setting `codex.account_notice_hidden`**, default `false`, a boolean only; Hide
  stores `true` and the notice does not show again.
- **The headless app prints the notice once at start** while Codex is the engine and the
  access mode is not `owner`, without starting Codex.
- **The notice enforces nothing**: no join is refused, no session ended, no setting
  changed by it.
- **README and the user guide** say what a ChatGPT account and ChatGPT's voice allow, next
  to the Claude subscription, and that VoiceBro advises and does not enforce.
- **`probe codex-say` is removed.** `scripts/make-wake-corpus.ps1` makes the eight other
  languages' sets with the catalog's Piper voices through `probe tts`, offline once the
  voices are downloaded; `probe tts` gets `--rate` to write the 48 kHz WAVs the corpus
  uses. The switches `-Codex` and `-CodexOnly` become `-Piper` and `-PiperOnly`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `codex-assistant`: the ChatGPT account notice, when it applies and that it enforces
  nothing.
- `settings-ui`: the notice in the Codex card with "Hear only me" and "Hide".
- `app-settings`: `codex.account_notice_hidden`.
- `headless-runtime`: the notice printed once at start with Codex as the engine.
- `voice-corpus`: the old wake corpus's eight other languages are spoken by catalog Piper
  voices whose output may be published.

## Impact

- The window: `apps/desktop/ui/index.html`, `main.js`, `styles.css`; the command mock
  `ui-dev/mock.js` (the Codex account kind and the hidden flag);
  `ui-dev/check-engine-config.mjs`.
- The core: the new field in `settings/mod.rs` with its tests.
- The headless app: `apps/server/src/main.rs`.
- The probes: `apps/probe/src/rtc.rs` and `main.rs` (codex-say out), `apps/probe/src/tts.rs`
  (`--rate`); `scripts/make-wake-corpus.ps1`, `scripts/run-wake-corpus.ps1`.
- Docs: `README.md`, `docs/user-guide.md`, `docs/ARCHITECTURE.md` (the settings table and
  the window's notices), `docs/MVP-PLAN.md` (the terms risk), `AGENTS.md` (local: the
  commands, the corpus and the mock's parameters).
- No new dependency; the version does not change.
