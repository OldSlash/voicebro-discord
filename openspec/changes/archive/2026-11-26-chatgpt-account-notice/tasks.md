# Tasks

## 1. The setting

- [x] 1.1 Add `codex.account_notice_hidden` (default `false`, serde default for older files) to `CodexSettings` in `crates/voicebro-core/src/settings/mod.rs`, with one strict-boolean helper shared with `claude.subscription_notice_hidden` that names each field in its error; verify with unit tests for the default, a 1.3.1 file without the field, `"yes"` refused naming `codex.account_notice_hidden`, and `true` written and read back (`cargo test -p voicebro-core settings::`)
- [x] 1.2 Add the field to the settings table in `docs/ARCHITECTURE.md`; verify the table names it with its default and that it takes only a boolean

## 2. The notice in the window

- [x] 2.1 Check that `https://openai.com/policies/terms-of-use/` and `https://openai.com/policies/service-terms/` open the current Terms of Use and Service Terms (section 8 present); verify by reading both pages
- [x] 2.2 Add `#codex-notice-card` next to `#claude-notice-card` in `apps/desktop/ui/index.html`, turn `.claude-notice` into the shared `.account-notice` class in `styles.css`, and add `renderCodexNotice()`, its Hide (`codex.account_notice_hidden`) and the shared `hearOnlyMe()` in `main.js` as design D1 and D2 say, called wherever `renderClaudeNotice()` is; verify the Claude notice's scenarios in `check-engine-config.mjs` still pass
- [x] 2.3 Give the command mock `?codex_account=chatgpt|api_key|bedrock|gcp|none`, `?codex_notice_hidden` and `setCodexAccount(kind)` (design D6), and the settings check of the new field in `mock.js`; verify the page loads with each parameter and `window.__mock.setCodexAccount("api_key")` hides the notice
- [x] 2.4 Add the `settings-ui` delta's scenarios to `apps/desktop/ui-dev/check-engine-config.mjs` (Everyone, owner-only, Hear only me with a link code, Hide kept after a reload, a refused Hide, an API key, another engine, no dialog); verify `node apps/desktop/ui-dev/check-engine-config.mjs` and the other three UI checks pass
- [x] 2.5 Look at the notice in the mock in all four themes and at the narrow window width; verify by screenshots that it sits at the top of the Codex card and its buttons are not cut

## 3. The notice in the headless app

- [x] 3.1 Log the ChatGPT account notice once at start in `apps/server/src/main.rs` while the engine is Codex and `discord.access` is not `owner`, without reading the account (design D4); verify with `voicebro-server` against a scratch `VOICEBRO_DATA_DIR` that the line appears with `everyone` and not with `owner`, and that no `codex` process starts before `/join`

## 4. The eight languages' wake sets

- [x] 4.1 Read the license of NVIDIA's Hi-Fi TTS dataset (the hifi_tts_92 checkpoint of jirka, siwis, paola and faber) on its own page; verify each of the eight voices of design D5 may have its output published, and record the chain (dataset, base) in the script's voice table
- [x] 4.2 Add `--rate <hz>` to `probe tts` (`apps/probe/src/tts.rs`), resampling with `audio::dsp::Resampler::convert` before the WAV is written; verify a Piper sentence written with `--rate 48000` is a 48 kHz mono 16-bit WAV of the same length
- [x] 4.3 Remove `probe codex-say` (`say` in `apps/probe/src/rtc.rs` and its entry in `apps/probe/src/main.rs`); verify `cargo run -p probe -- list` no longer lists it and `rtc-call` is still there
- [x] 4.4 Replace `-Codex`/`-CodexOnly` with `-Piper`/`-PiperOnly` in `scripts/make-wake-corpus.ps1` (design D5), printing each language's voice and license, and update the wording of `scripts/run-wake-corpus.ps1`; verify `make-wake-corpus.ps1 -PiperOnly -Languages de` makes the six German WAVs and leaves the other folders alone
- [x] 4.5 Make all eight sets with `-PiperOnly` and run `scripts\run-wake-corpus.ps1 -Languages cs,nl,fr,de,it,pl,pt,es`; verify every language has its six WAVs and report per language how many runs heard the default phrases

## 5. Documentation

- [x] 5.1 `docs/user-guide.md`: a section "A ChatGPT account is yours alone" under Codex (the account, the realtime voice's terms, the notice, Hear only me, Hide, an API key), the Access section's "Only my Discord account" naming a ChatGPT account too, and a troubleshooting row for the notice; verify every new link and anchor resolves
- [x] 5.2 `README.md`: a short section on the terms of the account behind each engine (Claude subscription, ChatGPT account, ChatGPT's voice; VoiceBro advises and enforces nothing, the code is open); verify it links to the user guide's sections
- [x] 5.3 `docs/MVP-PLAN.md`: the terms risk's mitigation names the notice of this change; verify the row reads correctly
- [x] 5.4 `AGENTS.md` (local): the probe list without `codex-say`, `probe tts --rate`, the wake corpus made with `-Piper` and its voices, the mock's new parameters; verify no `codex-say` and no `-Codex` switch is left in it

## 6. Checks

- [x] 6.1 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`; verify all pass and say what they returned
- [x] 6.2 Run `.\.tools\openspec\node_modules\.bin\openspec.cmd validate chatgpt-account-notice --strict`; verify it passes

## Workflow follow-up

- Commit, archive and merge only when the user says so (one commit for the change, merged with `--no-ff --log`).
