# Design

## Context

See proposal.md for the request. Today (main after `claude-web-search`):

- Config → Assistant shows, per mode, the Vosk wake model with its notice (`wake-download`)
  or the Whisper model with its notice (`whisper-download`). Config → Local voice holds the
  voice with its notice (`tts-download`). Settings → Whisper holds a third notice
  (`settings-whisper-download`): every missing Whisper file in the realtime mode, the CUDA
  runtime only in the Whisper mode.
- `main.js` opens the install dialog (`wake-install`, subjects `wake` and `whisper`) from
  several triggers: a choice that misses files (`chooseWakeModel`, `chooseWhisper`), a
  switch of the mode or the activation (`mode`), a rise in the number of voice channels
  while the wake word needs files (`join`), and a `/join` refused in the Whisper mode, seen
  through the log event that starts with "The Whisper mode needs downloads".
- The core refuses a `/join` in the Whisper mode when Whisper's or the voice's files are
  missing (`Runtime::prepare_whisper`, `JOIN_NEEDS_DOWNLOADS`). In the realtime mode it
  never refuses for Vosk: the bot joins and the wake state stays `needs_download`.
- The status carries `wake.state` (with its items), `whisper.missing` and `tts.missing` in
  both modes, so every notice can be drawn from the status alone.

## Goals / Non-Goals

**Goals:**
- One place per downloadable thing: the Vosk model on Config → Assistant (unchanged), the
  Whisper model and its files on Settings → Whisper, the voice on Settings → Local voice.
- A choice never opens a dialog; only a refused `/join` does.

**Non-Goals:**
- No setting is renamed or added; `ui.collapsed_cards` keeps the ids `whisper` and
  `local-voice`.
- No change to how downloads run, are checked or unpacked, or to the download by hand.
- No automatic join after a download: the member runs `/join` again.

## Decisions

### D1. The cards move, their ids and fields stay

The Local voice card moves as it is from the Config tab to the Settings tab, after the
Whisper card. The Whisper model row moves from Config → Assistant into the Whisper card,
before the device. The Settings tab's order is Appearance, Diagnostics, Whisper, Local
voice. The `data-mode="whisper"` model row and its notice leave the Assistant card; the
Assistant card keeps the mode, Recognition, Engine, the Vosk model rows of the realtime
mode, the phrase rows of each mode, Activation, Auto-stop and One speaker at a time. The
Settings tab's autosave line and the Config tab's autosave line name the cards they now
hold.

Alternative: keep the voice on Config and move only the model. Rejected: the user asked
for the voice's settings in Settings.

### D2. One notice in the Whisper card

The Whisper card gets one notice, drawn from `status.whisper.missing` in both modes, with
Download calling `whisper_download({ model })`. It replaces `whisper-download` and
`settings-whisper-download`; the `isRuntime` filter and the "runtime only in the Whisper
mode" branch go. The voice's notice (`tts-download`) moves with its card and keeps its
logic.

Alternative: two notices in the same card (model, CUDA runtime). Rejected: once the model
sits in the card, both would list files of the same Download.

### D3. A choice saves at once

`chooseWakeModel` and `chooseWhisper` call `saveControl` directly in both modes; the
`choice` reason of the dialog, `install.previous`, `install.saved` and
`whisperInstall.choice` go. The managers already report what a new choice misses (the wake
state's items, `missing`) and accept a download for the stored choice, so nothing changes
in the core for this.

### D4. The dialog opens for a refused join only

The window keeps one trigger per subject: the log event of a refused `/join`, recognized by
the start of the reply text (`The Whisper mode needs downloads` and a new
`The wake word needs downloads`). On it the window asks the status what is missing and
opens the dialog with that subject. The other triggers go: the mode and activation switch
(`mode`), the voice channel count (`join` from `renderInstall`), the bot already in a channel
at the first status, the engine choice, and the subscription dialog's `whisperNext`.

Alternative: a new event kind for refused joins. Rejected: the log-event route works for the
Whisper mode today, and the two reply texts are constants shared by the core and the mock.

### D5. The core refuses a wake-word join without the Vosk files

`Runtime::prepare_join` gains a check before the assistant's preparation, so a refusal for
missing files starts nothing of the assistant: in the realtime mode with
`session.mode = wake_word`, it asks the wake manager what the selected model misses (the
same list `wake_requirements` returns: the Vosk library and the model). A non-empty list
refuses the join with `wake::join_refusal`: "The wake word needs downloads (<labels>): open
VoiceBro → Config and press Download under Wake model, then run /join again"; its start is
`JOIN_NEEDS_WAKE_FILES`. The check reads the disk only, starts no download and is done
before the bot enters the channel, as for the Whisper mode. While a download of those files is running they are
still missing, so the join is refused too.

`JOIN_NEEDS_DOWNLOADS` becomes "The Whisper mode needs downloads: open VoiceBro → Settings
and press Download in the Whisper and Local voice cards, then run /join again".

### D6. The Whisper mode's note on Config → Assistant

In the Whisper mode, the Assistant card shows a note under the mode while
`status.whisper.missing` or `status.tts.missing` is not empty. It names what is missing
("the Whisper model base (q8_0)", "the voice Supertonic 3 F1"; the runtime and the VAD
model count as part of the Whisper model) and holds "Open Settings". The button shows the
Settings tab, removes the first card with missing files (Whisper, else Local voice) from
`ui.collapsed_cards` if it is there, and scrolls its notice into view. The note follows the
status, never the form, like the notices.

### D7. The Claude subscription dialog no longer chains a download dialog

With D4 the subscription dialog is the only dialog a Claude Code choice can open; its
"Both dialogs" scenario is changed to say no download dialog follows. Change
`claude-notice-hide` reworks that dialog's notice afterwards.

## Risks / Trade-offs

- [A member gets a refused `/join` where the bot used to join without the wake word] → The
  reply says what is missing and what to press; it is what the user asked for.
- [The window misses the log event when it was not open at the refusal] → The reply in
  Discord carries the same instructions, and the notices on the cards list what is missing.
- [Someone picks a model by mistake and it is saved at once] → Nothing is downloaded until
  Download; picking the previous model again removes the notice.

## Migration Plan

Nothing to migrate: no setting changes. Rollback is the previous build.
