# Tasks

## 1. Images

- [x] 1.1 Create the branch `change/readme-and-user-guide` from `main`; verify `git log -1` shows the `main` tip as parent (branched at `d5c97bd`)
- [x] 1.2 Set the mock's build stamp and version to 1.0.0 in `apps/desktop/ui-dev/mock.js`; take the screenshots from the mock page in a headless Edge whose profile lives under `target\` (the Codex chat tab with one entry of every kind for the README, the Assistant settings card with the English model and its phrases for the guide) into `docs/images/`, and write the icon as `docs/images/icon.png` from `apps/desktop/icons/icon.ico`; verify each image opens, shows no real server, token or conversation, and is under 300 KB (the mock's test entries, one of every kind, read badly in a README (an error, a call start after the answer), so the mock gained `?sample`: a sample session of two players and a release-like stamp "1.0.0 2026-11-11"; `docs/images/chat.png` 49 KB and `docs/images/wake-settings.png` 60 KB (the English model, "hey bro" / "brother", "goodbye" / "good bye", the download notice); no real name, server or token in either. The icon was left out: `icon.ico` holds at most 64 px and is a placeholder, a cyan circle with an exclamation mark, which reads as a warning sign in a header)

## 2. README

- [x] 2.1 Rewrite `README.md` as the landing page of the proposal: header, screenshot, Features, Requirements, Download, Quick start (at most five steps), Building from source (prerequisites with versions, clone, build, test, run, the trial installer, the repository map), License (MIT with the copyright line of `LICENSE`, the third-party notices, Vosk, Codex CLI, no affiliation); verify every relative link and image path exists, every command and label matches the code and the window, and no internal check note is left (141 lines; nine relative links resolve; the labels are the window's ("Config → Discord bot token", "Save changes", "Install Codex", "Sign in to Codex", "Download"); the hotkey claim was narrowed to "system-wide", since play in a full-screen game was not tested here; no check note left. External links: the tool and project pages answer 200; the repository's own links (releases, issues, clone URL) answer 404 to an anonymous request, so the repository is not public yet)

## 3. User guide

- [x] 3.1 Write `docs/user-guide.md` from the old README's content in plain language, with a table of contents, the steps of setup, wake phrases and other languages, everyday use, a settings table, privacy and files, updating, uninstalling and a troubleshooting table; verify every anchor of its table of contents and of the README's links into it resolves, and every label and path matches the code and the window (twelve sections, a commands table, a settings table, a files table, a troubleshooting table, a "what leaves your PC" list; all fifteen anchors resolve; Codex's run time corrected to include the minute after Sign in or Check. The bot's permissions were first copied from the old README as View Channels, Connect and Speak; checked against the code and the live bot's install settings (`GET /applications/@me`: `bot`, `applications.commands`, Video, View Channels, Send Messages, Embed Links, Connect, Speak; no privileged intents), the guide now lists five with what each is for, the intents to leave off and the channel's own permission settings, and the README's quick start names the same five. Video is not used by the code and is left out. A server whose @everyone role grants none of these was not tested: on the test server @everyone already grants them all)

## 4. The zip and the docs

- [x] 4.1 Add `docs\user-guide.md` and the images both documents use to the zip in `scripts/build-release.ps1`, and update `docs/ARCHITECTURE.md` (the layout tree and 4.10) and `LOCAL-TESTING.md` (the zip's contents); verify a trial build's zip holds the guide and the images at the paths the links use (the trial zip holds `docs\user-guide.md`, `docs\images\chat.png`, `docs\images\wake-settings.png` next to `README.md`; `LOCAL-TESTING.md` also says how the screenshots are taken)

## 5. Checks

- [x] 5.1 Render `README.md` and `docs/user-guide.md` in the browser pane; verify the header, the images and the tables display, and `openspec validate readme-and-user-guide --strict` passes (rendered with `marked` from a local server inside the project, not by GitHub, so the `[!NOTE]` boxes show as plain quotes: the README's centered header, its screenshot loaded, six sections, three tables, three code blocks; the guide's thirteen sections, four tables and its screenshot loaded)

## Workflow follow-up

- Commit and merge into `main` when the user says so; archive the change in the same step.
