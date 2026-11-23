# Proposal

## Why

On 2026-11-22 the user asked for control buttons on the bot in Discord itself, so that the
bot can be run from the Discord UI without the VoiceBro window: kick it out of the voice
channel, make it stop talking, and switch it between the voice and text modes. Today the
panel the bot posts with `/join` has one button, Mute assistant, which any member can
press, and the assistant mode can be changed only in the window.

The user decided on the same day: the buttons are pressed and the mode is changed by the
owner only; there is no Connect (join) button; the controls are buttons on the panel that
appears with the bot, not slash commands.

## What Changes

- **The panel gets buttons, not commands.** The panel stays the message `/join` posts and
  `/leave` deletes. Its first row holds Mute assistant / Unmute assistant (as today) and a
  new Leave channel; its second row holds the three assistant modes: Realtime voice,
  Whisper mode and Text answers. The button of the current mode is marked and cannot be
  pressed. No slash command is added or changed.
- **BREAKING: only the linked owner can press the panel's buttons**, Mute included, in
  every access mode (`everyone`, `agreed`, `owner`). Anyone else who presses one gets a
  reply only they can see, and nothing changes. While no owner is linked the buttons work
  for nobody, and the reply says how the operator links one. Today any member can press
  Mute. `/join`, `/leave` and `/status` stay open to everyone, as today.
- **Leave channel** does what `/leave` does: the bot leaves the voice channel, the panel
  is deleted, and only the owner sees the reply naming the channel.
- **A mode button switches the mode** through the same save the window uses: the value is
  checked, written to `settings.json` and applied at once. The mode is app-wide, so it
  changes in every server the bot is in: each running session ends with the falling tone,
  the bot stays in its channels, the assistant is attached in the new mode, every panel
  shows the new mode within a second, and the window's Config → Assistant shows it without
  a restart.
- **Realtime voice with Claude Code or OpenCode switches the engine to Codex** in the same
  save, since the voice call works only with Codex; the reply says so. The Claude Code and
  OpenCode settings stay stored. This mirrors what change `realtime-switches-to-codex`
  plans for the window.
- **A mode whose files are not on disk is refused** before anything is saved: the Whisper
  files and the voice for the Whisper mode, the Whisper files for Text answers, the Vosk
  library and model for Realtime voice with the wake word. The reply names what is missing
  and where the operator downloads it. Nothing is downloaded from Discord, since every
  download needs the operator's OK in the window.
- **A press on an old panel changes nothing.** A panel left over from an earlier session
  (after a leave, a rejoin or an app restart) loses its buttons at the owner's press, with
  a note that `/join` posts a new panel.

Assumption, recorded here: "text mode" in the request means the Text answers mode. A
separate switch for posting answers to a chat (`assistant.post_answers`) is not part of
this change.

Not in this change: a Join or Connect button, new slash commands, buttons for the engine,
for answer posting or for stopping the current answer, a mode per server, and limiting
`/leave` to the owner.

## Capabilities

### New Capabilities

- `discord-panel-controls`: the panel's buttons and who may press them: the owner check,
  Leave channel, the mode buttons with their save, the engine switch for Realtime voice,
  the refusal for missing files, and presses on an old panel.

### Modified Capabilities

- `voice-channel-session`: requirement "Member panel": the panel carries the buttons of
  `discord-panel-controls`, and Mute is pressed by the owner.
- `owner-only-access`: requirement "The commands stay open to everyone": the panel's Mute
  button leaves the list of things open to everyone; the panel's buttons are the owner's
  in every access mode.
- `speaker-consent`: requirement "The commands stay open to everyone": the panel's Mute
  button leaves the list in the same way.

## Impact

- Core: `discord/panel.rs` (the two rows, the custom ids, the old-panel note),
  `discord/handler.rs` (the owner check, Leave, the mode presses, the replies),
  `discord/session.rs` (the panel's state with the mode), `discord/mod.rs` (`VoiceHost`
  gets the linked owner's id, the mode as a value and the mode change), `runtime.rs`
  (`GateHost` implements them; the mode change goes through `Runtime::save_settings`; the
  missing-files check before the save).
- Window: no new control. It already reloads the settings on `settings_changed`; the
  command mock and a headless UI check get a scenario for a mode changed from outside.
- Docs: `docs/user-guide.md` (the panel, who presses it, the commands table),
  `docs/ARCHITECTURE.md` (the panel, `VoiceHost`, the mode-change flow), `AGENTS.md`
  (local).
- Discord: no new permission, intent or command; button presses arrive as interactions,
  which the bot already receives.
- Other changes in flight: `cross-platform-headless` (the buttons live in the core and so
  work in the console binary too; see the design), `realtime-switches-to-codex` (the same
  engine rule for the window). No new dependency, no settings schema change, no version
  change.
