# Design

## Context

See proposal.md for the motivation. What exists:

- The Claude subscription notice (changes `claude-and-opencode-engines`, `claude-notice-hide`):
  `renderClaudeNotice()` in `apps/desktop/ui/main.js` fills a box at the top of the engine
  card (`#claude-notice-card`) with the text, two links, "Hear only me" and "Hide";
  `hearOnlyMe()` saves `discord.access = "owner"` and starts a link code when nobody is
  linked; `hideClaudeNotice()` saves `claude.subscription_notice_hidden`. A one-time dialog
  opens at the first choice of Claude Code. The headless app runs `claude.check()` at start
  and logs the notice (`apps/server/src/main.rs`, `subscription_notice()`).
- Codex's account: `AssistantStatus.account_type` and `plan` come from the app-server's
  `account/read` (`codex/account.rs`). The app-server's schema (Codex CLI 0.167.0) has four
  account types: `chatgpt` (with `planType`), `apiKey`, `amazonBedrock` and `gcp`. Before
  Codex has run in this process, `account_type` is `None`.
- Codex must not start for a status read (`codex-assistant`, "Status reads start no
  process"); it starts at `/join`, Sign in, Check, Refresh lists, or the lists read at start.
- `make-wake-corpus.ps1 -Codex` calls `probe codex-say` once per sentence (48 sentences,
  eight languages, voices `juniper` and `cove`), each a live call. `probe tts` writes a
  sentence at the voice's own rate (Piper: 22.05 kHz); the old corpus reads 16 kHz or
  48 kHz WAVs.

## Goals / Non-Goals

**Goals:**

- The same advice for a ChatGPT account as for a Claude subscription, in the same place
  and with the same buttons, minus the dialog.
- The headless app says it once at start without starting Codex.
- The eight other languages' wake sets made offline by catalog voices whose output may be
  published.

**Non-Goals:**

- Enforcing anything, or detecting how the bot is used (commercial or not, recorded or not).
- An API-key sign-in path for Codex in the window: the app still offers only the ChatGPT
  sign-in; a key set up outside VoiceBro is only recognized so the notice stays away.
- Changing the Claude notice or its dialog.
- Moving the eight languages' sets into the published voice corpus.

## Decisions

### D1. When the notice applies

The window decides from the Codex status: `account_type` `chatgpt`, `None` (not read yet,
not signed in) or any type it does not know shows the notice; `apiKey`, `amazonBedrock` and
`gcp` do not. As with Claude's unknown sign-in, not knowing yet counts as a ChatGPT account:
that is the only sign-in the app offers. The rest of the condition mirrors the Claude
notice: engine Codex, `discord.access` not `owner`, `codex.account_notice_hidden` false. The
assistant mode does not matter: the account is shared in the Whisper and text answers modes
too; the voice sentence names the realtime voice mode.

Alternative: show it only after Codex read a ChatGPT account. Rejected: on a first start
nothing has been read, and the notice would appear only after the first `/join`, when the
channel already uses the account.

### D2. The window: a second box in the engine card, no dialog

A box `#codex-notice-card` next to `#claude-notice-card` at the top of the engine card,
filled and shown by `renderCodexNotice()`, called wherever `renderClaudeNotice()` is (the
status refresh, the access mode change, the engine change, a settings save). The two notices
share one stylesheet class (`.account-notice`, which `.claude-notice` becomes) and one
`hearOnlyMe()`, whose busy flag and re-render cover both. "Hide" saves
`codex.account_notice_hidden: true`; a refused save leaves the reason under the notice, as
in the Claude notice. No dialog: the user decided so on 2026-11-26, and Codex, the default
engine, has no "first choice" to hang one on.

Text (English, as all UI text):

- "A ChatGPT account (Free, Go, Plus, Pro, Business, Enterprise or another ChatGPT plan) may
  be used only by you. OpenAI's Terms of Use do not let you make your account available to
  anyone else. While other people's voices reach the assistant, they use your account."
- "In the realtime voice mode the assistant speaks with ChatGPT's voice. OpenAI's Service
  Terms allow that voice for non-commercial use only and do not allow distributing it as a
  recording or a sound file: do not record and publish what the assistant says."
- "Let only your own voice through with "Only my Discord account" in the Access card
  (everyone still hears the answers). VoiceBro does not enforce this: the choice is yours."
- Links: "OpenAI Terms of Use" (`https://openai.com/policies/terms-of-use/`) · "OpenAI
  Service Terms" (`https://openai.com/policies/service-terms/`). Both URLs are checked
  before use; a region page is not linked.

### D3. The setting

`codex.account_notice_hidden: bool`, default `false`, serde default for older files, and
only `true` or `false` accepted, with an error that names the field. The Claude flag's
strict deserializer names its own field in the error; it becomes one helper that both
fields use with their own names. No `shown` flag: there is no dialog.

### D4. The headless notice: at start, without a check

At start, while the engine is Codex and the access mode is not `owner`, the headless app
logs the notice once (a warning, as the Claude notice), naming `discord.access` in
`settings.json` and `voicebro-server --link-owner`, with both links. It does not read the
account: that would start the app-server at start, which the Codex lifecycle forbids
outside the lists read. The text therefore says it applies to a ChatGPT sign-in and not to
an API key, Amazon Bedrock or Google Cloud.

Alternative: print it after the first account read of the run (lists at start or the first
join). Rejected: more state for a line that is most useful before anyone joins.

### D5. The eight languages' voices

`-Piper` and `-PiperOnly` replace `-Codex` and `-CodexOnly`; `-Languages` and `-Force` stay;
`-Probe` stays (a copy of `probe.exe` lets cargo rebuild meanwhile). For each sentence the
script runs `probe tts <voice id> "<text>" <out.wav> --rate 48000 --assets <assets dir>`;
the probe downloads a missing voice with its pinned hash. The sentences and phrases of each
language stay as they are; only the voice changes. `trigger.wav`, the per-language
`transcripts.txt` and the retry loop go: Piper reads the text as given.

The voices, from the model cards read on 2026-11-26 (all are fine-tuned Piper medium
voices; none from the lessac checkpoint):

| Language | Voice | Dataset | Base checkpoint |
|---|---|---|---|
| Czech | `piper:cs_CZ-jirka-medium` | CC0 | hifi_tts_92 (US English) |
| Dutch | `piper:nl_NL-pim-medium` | CC0 | rdh (CC0) |
| French | `piper:fr_FR-siwis-medium` | CC BY 4.0 | hifi_tts_92 |
| German | `piper:de_DE-thorsten-medium` | CC0 | rdh (CC0) |
| Italian | `piper:it_IT-paola-medium` | CC0 | hifi_tts_92 |
| Polish | `piper:pl_PL-darkman-medium` | CC0 | rdh (CC0) |
| Portuguese | `piper:pt_BR-faber-medium` | CC0 | hifi_tts_92 |
| Spanish | `piper:es_ES-davefx-medium` | CC0 | rdh (CC0) |

Where the catalog has two voices, the one whose base checkpoint is CC0 (rdh) is taken
(Polish darkman over gosia, Spanish davefx over ald). The hifi_tts_92 checkpoint comes from
NVIDIA's Hi-Fi TTS dataset; its license is read on its own page before the sets are made,
and a voice whose chain does not allow publishing its output is not used.

`probe tts` gets `--rate <hz>`: the synthesized clip goes through
`audio::dsp::Resampler::convert` before the WAV is written. `probe codex-say` (`say` in
`apps/probe/src/rtc.rs`) and its entry in the probe list go; `rtc-call` stays.

Alternatives: Supertonic 3 speaks all eight from one pack, but its OpenRAIL-M license
carries use restrictions and one pack would give every language the same few speakers;
Kokoro has none of Czech, Dutch, German or Polish; the Windows voices' output may not be
published and this PC has none of these languages.

### D6. The command mock and its check

`mock.js`: `?codex_account=chatgpt|api_key|bedrock|gcp|none` sets the Codex account type
the status carries (default `chatgpt` while signed in), `?codex_notice_hidden` stores the
flag, and `setCodexAccount(kind)` changes the type. `check-engine-config.mjs` gets the
scenarios of the `settings-ui` delta: the notice with Everyone, gone with owner-only, Hear
only me with a link code, Hide stored and kept after a reload, a refused Hide, an API key,
another engine, and no dialog.

## Risks / Trade-offs

- [The Piper voices are heard differently by Vosk than the earlier sets] → the corpus is
  run per language after the sets are made and the results are reported; the sets are test
  material, the app's behavior does not change.
- [An API-key user of the headless app sees the notice at every start] → the text says
  when it does not apply; the window, which reads the account, shows nothing then.
- [OpenAI moves its terms pages] → the links point to the policy pages, not to anchors.
- [The notice can be ignored] → by design (the user's decision): VoiceBro advises only.

## Migration Plan

The new field loads as `false` from older settings files and is written at the next save.
Nothing else migrates; going back to an older build ignores the field.
