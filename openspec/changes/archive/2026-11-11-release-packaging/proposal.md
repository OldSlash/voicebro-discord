# Proposal

## Why

VoiceBro is to be published so that anyone can download and use it. Today `cargo tauri
build` would produce an NSIS installer with Tauri's defaults, but it has never been built,
nothing is tagged, the uninstaller leaves the settings, logs, 155 MB of models, the
WebView2 data and the stored Discord token behind, a per-machine install into Program Files
would break the app (it writes next to its executable), and there is no README: a new user
would not know how to create a Discord bot, install Codex or sign in. The user decided on
2026-11-11: an installer plus a portable zip, everything the app creates inside its own
folder, a full README, and the first public version.

## What Changes

- **The installer for the current user.** NSIS, no administrator rights, default folder
  `%LOCALAPPDATA%\VoiceBro`, the folder selectable, a Start menu entry, the desktop shortcut
  offered on the last page, English or Russian by the system language. Installing a newer
  version over an older one keeps the data.
- **Uninstall removes the data when asked.** With "Delete the application data" checked,
  the uninstaller also removes `settings.json`, `logs\`, `assets\`, `models\`,
  `codex-workspace\`, `webview-data\`, the stored Discord token and then the empty folder;
  unchecked, it leaves them.
- **A portable zip** with `voicebro.exe`, `README.md`, `LICENSE` and
  `THIRD-PARTY-NOTICES.txt`, which runs from any folder the user can write to.
- **A writable folder is required.** At start, when the app's folder cannot be written,
  the app shows a message naming the folder and what to do, and exits.
- **Files stay in the app's folder.** Checked by a run that compares the system folders
  before and after; `docs/ARCHITECTURE.md` is corrected where it says nothing is written
  under the user profile (the token in the Credential Manager and the user's own Codex
  are).
- **One release script.** `scripts\build-release.ps1` builds the installer and the zip
  from `main` at the tag `v<version>` only, writes the third-party notices and a
  `SHA256SUMS.txt`, and puts everything into `target\release\dist\`; a `-Trial` switch
  builds from any commit into `dist-trial\` for testing, never handed over.
- **Third-party notices.** The license texts of every crate built into `voicebro.exe`,
  collected from Cargo's metadata by a script, plus Vosk and its models as components the
  app downloads with the user's OK.
- **README.md**, the user guide: what VoiceBro is and needs; creating the Discord bot
  and inviting it; installing Codex and signing in to ChatGPT; installing VoiceBro (the
  SmartScreen warning of an unsigned build, the checksums); the first settings; wake
  phrases per language and adding other languages; daily use; the diagnostic switches;
  where the files live; updating and uninstalling; troubleshooting.
- **Version 1.0.0** in the app and the installer.

## Capabilities

### New Capabilities

- `app-distribution`: how VoiceBro is built, delivered, installed, updated and removed,
  what a release contains, and where the installed app keeps its files.

### Modified Capabilities

None.

## Impact

- `apps/desktop/tauri.conf.json` (`bundle.windows.nsis`, resources, version),
  `apps/desktop/Cargo.toml` (version, the `windows-sys` message box feature), a new
  `apps/desktop/installer-hooks.nsh`, `apps/desktop/src/main.rs` (the writable check).
- New `scripts\build-release.ps1` and `scripts\third-party-notices.ps1`, a new `README.md`.
- Docs: `docs/ARCHITECTURE.md` (1, 4.6 data directory and the profile, 4.8 the shell, a
  distribution part), `AGENTS.md` (the release commands; installers from the script only),
  `LOCAL-TESTING.md` (installer and zip tests, all inside `target\`).
- No new dependencies. Code signing is not part of this change (postponed by the user).
- Comes last: the README describes the work of `wake-languages`, `release-logging` and
  `codex-onboarding`.
