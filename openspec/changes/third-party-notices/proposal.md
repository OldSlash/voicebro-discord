# Proposal

## Why

`THIRD-PARTY-NOTICES.txt` lists every Rust crate, but two native libraries compiled into
`voicebro.exe` are missing with their own licenses: Opus 1.5 (BSD-3-Clause) and Microsoft's
WebView2 loader (Microsoft's BSD-style license). Both licenses require their text to come
with every binary copy. The installer's own parts (NSIS and Tauri's plug-in) are not named
at all, and the README does not say what VoiceBro is built on. This has to be fixed before
the first public release.

## What Changes

- The notices name the native code inside crates with its own license text:
  - Opus 1.5, vendored and compiled by `libopus_sys` 0.3.3; the text is read from the
    crate's `opus/COPYING`;
  - the WebView2 loader (`WebView2LoaderStatic.lib` from the Microsoft.Web.WebView2
    1.0.3800.47 SDK, inside `webview2-com-sys` 0.39.1); the crate ships no license text, so
    a copy of the SDK's license from its NuGet page is kept in the repository;
  - the BoringSSL-derived code of `ring` stays covered by the crate's own `LICENSE-*`
    files, and the section says so.
- The notices get a section for the installer: NSIS 3.13 with its `COPYING` (zlib/libpng;
  the LZMA module under CPL 1.0 with a linking exception), kept in the repository; Tauri's
  `nsis_tauri_utils` 0.5.3 plug-in (MIT or Apache-2.0); and Microsoft's WebView2 Runtime
  installer, which the installer downloads only when the runtime is missing.
- The notices get a section for what the app needs but does not ship: the Codex CLI
  (Apache-2.0), installed by the user. Vosk and its models stay in their section.
- The notices script fails when a listed component's license file is missing, so a
  dependency update cannot drop a text silently.
- The README gets a "Built with" section: the main components with links and licenses,
  and where the full list is.
- The README edits already made and not committed come with this change:
  - a Limitations section: the voice only through OpenAI's Codex; a low ping to the
    Discord voice server is needed, at about 300 ms or more the voice can stutter, and
    the channel's Region Override helps;
  - Building from source without the manual `PATH` line: Developer PowerShell for VS 2022
    (checked), `winget install Kitware.CMake`, or a full Visual Studio;
  - the user guide's troubleshooting row for a stuttering voice.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-distribution`: "Third-party notices" covers native code inside crates, the
  installer's components and what the app needs but does not ship; "User guide" adds the
  README's Built with and Limitations sections.

## Impact

- `scripts/third-party-notices.ps1`; new license texts under `scripts/notices/`
  (`WebView2-SDK-LICENSE.txt`, `NSIS-COPYING.txt`, and `nsis_tauri_utils-LICENSE_MIT.txt`,
  since the plug-in's MIT copyright line differs from the `tauri` crate's).
- `README.md`, `docs/user-guide.md`.
- `docs/ARCHITECTURE.md` 4.10 (what the notices contain).
- No change to the app's code or behavior.
