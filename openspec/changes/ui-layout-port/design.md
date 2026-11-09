# Design

## Context

See proposal.md. The first version's window (its screenshots in `docs/NEXT-CHANGES.md`)
has a tab bar, cards with two-column forms, stat tiles, info lists, a status bar and four
themes; theme and font size are kept in the browser storage. This app's UI (`apps/desktop/ui`) has three tabs and its own minimal CSS; its
commands and the status snapshot already carry most of what the first version shows.
WebView2 runs with Tauri's default data folder `%LOCALAPPDATA%\com.voicebro.desktop`
(130 MB measured), which breaks the rule that the app writes nothing under the user profile.

## Goals / Non-Goals

**Goals:** the first version's look and structure, tab for tab, wired to this app's
commands; appearance settings; portable window data.

**Non-Goals:** the form's saving behaviour (change `settings-form-behavior`), the content of
Console, Codex chat and Performance (changes `chat-and-console-tabs`,
`performance-statistics`), memory readings (change `memory-measurement`); a framework or
bundler; the first version's browser parts and update check.

## Decisions

**D1. Written new to the first version's look.** `index.html`, `styles.css` and the window
parts of `main.js` are written for this app from the first version's screenshots: the same
tabs, cards, status bar and themes, without the browser cards, the ChatGPT URL, the browser
executable, the audio mode and the update tile, every string in English. The status bar's
memory slot is left for change `memory-measurement`. *Alternative:* a different design
guided by the skill. Rejected: the user asked for the first version's interface.

**D2. Own code only.** No file of the first version's window is copied: the markup, the
styles and the window code are written from the screenshots and this app's commands.

**D3. Appearance in the settings file.** `ui.theme` and `ui.font_size` live in a new `ui`
section, so they travel with the portable folder; the page applies them from the settings at
load (CSS variables, `html[data-theme]`) and saves them like any other setting. The first
version's `localStorage` keys are not used.

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
it guides spacing, contrast and states within the first version's look, not a new one.

**D8. Checking without the window.** The page is opened from disk in a browser; without
`window.__TAURI__` it renders its layout with placeholders, which is enough to compare each
tab with the first version's screenshots. Live data in the browser comes with the command
mock of change `settings-form-behavior`.

## Risks / Trade-offs

- [The first version's window relies on its own commands] → the window code is written for
  this app's status snapshot.
- [Building the window in code changes how Tauri reads the window list] → the configuration
  keeps no window; the capability file's window label stays `main`.
- [Two settings sources during the transition] → the `ui` section is new; nothing else moves
  in the schema.

## Migration Plan

The old WebView2 folder under `%LOCALAPPDATA%` is left in place and reported; the new folder
is created on the next start. Rollback is reverting the branch.
