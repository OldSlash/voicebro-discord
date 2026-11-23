# Design

## Context

See proposal.md for the reason and the user's decisions. The current state that shapes the
approach:

- The panel is the reply `/join` posts (`discord/handler.rs` `handle_join`): an embed from
  `discord/panel.rs` and one button with the custom id `voicebro:mute:<guild>`. The
  session keeps the panel's channel and message id (`VoiceSession::panel`); a leave deletes
  it; the refresher edits every panel at most once a second when `panels_dirty` is set.
- The Mute press (`handle_component`) checks only that the button's guild is the
  interaction's guild, defers an update, toggles the mute and edits the panel.
- The Discord adapter sees the rest of the app only through the `VoiceHost` trait, which
  `GateHost` implements. A settings change goes through `Runtime::save_settings` (patch,
  validate, store, `changed_sections`, `apply`, `SettingsChanged`), which lives on
  `Runtime`, not on `GateHost`. A mode change in `apply` ends every running session with
  the falling tone and attaches the assistant in the new mode (`switch_mode`); an engine
  change also prepares the new engine for each channel in the background
  (`switch_engine`). Both mark the panels dirty.
- `OwnerState::linked_id()` gives the linked owner's Discord id whatever the access mode,
  without a lock.
- The window reloads the settings on `settings_changed` (`loadSettings` → `fillForm`;
  `fillConfig(false)` leaves the unsaved "Discord and Codex" fields alone).
- The window opens its install dialog when a log line contains "The Whisper mode needs
  downloads" or "The wake word needs downloads", the openings of the join refusals.
- Settings validation refuses `assistant.mode = realtime` with an engine other than Codex.

## Goals / Non-Goals

**Goals:**

- One code path for a mode change, whether it comes from the window or from the panel.
- The owner check happens before any effect and in one place for every button.
- A press never blocks Discord's 3-second answer limit.

**Non-Goals:**

- No change to the window's controls; the window only shows the new mode.
- No new Discord permission, intent or registered command.
- No change to how the engine switch prepares the engine (`switch_engine` as today).

## Decisions

### D1. Buttons on the existing panel, three mode buttons

The panel keeps its embed. `panel::components` returns two action rows: Mute assistant /
Unmute assistant (as today) and Leave channel (danger style); then Realtime voice, Whisper
mode and Text answers. The current mode's button has the primary style and is disabled;
the others are secondary. Custom ids carry the guild: `voicebro:mute:<guild>` (unchanged),
`voicebro:leave:<guild>` and `voicebro:mode:<realtime|whisper|text>:<guild>`, parsed into
one `PanelButton { guild, action: Mute | Leave | Mode(AssistantMode) }`.

Alternative: a select menu for the mode. Rejected: it takes two clicks and shows the
current mode less plainly, and the user asked for buttons.

### D2. The owner check first, for every button

`VoiceHost` gets `owner_id() -> Option<u64>` (`GateHost`: `OwnerState::linked_id()`). The
component handler compares it with the presser's id before anything else. A refusal is an
ephemeral message response, not a message update, so the panel stays as it is. Two
refusal texts, both pure functions with tests: "These buttons are for this bot's owner
only." and, with no owner linked, "No owner is linked to this bot yet, so its buttons work
for nobody. The operator links their Discord account in VoiceBro → Config → Access." A
refused press is logged in one line without the presser's name.

Alternatives: show the buttons only to the owner, or disable them for others. Not possible:
a message's components are the same for every viewer. Discord's own roles (for example
Manage Server). Rejected: the user named the owner, and the owner link is the only
identity the app sets on purpose (by a code only).

### D3. An old panel loses its buttons

After the owner check the handler compares the pressed message with the session's
`panel.message` for that guild. Any other message (no session, or a session with another
panel: a leftover after an app restart, or a panel from an older build) gets a message
update that keeps the embed, removes every button and says "This panel is from an earlier
session. /join posts a new one." Nothing else happens.

### D4. Leave channel reuses `/leave`

The handler defers an ephemeral reply, reads the channel name, calls
`session::leave_voice` (which deletes the panel, as for `/leave`) and edits the reply to
"Left <channel>.". The ephemeral reply does not depend on the deleted panel message.

### D5. A mode change goes through `Runtime::save_settings` over a request channel

`VoiceHost` gets `change_mode(mode) -> ModeChangeFuture` (a boxed future of
`Result<ModeChange, String>`; `ModeChange` says whether the engine was switched to
Codex). `GateHost` implements it by sending a request with a oneshot reply into an mpsc
channel. `Runtime::start` takes `self: &Arc<Self>` and spawns one task holding a
`Weak<Runtime>` that serves the requests in order with `Runtime::change_mode(mode)`:

1. Build the patch `{"assistant": {"mode": "<mode>"}}`, adding `"engine": "codex"` when the
   mode is realtime and the engine is not Codex.
2. Check the files the new mode needs (D6); a gap returns the refusal and saves nothing.
3. Call `save_settings(&patch)`: the same validation, storage, `apply` and
   `SettingsChanged` as the window's save. Its error is the refusal.

The handler defers a message update first, so the save may take its time. On success it
edits the panel with the new state; with the engine switched it also sends an ephemeral
follow-up: "Switched to the realtime voice. The engine is now Codex: the voice call works
only with Codex." A refusal is an ephemeral follow-up with the reason; the panel is not
edited. Every other panel is refreshed by `switch_mode` / `switch_engine`, which mark the
panels dirty as they do for the window.

`save_settings` gets a save lock around read, patch, store and apply, so a press and a
window save at the same moment cannot drop one another's change.

Alternatives considered:

- `GateHost` holding a `Weak<Runtime>` set after construction. It needs the `Arc` as
  well, and it gives the host a back-reference to the runtime that nothing else needs; the
  channel keeps the dependency one-way.
- Moving the save and `apply` into a component shared by `Runtime` and `GateHost`.
  Rejected for this change: a large refactor for one caller.
- Serving the requests in each shell (desktop, console). Rejected: the same code twice.
- A separate `Runtime::serve_panel()` the shells call after `start`. Rejected: a shell
  that forgets it leaves the buttons dead without an error; with `start(self: &Arc<Self>)`
  the compiler makes every shell hold the runtime in an `Arc`. The desktop already does;
  the console binary of `cross-platform-headless` needs `Arc::new`, done by whichever of
  the two changes reaches `main` second.

### D6. Files are checked on disk before the save

The check reads the disk only, with the same requirement lists the join refusals use: the
Whisper manager's missing files for the selected model, device and voice activity model,
the voice manager's for the selected voice (the Whisper mode only), and
`WakeManager::requirements` for the wake model and the Vosk library (realtime with the
wake word only; always listening needs no Vosk files). A gap returns a refusal of its own
wording, since the join refusals end with "then run /join again" and their openings make
the window open its install dialog:

- "Not switched: the Whisper mode needs files that are not downloaded yet (<items>). The
  operator downloads them in VoiceBro → Settings, in the Whisper and Local voice cards."
- the same for Text answers with the Whisper card alone;
- "Not switched: the wake word needs files that are not downloaded yet (<items>). The
  operator downloads them in VoiceBro → Config, under Wake model."

The log line for a refused mode change carries neither join-refusal opening, so the window
opens no install dialog for a press in Discord: the operator may not be at the PC, and the
dialog's text speaks of a refused join. When `cross-platform-headless` is on `main`, the
console binary's terminal guidance picks a text naming `voicebro-server --install-assets`
instead of the window.

Alternative: switch anyway and let the files wait, as the window allows. Rejected: from
Discord nobody may be at the PC to agree to a download, and the channels would keep an
assistant that cannot hear.

### D7. The panel reads the mode as a value

The panel needs the current mode to mark its button, so `VoiceHost` reports the mode as an
`AssistantMode` read from the settings, next to the label `assistant_mode()` gives today
for the Mode field and `/status`.

## Risks / Trade-offs

- [BREAKING: Mute from the panel needs a linked owner; servers without one lose it] → The
  refusal says how to link; the window's button and the hotkey still mute; the user guide
  says so.
- [Everyone sees buttons only the owner can use] → The refusal is ephemeral and short;
  nothing changes for the others.
- [Realtime voice switches the engine to Codex, and Codex may then fail to start for a
  channel] → As for the same switch in the window, the channel stays without a session and
  the log and the Status tab give the reason; the panel's reply does not wait for the
  engine's preparation.
- [One press changes the mode in every server] → The mode is app-wide by design; the specs
  say so, and every panel shows it within a second.
- [A press during a window save] → The save lock of D5.
- [Linux console binary] → It accepts only the `everyone` access mode and has no owner link
  flow, so the buttons would work for nobody there. See Open Questions.

## Migration Plan

Nothing to migrate: no setting, no stored file and no command changes. Panels posted by
earlier builds lose their buttons at the owner's first press (D3). Rollback is a revert of
the change.

## Open Questions

- The Linux console binary of `cross-platform-headless` (not tested) has no owner, so every
  press would be refused there. Whether Linux gets another way to name the person who may
  press is decided when that change and this one are both on `main`; it changes neither
  this change's specs nor its tasks.
