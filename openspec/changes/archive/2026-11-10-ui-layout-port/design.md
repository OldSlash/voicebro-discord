# Design

## Context

See proposal.md. The new window has a tab bar, cards with two-column forms, stat tiles,
info lists, a status bar and four themes. The app's UI today (`apps/desktop/ui`) has three
tabs and its own minimal CSS; its commands and the status snapshot already carry most of
what the new window shows. WebView2 runs with Tauri's default data folder `%LOCALAPPDATA%\com.voicebro.desktop`
(130 MB measured), which breaks the rule that the app writes nothing under the user profile.

## Goals / Non-Goals

**Goals:** the new look and structure, tab for tab, wired to the app's commands;
appearance settings; portable window data.

**Non-Goals:** the form's saving behaviour (change `settings-form-behavior`), the content of
Console, Codex chat and Performance (changes `chat-and-console-tabs`,
`performance-statistics`), memory readings (change `memory-measurement`); a framework or
bundler; an update check.

## Decisions

**D1. Written new.** `index.html`, `styles.css` and the window parts of `main.js` are
written for this app: the tabs, cards, status bar and themes the proposal lists, with no
update tile, every string in English. The status bar's memory slot is left for change
`memory-measurement`. *Alternative:* a different design guided by the skill. Rejected: the
user asked for this layout.

**D2. Own code only.** The markup, the styles and the window code are all written for this
app and its commands.

**D3. Appearance in the settings file.** `ui.theme` and `ui.font_size` live in a new `ui`
section, so they travel with the portable folder; the page applies them from the settings at
load (CSS variables, `html[data-theme]`) and saves them like any other setting.
`localStorage` is not used for them.

**D4. System facts.** A `system_status` command returns version, build stamp, process id,
uptime (from a start instant kept in the shell), platform, architecture and CPU count. The
status bar counts guilds and voice channels from the existing Discord status.

**D5. Config tab.** It edits the token (through the existing secret commands), the guild id
and the Codex executable; Save changes sends one settings patch plus the token when one was
typed; Reset reloads the stored values; the read-only view shows `settings.json` as stored,
with its path from the status. The "Auto-start enabled" badge reflects change
`first-version-parity`.

**D6. WebView2 data next to the executable.** The main window is created in `setup` with
`WebviewWindowBuilder` and `data_directory(<data dir>\webview-data)` instead of from
`tauri.conf.json`, unless the Tauri 2.12 configuration supports a folder relative to the
executable (check the `WindowConfig` source first). After a run, nothing new may appear
under `%LOCALAPPDATA%\com.voicebro.desktop`; the existing 130 MB folder is the user's to
delete.

**D7. Skill.** The `frontend-design` skill from `anthropics/skills` is installed for every
agent as `AI-REFERENCE.md` describes (`.agents/skills/` and `.claude/skills/`, real copies);
it guides spacing, contrast and states within the layout of D1, not a new design.

**D8. Checking without the window.** The page is opened from disk in a browser; without
`window.__TAURI__` it renders its layout with placeholders, which is enough to check each
tab's layout. Live data in the browser comes with the command
mock of change `settings-form-behavior`.

## Risks / Trade-offs

- [Building the window in code changes how Tauri reads the window list] → the configuration
  keeps no window; the capability file's window label stays `main`.
- [Two settings sources during the transition] → the `ui` section is new; nothing else moves
  in the schema.

## Migration Plan

The old WebView2 folder under `%LOCALAPPDATA%` is left in place and reported; the new folder
is created on the next start. Rollback is reverting the branch.
