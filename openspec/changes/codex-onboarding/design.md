# Design

## Context

See proposal.md for the motivation. What the code does today:

- `find_codex(configured)` in `assistant/codex/app_server.rs`: a configured path (used only
  if it is a file, no fallback), else `codex.exe` in a PATH directory, else the native exe
  in npm's global folder under `%APPDATA%`, else `codex.cmd` on the PATH. It reads the
  process's own `PATH`, which a running app does not refresh.
- When nothing is found, `PrepareError::NotFound` reads "Codex is not installed: install
  Codex CLI (npm install -g @openai/codex) or set its path in the VoiceBro settings", the
  status summary "codex not found: install Codex CLI (npm install -g @openai/codex) or set
  its path in Settings", and `/join` replies with the error.
- The Status tab has "Sign in to Codex" (`codex_sign_in`: the app-server's ChatGPT login,
  opened with `rundll32 url.dll`) and "Check Codex" (`codex_check`), and shows the
  assistant's `summary`.
- Checked on 2026-11-11: winget's package `OpenAI.Codex` (publisher OpenAI, Inc.,
  Apache-2.0) is a portable zip, `codex-x86_64-pc-windows-msvc.exe.zip` from
  github.com/openai/codex/releases, version 0.167.0 in winget. The GitHub release 0.167.1
  has that zip at 206 MB and the exe at 369 MB. winget puts portable packages under
  `%LOCALAPPDATA%\Microsoft\WinGet\Packages\` and a command link in
  `%LOCALAPPDATA%\Microsoft\WinGet\Links\`, which it adds to the user's PATH for new
  processes only. `winget.exe` is an App Execution Alias in
  `%LOCALAPPDATA%\Microsoft\WindowsApps\`.
- Change `release-logging` caches the Codex version by the executable's path, size and
  modification time, so a newly found executable is read once.

## Goals / Non-Goals

**Goals:**

- A user without Codex gets from "not installed" to "signed in" from the Status tab, with
  every step visible and nothing installed without a click.
- The app finds a Codex installed while it runs.

**Non-Goals:**

- Bundling or downloading Codex into the app's own folder (the user chose the common
  Codex for now).
- Updating Codex from the app, or pinning a Codex version.
- Installing Node.js or winget.

## Decisions

### D1. The finder gains winget's links folder, as a pure function

`find_codex` becomes a thin wrapper over `find_codex_in(configured, path_dirs,
local_app_data, app_data)`, which tries in order: the configured path; `codex.exe` in the
PATH directories; `<local_app_data>\Microsoft\WinGet\Links\codex.exe`; npm's native exe;
`codex.cmd` on the PATH. The pure function is what the tests exercise; the wrapper reads
the environment. The status already calls the finder on every read, so a new install
shows up within one status period (5 s).

### D2. The install runs in a console the user sees

A desktop command `codex_install` starts:

```text
cmd.exe /V:ON /C "winget install --id OpenAI.Codex --exact --source winget
                  & set rc=!errorlevel! & echo. & pause & exit !rc!"
```

with `CREATE_NEW_CONSOLE`, so the window shows winget's progress and any prompt winget
asks; the `pause` keeps the output readable until the user presses a key; the exit code is
winget's. The command returns at once; a task waits for the process and publishes the
result as an `AppEvent::Log` line and an install state the window reads
(`idle | running | finished { code }`), kept in the desktop shell. A second press while
running is refused.

No `--accept-package-agreements` or `--accept-source-agreements`: if winget asks, the user
answers in the console.

Alternative considered: download the release zip from GitHub into the app's folder with a
pinned hash, like the Vosk assets. Rejected for now by the user's decision to use the
common Codex; it remains the way to a pinned, tested Codex version later.

### D3. winget presence without starting a process

`codex_install_info` reports whether winget is available by checking
`%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe` with `symlink_metadata` (an App
Execution Alias is a reparse point with no size), then `winget.exe` on the PATH. It also
returns the package id, the source and the sizes shown in the notice, written as "about
200 MB to download, about 370 MB on disk" (measured on 0.167.1; they are labels, not
checks).

### D4. Texts

- `PrepareError::NotFound`, the `/join` reply: "Codex CLI is not installed: press Install
  Codex on the VoiceBro Status tab (or set the path of codex.exe in Config), then run
  /join again".
- The status summary and the start error (`NOT_FOUND_STATUS`): "codex not found: press
  Install Codex on the Status tab, or set the path of codex.exe in Config". It keeps the
  "codex not found" start that the main spec's scenario "Codex not installed" names.
- The Status tab's Codex area, when Codex is found and not signed in: "Codex <version> is
  installed. Next: Sign in to Codex (opens the ChatGPT sign-in in your browser)."

### D5. The notice on the Status tab

A block in the Codex area, rendered from the status (`assistant.executable` empty) and the
install info and state:

- Codex missing, winget present: the text from the proposal and the Install Codex button.
- Running: "Installing Codex in the console window…", the button disabled.
- Finished with a non-zero code: "winget ended with code <n>; the console window shows
  why." and the button again.
- Codex missing, no winget: the two other ways, no button.

`apps/desktop/ui-dev/mock.js` simulates the four states and the sign-in step.

## Risks / Trade-offs

- [winget installs the newest Codex, which may change the app-server protocol checked
  against 0.167.0] → a handshake failure already shows in the status with its text; the
  README (change `release-packaging`) names the tested version and the `codex-schema`
  probe stays the developer's check.
- [winget is missing or broken on some Windows 10 machines] → the no-winget notice with
  the npm and manual ways.
- [The console window is closed without pressing a key] → the process ends with the
  window and its exit code is still read.
- [Codex lands outside the app folder (`%LOCALAPPDATA%\Microsoft\WinGet\Packages`, sign-in
  in `~\.codex`)] → the user's decision for now; the README says where.

## Migration Plan

Nothing to migrate. A user with Codex already installed sees no change except the new
texts when Codex is missing.
