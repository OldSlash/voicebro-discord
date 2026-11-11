# Proposal

## Why

A public user installs VoiceBro and has no Codex CLI. Today the app only says "codex not
found: install Codex CLI (npm install -g @openai/codex) or set its path in Settings", which
assumes Node.js and a terminal. The user decided on 2026-11-11 that the app keeps using the
user's own Codex (its sign-in in `~\.codex`) and, when Codex is missing, offers to install
it, then to sign in.

## What Changes

- **An install offer when Codex is missing.** The Status tab shows "Codex CLI is not
  installed" with what an install does (Windows Package Manager, package `OpenAI.Codex`
  from OpenAI's GitHub releases, about 200 MB to download and 370 MB on disk) and an
  "Install Codex" button. The button opens a visible console window running
  `winget install --id OpenAI.Codex --exact --source winget`; winget's own prompts are
  answered there by the user, nothing is accepted on their behalf. When the window closes,
  the app looks for Codex again and shows the result: the version found, or winget's exit
  code with a pointer to the console's output.
- **No winget, no dead end.** When `winget.exe` is not present, the notice says so and
  gives the two other ways: `npm install -g @openai/codex`, or the download from
  github.com/openai/codex/releases with its path set in Config → Codex executable.
- **Codex installed by winget is found without a restart.** The finder also looks in
  winget's command links folder (`%LOCALAPPDATA%\Microsoft\WinGet\Links\codex.exe`), which
  a running app does not have on its PATH after an install; each status read finds a newly
  installed Codex.
- **Sign in follows.** Once Codex is found and the account read says not signed in, the
  Status tab's Codex area says "Codex is installed; sign in to ChatGPT" next to the
  existing "Sign in to Codex" button.
- **`/join` says what to do.** With Codex missing, the bot's reply says to press Install
  Codex on the app's Status tab; the status error text says the same, instead of the npm
  command.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `codex-assistant`: where Codex is looked for (winget's links folder), the reply to
  `/join` and the status text when Codex is missing.
- `status-view`: the Codex setup notice with the install offer, the winget-less
  instructions, and the sign-in step.

## Impact

- `crates/voicebro-core/src/assistant/codex/app_server.rs` (`find_codex` order),
  `codex/mod.rs` (error and status texts, the join reply).
- `apps/desktop/src/main.rs` (commands `codex_install_info` and `codex_install`: winget
  presence, the visible console, the exit code), `apps/desktop/ui/index.html`, `main.js`,
  `styles.css` (the notice on the Status tab), `apps/desktop/ui-dev/mock.js`.
- Docs: `docs/ARCHITECTURE.md` 4.5 (finding Codex, the install offer) and 4.8 (Status
  tab).
- Depends on `release-logging` (D4 there): the cached version is re-read when the found
  executable changes. No new dependencies. Installs nothing by itself: winget runs only
  after the user's click, in a window they see.
