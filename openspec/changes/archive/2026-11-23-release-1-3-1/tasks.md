# Tasks

## 1. Version 1.3.1

- [x] 1.1 Set the version to 1.3.1 in `apps/desktop/tauri.conf.json`, `apps/desktop/Cargo.toml`, `crates/voicebro-core/Cargo.toml` and `apps/server/Cargo.toml`, and let `cargo build` update `Cargo.lock`. Verify that `git grep '"1.3.0"\|= "1.3.0"'` finds none of them, and that `Cargo.lock` lists `voicebro-core`, `voicebro-desktop` and `voicebro-server` at 1.3.1
  (2026-11-23: the four files read 1.3.1. `cargo update --workspace --offline` moved only those three lock entries, 6 lines; the console app's debug exe was in use, so the lock was not left to `cargo build`)
- [x] 1.2 Make the command mock's stamp, version and first log line read 1.3.1 and 2026-11-23, and the user guide's `Get-FileHash` example name `VoiceBro_1.3.1_x64-setup.exe`. Verify with a search for 1.3.0 that what is left names a real 1.3.0 build: the stamp example in `SECURITY.md` and the bug report form, the chat log note in the user guide, the specs
  (2026-11-23: the search outside the archive finds exactly those six lines)

## 2. Release notes

- [x] 2.1 Write `release-notes.md` in this change's folder, in the layout of 1.3.0's notes, from the merges in `git log --first-parent v1.3.0..main`. Verify:
  - every merge a user or operator can notice has a bullet;
  - the file names are those the release script writes;
  - updating from 1.3.0 names what an operator has to do: link an owner for the panel's buttons, and look at the privacy policy's part on messages;
  - labels are as the window and Discord show them.

  (2026-11-23: the six merges after `v1.3.0` are covered. cross-platform-headless,
  voice-corpus-tests and the console part of console-and-panel-fixes are under "For those
  who build from source". The community files commit `1bb4c94` changes nothing in the app.
  The file names are those `build-release.ps1` writes. The update part names the owner link
  and the privacy policy's "What the bot receives from Discord". The labels are those of the
  user guide and the panel: Mute assistant, Unmute assistant, Text answers, Leave channel,
  Link my Discord account, Copy, Realtime voice.)

## 3. Checks

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (CMake on PATH) and the four `node apps/desktop/ui-dev/check-*.mjs` checks. Verify that all pass with no clippy error, and report the output
  (2026-11-23: fmt clean; clippy no errors, only the older warnings of `voicebro-core` and `probe`; tests core 559, desktop 17, probe 2, all passed, 0 failed; UI checks: wake dialog 24, Config tab 53, Whisper config 39, engines 29 scenarios, all passed with the mock's stamp at 1.3.1)
- [x] 3.2 Copy `target\release\dist\` (the 1.3.0 files) to `target\release\dist-1.3.0\`, then build with `scripts\build-release.ps1 -Trial`. Verify:
  - the zip holds exactly `voicebro.exe`, `README.md`, `docs\user-guide.md`, `docs\images\*.png`, `LICENSE` and `THIRD-PARTY-NOTICES.txt`;
  - neither file holds wake, Whisper or voice files;
  - both sums equal `Get-FileHash`;
  - `voicebro.exe`'s file version is 1.3.1;
  - the notices script finished without a crate missing its text.

  (2026-11-23:
  - The 1.3.0 files are in `target\release\dist-1.3.0\` and hash the same as the
    originals.
  - The trial: installer 9.0 MB, zip 12.7 MB, about the sizes of 1.3.0's, so no model is in
    either.
  - The zip holds exactly the seven entries, none of them a wake, Whisper or voice file.
  - Both `Get-FileHash` values equal `SHA256SUMS.txt`.
  - `voicebro.exe` has file and product version 1.3.1.
  - The notices script finished. Its 4.1 MB file carries songbird's copyright line, the
    lessac note and `onnxruntime.ai`.
  - The installer's contents were not unpacked.)

## Workflow follow-up

- One commit on `change/release-1-3-1` with the version, the docs, the release notes and this
  change archived. No AI attribution lines. Then the merge into `main` with
  `git merge --no-ff --log change/release-1-3-1`, and the branch deleted (the user's request
  of 2026-11-23).
- Tag `v1.3.1` on `main` (annotated, "VoiceBro 1.3.1", as `v1.3.0`), run
  `scripts\build-release.ps1`, check `SHA256SUMS.txt` against `Get-FileHash`, and write
  `target\release\dist\RELEASE-NOTES.md` with the two SHA-256 values.
- The user pushes and publishes the GitHub release: the notes pasted; the installer, the zip
  and `SHA256SUMS.txt` attached.
