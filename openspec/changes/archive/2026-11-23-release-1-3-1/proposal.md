# Proposal

## Why

The last release is v1.3.0 (2026-11-21). Six changes were merged since, all on 2026-11-23:

- `cross-platform-headless`;
- `realtime-switches-to-codex`;
- `owner-link-paste`;
- `voice-corpus-tests`;
- `discord-panel-controls`;
- `console-and-panel-fixes`.

On 2026-11-23, after the live checks, the user asked to merge and archive them, raise the
version to 1.3.1, tag it, build the release and prepare the release notes and the files.

## What Changes

- **The version becomes 1.3.1** (the user's decision of 2026-11-23) in
  `apps/desktop/tauri.conf.json` and in the `Cargo.toml` files of the desktop app, the core
  and the console app, with `Cargo.lock` following. The dev probes keep their own version.
- **The command mock's build stamp and the user guide's `Get-FileHash` example** read
  1.3.1.
- **Release notes for 1.3.1** go in this change's folder, covering every merge since
  `v1.3.0` that a user or an operator can notice. After the build from the tag, a copy with
  both SHA-256 values goes to `target\release\dist\`.
- **The release is built** with `scripts\build-release.ps1` from `main` at the tag
  `v1.3.1`: the installer, the portable zip and `SHA256SUMS.txt`, as for 1.3.0. The 1.3.0
  files in `target\release\dist\` are copied to `target\release\dist-1.3.0\` first, because
  the script deletes that folder.

Not in this change:

- New screenshots. Both images still show build 1.3.0, and nothing they show has changed.
- The build stamp example in `SECURITY.md` and the bug report form. It shows a real 1.3.0
  stamp, and a 1.3.1 stamp would need the hash of the tagged commit.
- `voicebro-server` in the downloads: the release script packs the window app only, and
  the README's Native headless mode builds the console app from source.
- An update test from the 1.3.0 installer.
- Any code or behavior change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. The behavior of the merged changes is in their archived specs. A release changes no
requirement, so the change sets `skip_specs`.

## Impact

- Version files: `apps/desktop/tauri.conf.json`, `apps/desktop/Cargo.toml`,
  `crates/voicebro-core/Cargo.toml`, `apps/server/Cargo.toml`, `Cargo.lock`.
- Docs: `docs/user-guide.md` (the `Get-FileHash` example); the mock's stamp in
  `apps/desktop/ui-dev/mock.js`.
- Release: `openspec/changes/release-1-3-1/release-notes.md`; the tag `v1.3.1` on `main`;
  `target\release\dist\` with the installer, the zip, `SHA256SUMS.txt` and
  `RELEASE-NOTES.md`. Nothing is pushed: the user publishes the release.
- The branch is `change/release-1-3-1` from `main`.
