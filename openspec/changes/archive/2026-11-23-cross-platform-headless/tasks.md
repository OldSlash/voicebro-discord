# Tasks

Linux support is added but not tested: the Linux tasks below are written and implemented without a Linux build or run. The user checks Linux later (group 11).

## 1. Shared core portability

- [x] 1.1 Add the crate-private native loader and route Vosk, Whisper and Sherpa/Piper through it. Preserve Windows DLL search flags and preload order, use explicit paths on Linux, and keep WinRT imports Windows-only.
- [x] 1.2 Add the pinned Linux x86-64 Vosk wheel to the existing asset store; install `vosk/libvosk.so` at the configured marker path. Resolve `curl` and `bsdtar` on Linux, retaining Windows system tools and window hiding. Leave the Windows Vosk asset, model catalog and recognizer unchanged.
- [x] 1.3 Resolve the configured Codex executable or `codex` on Linux `PATH`; keep the Windows lookup order and existing app-server authentication and restrictions.

## 2. Native headless entry point

- [x] 2.1 Add the `apps/server` workspace package and lockfile entry. Wire the existing settings, secret store, event bus, Codex assistant and runtime. Reject unsupported settings before connecting, use `DISCORD_TOKEN_FILE` or Windows Credential Manager as specified, and surface startup and connection failures without exposing credentials. (Superseded in part by 3.1 and 3.2: every mode and engine; and by 8.5: the stored token and `--set-token`.)
- [x] 2.2 Add `--install-wake-assets` using the selected model and existing asset installer, with no Discord or Codex startup. List missing assets on normal startup without downloading; make missing-wake and Codex join guidance usable from the terminal while retaining desktop guidance. (Superseded by 5.1: `--install-assets`.)
- [x] 2.3 Handle Ctrl+C, Linux SIGTERM and best-effort Windows console close through shared runtime shutdown. Check the production builds with `cargo check --locked -p voicebro-server` and `cargo check --locked -p voicebro-desktop` on Windows. (The Linux check moved to group 11.)
- [x] 2.4 Document native Windows and Linux prerequisites, build and run commands, writable data paths, token and Codex setup, explicit asset installation, supported settings and shutdown in `README.md`. Update `docs/ARCHITECTURE.md` and the local `AGENTS.md` for the new entry point and shared loading. (Brought up to date again in 10.1.)

## 3. Every mode and engine in headless

- [x] 3.1 Move the engine composition (Codex, Claude Code, OpenCode behind `EngineSwitch`) from the desktop into a core helper used by both shells; the desktop keeps the per-engine handles its commands need, and its behavior does not change. Headless builds its assistant through it.
- [x] 3.2 Replace the headless validation: `Settings::validate` on both platforms; on Linux (not tested) also reject a Windows voice in the Whisper mode with a message naming the field. Every access mode is accepted (group 8). The settings file is never rewritten on a rejection.
- [x] 3.3 Give every window-only message a terminal variant through `EventBus::for_terminal`: Claude Code and OpenCode not found, not signed in or without a usable model; the Whisper and voice join refusals and their "waiting for the OK in the window" log lines (the owner-link line is 8.4). Keep the desktop texts unchanged.
- [x] 3.4 Print the Claude subscription notice once at headless start under the conditions of the requirement "The Claude subscription notice", reading the sign-in kind the way the desktop does; it refuses nothing.
- [x] 3.5 Find `claude` and `opencode` on Linux `PATH` (not tested) after their configured paths; keep the Windows lookups unchanged.

## 4. Linux Whisper and voice runtimes (not tested)

- [x] 4.1 Probe each official Linux archive before pinning it: whisper.cpp `b5454` `whisper-bin-ubuntu-x64.tar.gz`, sherpa-onnx `v2.0.1` `linux-x64-shared-lib.tar.bz2`, piper-phonemize `v1.5.9` `linux-x64-shared-lib.tar.bz2`. Record the SHA-256, the size, the library file names, sonames, symlinks and the glibc version they need, from the downloaded files (into `target\`, never `%TEMP%`).
- [x] 4.2 Add the Linux assets to the catalogs with `root` and `keep`, keeping only the libraries the app loads (and espeak-ng data for the phonemizer). Leave the Windows assets unchanged. Linux has no CUDA runtime.
- [x] 4.3 Add per-platform library name tables for the Whisper runtime, the voice engine and the phonemizer, and on Linux load each library's dependencies first by absolute path in dependency order. Keep the Windows names, flags and order.
- [x] 4.4 On Linux, a `gpu` request takes the existing CPU fallback with a reason that names the missing Linux CUDA runtime; the Windows voices list nothing on Linux.

## 5. Asset installation from the terminal

- [x] 5.1 Replace `--install-wake-assets` with `--install-assets`: install what the current settings need, through the same requirement functions as the join (Vosk for realtime with the wake word; Whisper runtime for the chosen device, model and VAD for the Whisper and text answers modes; voice engine, voice and phonemizer for the Whisper mode). Print each file with its source and size, the phonemizer's GPL-3.0 license, and progress. No Discord, no engine, no settings change, no change to the operator's own models.
- [x] 5.2 Normal startup lists every missing file of the current mode and downloads nothing; each join refusal for missing files names `voicebro-server --install-assets`.

## 6. Log and build stamp

- [x] 6.1 Move the desktop's log filters (app targets, detail lines, songbird's receive task, verbose log) into the core; the desktop uses them from there with no change in what its file holds.
- [x] 6.2 Give headless a logger that writes `logs/voicebro.log` in the data directory (1 MB, one old file kept, local time) and the same lines to stderr, with the shared filters and `diagnostics.verbose_log`. The terminal reporter stops printing log events itself and keeps the Discord-error exit and the missing-asset summary.
- [x] 6.3 Add a build script to `apps/server` that computes the same build stamp as the desktop; write it as the first line of every start and pass it to the runtime. The version stays 1.3.0.

## 7. Child processes on Linux (not tested)

- [x] 7.1 Start the Codex app-server, Claude Code and OpenCode's server in their own process group on Linux; stop each group with SIGTERM, then SIGKILL after a short timeout. Keep the Windows spawning unchanged.

## 8. Access modes in headless

- [x] 8.1 Add `voicebro-server --link-owner`: a normal start, then the selected engine's account read as Check does and `Runtime::start_link`. Print the code only to standard output, never through the `log` crate; without a signed-in account print no code and the sign-in guidance, and keep running. Report the code's end (used, expired, five wrong tries) and the linked name.
- [x] 8.2 Print the access mode, the linked owner or "not linked" and the number of consent records at start, and each later link or records change, from the existing owner and consent events.
- [x] 8.3 On Linux (not tested) replace the memory stores with the file stores, and replace the "unsupported" `protect` and `unprotect` with the design's encryption: XChaCha20-Poly1305 with a random nonce and the file's entropy as associated data; the key from HKDF-SHA256 of `${XDG_CONFIG_HOME:-~/.config}/voicebro/protect.key` (32 OS-random bytes, folder 0700, file 0600, created exclusively) salted with `/etc/machine-id`. Refuse a key file readable by others or a missing machine id with a message naming the file, and never write plain text. Add `chacha20poly1305` and `hkdf` (with `sha2` 0.10) as core dependencies on every target (so Windows compiles the format code) at the versions already in `Cargo.lock`. Keep DPAPI and the Windows files unchanged.
- [x] 8.4 Give the owner-link log line in `owner.rs` ("link again in the window") its terminal variant naming `--link-owner` (with 3.3).
- [x] 8.5 Add `voicebro-server --set-token`: read one line from standard input with echo off on a terminal, trim (a leading byte order mark too), refuse a blank value or one with characters a token never has, store it (Windows Credential Manager `VoiceBro/discord_token`; Linux (not tested) `discord-token.bin` through the `protect` layer) and exit without connecting. Token source order at start: `DISCORD_TOKEN_FILE`, then the stored token; with neither, exit with both ways named. Never print, log or pass on the token.
- [x] 8.6 Add `linux.encrypt_files` (default `true`, boolean, missing reads as `true`, no schema version change; not shown in the window, kept by window saves). On Linux (not tested) give every protected file a format tag (encrypted or plain); write plain files with mode 0600 only while the setting is `false`; at start rewrite each readable file whose form differs from the setting, and warn at every start while it is `false`. Windows ignores it.

- [x] 8.7 Add `voicebro-server --unlink-owner`: no runtime, no connection; load the link through `OwnerState` and remove it as Unlink does (an unreadable link file is deleted too), name the owner removed or say none was linked. The log line names the command instead of the window.
- [x] 8.8 Add `voicebro-server --list-consents` (each record's Discord user id, name and time) and `--remove-consent <Discord user id>` (through `ConsentState::withdraw` as Remove does, the store opened with the current settings' engine; an id without a record or not a Discord user id changes nothing and says so). Nothing is sent to the person; the log line names the command instead of the window.
- [x] 8.9 Add `voicebro-server --clear-token`: delete the stored token (Credential Manager on Windows, saying the desktop app loses it too; `discord-token.bin` on Linux, not tested), never the file `DISCORD_TOKEN_FILE` names; say what was removed or that nothing was stored.

## 9. Fix found in the review

- [x] 9.1 Restore the `Synth` trait import in the Windows voice test in `tts/windows.rs` so the core's tests compile again (`cargo clippy --all-targets` fails with E0599 at `synthesize`).

## 10. Docs and checks on Windows

- [x] 10.1 Update `README.md` (every mode in headless, `--install-assets`, `--link-owner` for an interactive terminal, each engine's terminal sign-in, the log, Linux marked as not tested with its prerequisites and CPU-only Whisper), `docs/ARCHITECTURE.md`, the user guide and `docs/PRIVACY-POLICY-TEMPLATE.md` (how the token, the owner link and the consent records are encrypted on Linux, where the key file lives and what losing it means, `--set-token`, and that `linux.encrypt_files: false` leaves them plain, protected only by file permissions) and the local `AGENTS.md` (the new commands, the setting and `discord-token.bin` in the data folder list).
- [x] 10.2 Run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` on Windows, and `cargo check --locked -p voicebro-desktop`; report what they returned.
- [x] 10.3 Run `voicebro-server` on Windows against the test guild in each assistant mode (realtime with Codex, Whisper with each engine, text answers), check `--install-assets` with a scratch data directory, link an owner with `--link-owner` and check the `owner` mode, run `--set-token` with the same token from `probe.local.json` (so the stored token does not change) and check that `linux.encrypt_files: false` changes nothing on Windows, and stop it with Ctrl+C during a session; report what was not tested.
  Ran on 2026-11-22 (debug build, scratch folders in `target\headless-test\`): `--install-assets` in the realtime wake-word and the Whisper + Claude Code + Piper setups (everything found installed), a settings file the check rejects (start refused, file unchanged), an empty and a missing token file, `--set-token` with a blank or odd value (nothing stored), a start with Whisper files missing and a wrong token (missing files listed with the command, then "Discord: 401: Unauthorized" and the exit), a real start in always mode (connected, 7 commands in the test guild, Ctrl+C sent to its console: disconnected, runtime stopped, exit 0, no Codex process left), `--link-owner` (code on standard output only, not in the log or stderr, Ctrl+C ends it) and Whisper + Claude Code (subscription notice printed). Still open, needing a person in Discord: a `/join` session in each mode, a `/link` with the printed code and the `owner` mode.
  2026-11-23: the user ran the console binary from the debug build of `a2a88d0` against the test guild and reported that the check passed.

- [x] 10.4 Bring the docs up to date for 8.7 to 8.9 (README's access part, the user guide, `docs/ARCHITECTURE.md`, the local `AGENTS.md`), run `cargo fmt --check`, `cargo clippy --all-targets` and `cargo test` again, and check the three commands on scratch data folders: nothing linked and no records (the empty paths), a non-id to `--remove-consent`, and `--clear-token` on Windows only together with an immediate `probe seed-secrets`, which writes the same token back.
  Ran on 2026-11-23: fmt clean, clippy without errors (only the older warnings of Rust 1.99's new lints), tests 533 + 17 + 2 pass; `--unlink-owner` with nothing linked and with an unreadable link file (deleted), `--list-consents` with nobody, `--remove-consent` with a non-id and an unknown id (nothing changed, exit 1), `--clear-token` twice, a start without a token (refused with both ways named), then `probe seed-secrets` and a start that connected. Not run: unlinking a real link and removing a real record, which need a `/link` and an `/optin` in Discord.

## 11. Linux checks (the user, later)

Open when the change was archived on 2026-11-23: the user asked to merge and archive the
changes and release 1.3.1. Linux stays marked as not tested in the specs and docs until
these two pass.

- [ ] 11.1 Build with `cargo check --locked -p voicebro-server` and `cargo build --locked --release -p voicebro-server` on glibc Linux x86-64.
- [ ] 11.2 Run `--install-assets` for each mode and start the bot in each assistant mode; link an owner with `--link-owner`, record a consent with `/optin`, restart and check both are kept, that both files are encrypted (no readable id or name), that the key file has mode 0600, and that an edited byte or a missing key makes them count as missing; store the token with `--set-token` and start without `DISCORD_TOKEN_FILE`, then `--clear-token`; `--unlink-owner`, `--list-consents` and `--remove-consent` on the linked owner and the recorded consent; switch `linux.encrypt_files` to `false` and back and check the files are rewritten each way, kept, and warned about while plain; stop the bot with Ctrl+C and SIGTERM and check that no engine process is left. Then remove "(not tested)" from the specs and docs for what passed.

Add no new tests. Change a test only to make it compile (9.1). Run the existing suite on Windows before the commit (10.2). No WAV fixtures or deployment files. Leave the implementation uncommitted for manual review; do not tag or push.
