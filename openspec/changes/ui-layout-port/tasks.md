# Tasks

## 1. Preparation

- [ ] 1.1 Create the branch `change/ui-layout-port` from the tip of `change/first-version-parity`; verify `git log -1` shows the previous tip as parent
- [ ] 1.2 Install the `frontend-design` skill from `anthropics/skills` into `.agents/skills/frontend-design/` and `.claude/skills/frontend-design/` as real copies, following `AI-REFERENCE.md`; verify both `SKILL.md` files exist and are identical

## 2. Settings and shell support (specs `app-settings`, `app-window`)

- [ ] 2.1 Add the `ui` section (`theme`, `font_size`) with defaults and validation; verify unit tests: valid values pass, `pink` and 25 are rejected naming the field, an older file loads with the defaults
- [ ] 2.2 Add the `system_status` command (version, build stamp, process id, uptime, platform, architecture, CPU count); verify a unit test of the uptime and the values in the running app's log at start
- [ ] 2.3 Move the WebView2 data folder next to the executable (`webview-data`) per design D6; verify a run with a scratch data dir creates `webview-data` there and adds nothing under `%LOCALAPPDATA%\com.voicebro.desktop` (compare the folder's newest file time before and after)

## 3. The window (specs `app-window`, `status-view`, `settings-ui`)

- [ ] 3.1 Write `index.html` and `styles.css` to the first version's look: the seven tabs, cards, the status bar, the four themes, without the browser parts and the update tile; English strings only; verify the page opened from disk in a browser shows all seven tabs and the status bar, and a search finds no Cyrillic and no "browser" text
- [ ] 3.2 Write `main.js` for this app: tab switching that keeps state, the status bar (connection colour, guilds, voice channels, uptime), the Status tab (guild list and fact tiles with Connect, Disconnect, Mute, Sign in, Check), the System tab; verify against the status snapshot in the running app and side by side with the first version's Status and System screenshots
- [ ] 3.3 The Settings tab cards (Appearance, Audio, Assistant, Codex, Hotkey) and the Config tab (token with help, guild id with help, Codex executable, auto-start badge, Save changes, Reset, stored settings with path, no token in it); appearance applied at once and stored; verify in the running app with a scratch data dir that each field shows its stored value, a theme change survives a restart, and the stored view holds no token
- [ ] 3.4 Placeholders for Console, Codex chat and Performance in the first version's layout (toolbar and empty panel); verify the three tabs render their toolbars

## 4. Docs and checks

- [ ] 4.1 Update `docs/ARCHITECTURE.md` 4.6 (`ui` section, `webview-data`) and 4.8 (tabs, status bar, commands) and `AGENTS.md` (the window's data folder, the skill); verify no text still lists the old three tabs
- [ ] 4.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`; verify all pass and report their output

## Workflow follow-up

- Checkpoint commits per task group on `change/ui-layout-port`, with the user's OK.
- The user compares the window with the first version and says what differs.
- The old `%LOCALAPPDATA%\com.voicebro.desktop` folder (130 MB) is the user's to delete.
- Merged into `main` with `--no-ff --log`, in chain order, once the app works end to end and
  the user says so; the change is archived in the same step.
