# Proposal

## Why

The user judged the window poor and asked for the first version's interface
(`docs/NEXT-CHANGES.md`, section 3, with six screenshots of it): a tab bar with Console,
Codex chat, Status, Performance, System, Settings and Config, cards with two-column forms,
and a status bar. The first version's window is the reference; this change gives this app the same
layout, styles and themes, written new.

## What Changes

- The window gets the first version's structure: tabs **Console, Codex chat, Status,
  Performance, System, Settings, Config**; cards; a status bar at the bottom with the
  Discord connection state, the guild count, the voice channel count and the uptime.
- **Status**: the connected guilds on the left; a grid of fact tiles on the right (Discord,
  bot account, voice channel, assistant engine, assistant state, wake word, mute, Codex
  account, call, hotkey), and the actions Connect, Disconnect, Mute, Sign in, Check.
- **System**: application (version, build stamp, process id, uptime, runtime), operating
  system (platform, architecture, CPU cores).
- **Settings**: cards Appearance (theme Dark, Darcula, Light, IntelliJ; font size), Audio
  (from change `audio-gain-settings`), Assistant (engine, activation mode, phrases with the
  vocabulary note), Codex (model, reasoning effort, voice, instructions, speaker labels),
  Hotkey.
- **Config**: the Discord token (masked, with a short how-to), the guild id
  (with its help text), the Codex executable, an "Auto-start enabled" badge, Save changes
  and Reset, and the stored settings shown read-only with the file location.
- Console, Codex chat and Performance get their place and layout here; their content comes
  with changes `chat-and-console-tabs` and `performance-statistics`.
- Theme and font size are stored in the settings file (`ui.theme`, `ui.font_size`), not in
  the browser storage, so they stay with the portable app.
- The WebView2 data folder moves next to the executable; today it writes 130 MB into
  `%LOCALAPPDATA%\com.voicebro.desktop`, against the portability rule.
- Everything about the ChatGPT browser is left out; all strings are English.

## Capabilities

### New Capabilities

- `app-window`: the window's structure (tabs, status bar, System tab, Config tab),
  appearance, and where the window keeps its data.

### Modified Capabilities

- `status-view`: the Status tab's layout as guilds plus fact tiles.
- `settings-ui`: the fields are split between the Settings cards and the Config tab.
- `app-settings`: the `ui` section with theme and font size.

## Impact

- `apps/desktop/ui/index.html`, `styles.css`, `main.js` written new to the first version's
  look; `apps/desktop/src/main.rs` (a `system_status` command, the WebView2
  data directory, the window built in code if the configuration cannot set the folder);
  `crates/voicebro-core/src/settings` (`ui` section).
- The `frontend-design` skill from `anthropics/skills`, installed locally for the agents.
- Docs: `docs/ARCHITECTURE.md` 4.6 and 4.8, `AGENTS.md`.
