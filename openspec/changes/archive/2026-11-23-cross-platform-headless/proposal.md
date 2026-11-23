# Proposal

## Why

VoiceBro's bot runtime is already independent of Tauri, but the production entry point and several native dependencies assume Windows. A small console entry point and targeted portability fixes let operators run the same voice bot on Windows and Linux without a GUI. The headless bot offers every assistant mode and engine the desktop app has, so choosing headless never means losing a mode.

Linux support is added but not tested: nothing of it has been compiled or run on Linux yet. Until the user checks it on Linux, every Linux statement in this change describes intended behavior only.

## What Changes

- Add `voicebro-server`, a native console binary for Windows and glibc-based Linux x86-64 (not tested). Reuse the core runtime, the existing settings and the engines' own CLI sign-ins, and handle normal console shutdown.
- Headless runs all three assistant modes (realtime voice, Whisper, text answers), all three engines (Codex, Claude Code, OpenCode) and both session modes, from the same `settings.json` and defaults as the desktop. It composes the engines exactly as the desktop does.
- Share native-library loading and select the platform's libraries and asset tools. On Linux (not tested): Vosk from the official 0.3.45 wheel, whisper.cpp `b5454` from its official Ubuntu x64 build (CPU only: the release has no Linux CUDA build, so `gpu` falls back to the CPU as on a PC without CUDA), sherpa-onnx 2.0.1 and piper-phonemize 1.5.9 from their official Linux x64 shared libraries. The Windows voices and the CUDA runtime stay Windows-only.
- `voicebro-server --install-assets` installs everything the current settings need (wake word, Whisper runtime, model and VAD, voice engine, voice and phonemizer) without starting Discord or an engine. Normal startup lists missing files and downloads nothing. A `/join` refused for missing files points to the command.
- Replace window-only guidance with terminal guidance in headless: missing or signed-out Codex, Claude Code and OpenCode, missing downloads, and the Claude subscription notice, which headless prints at start.
- Headless writes the same file log as the desktop (same filters, detail lines only with the verbose log) and the same lines to stderr, with the build stamp as the first line.
- On Linux (not tested) every engine's child process runs in its own process group and is stopped as a group, so Ctrl+C in the terminal does not reach it directly and a launcher script never leaves the native process behind.
- Fix the Windows voice test that no longer compiles after the import change (`Synth` out of scope).
- Preserve the Windows desktop app, data layout, credential storage and supported features.
- Headless supports every access mode (`everyone`, `agreed`, `owner`) on both platforms. `voicebro-server --link-owner` starts the bot and prints a one-time code in the terminal for `/link`, after reading the engine's account as Link in the window does. The owner link and consent records are encrypted on both platforms: on Windows with DPAPI as on the desktop; on Linux (not tested) with authenticated encryption under a key kept in the user's home folder, readable only by that user and bound to the machine, so that only the same user on the same machine can read them, as with DPAPI.
- `voicebro-server --set-token` reads the Discord token from standard input (without echo on a terminal) and stores it: on Windows in Credential Manager as the desktop does, on Linux (not tested) as `discord-token.bin` in the data directory, encrypted like the records. `DISCORD_TOKEN_FILE` still takes precedence on both platforms, and Linux no longer requires it.
- The window's own access and token controls get terminal commands, run while the bot is stopped: `--unlink-owner` (the Access card's Unlink), `--list-consents` and `--remove-consent <Discord user id>` (its list and Remove), and `--clear-token` (removing the stored token).
- Encryption on Linux (not tested) is optional: the new setting `linux.encrypt_files` (default `true`) turns it off, and then the token, the owner link and the consent records are plain files readable only by the user that runs VoiceBro. The docs say so, and the terminal warns at every start while it is off. Windows always encrypts and ignores the setting.
- Out of scope: window-only features (hotkey mute, Memory and Performance monitoring, the Install and Sign in buttons, voice preview, and the Access card's Cancel, since a waiting code ends with the process), new tests, WAV assets, containers, deployment automation and release changes. Compiling and running on Linux is the user's own check, done later.

## Capabilities

### New Capabilities

- `headless-runtime`: Native console startup, shared settings, every assistant mode, engine and access mode, asset installation, the owner link and the access records from the terminal, terminal output and graceful shutdown.

### Modified Capabilities

- `codex-assistant`: Linux executable discovery (not tested), headless sign-in guidance and terminal guidance for missing Whisper files.
- `claude-assistant`: Linux executable discovery (not tested), headless guidance and the subscription notice in the terminal.
- `opencode-assistant`: Linux executable discovery (not tested) and headless guidance.
- `secret-storage`: The headless Discord token from a file or from `--set-token`, kept in Windows Credential Manager or, on Linux (not tested), in an encrypted file, and removed with `--clear-token`.
- `app-settings`: The Linux encryption setting `linux.encrypt_files`.
- `wake-model-assets`: Platform-specific Vosk runtime and terminal provisioning.
- `whisper-recognition`: The Linux CPU runtime (not tested) and its library names.
- `local-speech-synthesis`: The Linux voice engine and phonemizer (not tested); the Windows voices stay Windows-only.
- `diagnostic-logging`: The headless file log, stderr output and build stamp.
- `owner-only-access`: The owner link and its removal from the terminal, and the encrypted link file on Linux (not tested).
- `speaker-consent`: Removing a record from the terminal, and the encrypted records file on Linux (not tested).

## Impact

Adds one Cargo workspace binary with its own build script for the build stamp. Targeted changes affect the core's native FFI loading, asset tools and catalogs (Vosk, Whisper, voices), executable discovery and diagnostics of the three engines, child-process stopping on Linux, the log filters (moved from the desktop into the core), the engine composition (shared by both shells) and the owner and consent stores with their encryption on Linux. The Linux encryption uses RustCrypto crates already in `Cargo.lock` (`chacha20poly1305`, `hkdf`), added as dependencies of the core on every target so Windows compiles the code Linux runs. Native run instructions accompany the implementation. Windows desktop packaging stays unchanged; the settings schema gains one field, `linux.encrypt_files`, which the window does not show and older files read as `true`.
