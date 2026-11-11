# Proposal

## Why

The README written in change `release-packaging` reads like a specification, not like the
page people see first on GitHub: it opens with twelve numbered steps, has no description of
what the app does, no screenshot and no table of contents, carries internal check notes
("checked against Discord's developer documentation on …") and stilted phrases, and gives
building from source three lines. The user compared three structures on 2026-11-11 and
chose a short landing README with the full guide in a separate document.

## What Changes

- **README.md becomes a landing page:** a centered header with the icon, the one-line
  description and links (Download, Quick start, User guide, Build from source), a
  screenshot, Features, Requirements, Download (installer or zip, the SmartScreen note), a
  Quick start of five short steps, Building from source and License.
- **Building from source** becomes a real section: prerequisites with versions (Rust
  stable MSVC 1.89+, Visual Studio 2022 Build Tools with a Windows SDK, the CMake they
  include and how to put it on `PATH`, Tauri CLI 2), the commands to clone, build, test and
  run, the trial installer, and a short map of the repository.
- **License** becomes its own section: MIT with the copyright line of `LICENSE`, the
  third-party notices, Vosk and its models (Apache-2.0, downloaded only after the OK),
  Codex CLI installed separately, and no affiliation with Discord or OpenAI.
- **`docs/user-guide.md`** takes the detailed setup and use, rewritten in plain language,
  with a table of contents and tables: the Discord bot, installing, connecting, Codex,
  wake phrases, other languages, everyday use, settings, privacy and files, updating,
  uninstalling, troubleshooting.
- **Screenshots** in `docs/images/`, taken from the command mock so that no real server,
  token or conversation appears; the icon as a PNG for the header. The mock's build stamp
  says 1.0.0.
- **The portable zip** also carries `docs\user-guide.md` and the images the two documents
  use, so the README's links work offline.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-distribution`: the zip's contents (the guide and its images), and what the README
  and the user guide each cover.

## Impact

- `README.md` (rewritten), new `docs/user-guide.md`, new `docs/images/` (icon, window
  screenshots).
- `scripts/build-release.ps1` (the zip's contents), `apps/desktop/ui-dev/mock.js` (the
  build stamp).
- Docs: `docs/ARCHITECTURE.md` (the layout tree and 4.10), `LOCAL-TESTING.md` (the zip's
  contents).
- No code of the app changes.
