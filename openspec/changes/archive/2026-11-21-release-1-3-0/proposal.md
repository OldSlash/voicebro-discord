# Proposal

## Why

The last public release is v1.0.1 (2026-11-14). Fourteen changes have been merged since,
and the version already reads 1.3.0. On 2026-11-20 the user chose to publish 1.3.0 and
asked for:

- release notes to paste;
- the installer, the portable zip and `SHA256SUMS.txt`, built as before. GitHub adds the
  source archive itself;
- a check that the setup docs are current and describe every setting;
- a legal check that nothing is stored unprotected or leaks.

Three read-only audits ran on `main` at `b193220` on 2026-11-20. `findings.md` holds what
they found.

- **Docs.** The docs lag behind the code:
  - the text answers mode, four of the seven slash commands and several settings are
    missing;
  - the quick start omits the Whisper downloads;
  - the posted question is not mentioned;
  - both screenshots show build 1.0.0.
- **Data handling.** The privacy policy template and the user guide do not say what the
  app actually keeps or passes on: names in the log, OpenCode's leftovers, what the
  uninstaller leaves, the engines' own data use.
- **Notices.** 13 crates have no license text.
- **Voices.** The voices' base models are not named.

The user's decision of 2026-11-20: these findings are written down in the docs, the privacy
policy and the notices. They are not changed in the code, and the voices stay.

## What Changes

- **The docs match 1.3.0.** The user guide describes every setting the window shows (label,
  card, choices, default), all seven commands with the DM ones, and the three modes. The
  README covers the quick start with the Whisper downloads, the Built with table (opus-rs)
  and the modes. The two screenshots are retaken.
- **The privacy policy template and the guide's privacy part say what the code does:**
  - the log keeps people's names and Discord ids;
  - the verbose log and Save chat history keep speech and answers, the chat history with
    no time limit;
  - a post starts with the transcribed question;
  - closing the app or switching the engine while the bot is in a channel with OpenCode
    leaves that conversation in OpenCode's own storage;
  - audio already held when someone withdraws can still reach the engine;
  - "Delete the application data" leaves `codex-lists.json` and the engines' folders,
    which hold the instruction;
  - the engines' providers handle the data under their own terms, model training
    included, and the engines may send their own telemetry;
  - Codex runs with the operator's own Codex configuration, MCP servers included;
  - web fetch requests pages from the operator's PC;
  - the text answers mode posts every answer.
- **The notices carry what the licenses ask.** They get a license text for every crate: a crate
  whose package has no license file gets the standard text of its license, kept once per
  license, with its authors named. They get the GPL-3.0 text of the phonemizer, and for each voice the
  data license, the model it was fine-tuned from with that model's license, and the CC BY
  attribution.
- **An update check from 1.0.1.** The kept 1.0.1 installer runs, then the 1.3.0 trial is
  installed over it. Settings, instruction, model and token must carry over.
- **Release notes for 1.3.0** go in the change folder. After the build from the tag, a
  paste-ready copy with the SHA-256 values goes to `target\release\dist\`.
- **No version change, no code change in the app.** The tag `v1.3.0` goes on `main` and
  the release is built when the user says so.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-distribution`:
  - the notices carry a license text for every crate and the voices' licenses with
    their base models;
  - the user guide and the privacy policy template describe every setting and command
    and what the app keeps and passes on;
  - a release comes with release notes.

## Impact

- **Docs:** `README.md`, `docs/user-guide.md`, `docs/PRIVACY-POLICY-TEMPLATE.md`,
  `docs/images/wake-settings.png`, `docs/images/chat.png`, and the distribution part of
  `docs/ARCHITECTURE.md`.
- **Notices:** `scripts\third-party-notices.ps1`, and new license copies under
  `scripts\notices\`.
- **Command mock:** its build stamp, for the screenshots.
- **Release:** the release notes and `scripts\build-release.ps1` from the tag. The 1.0.1
  artifacts in `target\release\dist\` are copied out first, because the script deletes
  that folder.
- No change to the app's code or behavior, no new dependency. The branch is
  `change/release-1-3-0` from `main`.
