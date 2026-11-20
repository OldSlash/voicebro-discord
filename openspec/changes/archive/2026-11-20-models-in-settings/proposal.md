# Proposal

## Why

On 2026-11-20 the user said the Settings tab is not where it should be. The Local voice
card and the Whisper model, with their downloads, sit on the Config tab, although they
belong to the PC, not to the bot. The download dialog also opens when a mode is switched or
a model is picked, which annoys the user: the window offers a download before it is clear
what for, while the same choice could just be made and downloaded under the field. The
user asked for:

- the Local voice card and the Whisper model with its download in Settings;
- no dialog when something is picked, only the notice under the field with its Download;
- the dialog only at `/join`, and the join refused while something it needs is missing;
- on Config → Assistant, in the Whisper mode, a note when the Whisper model or the voice is
  not downloaded, with a button to Settings.

## What Changes

- **The Local voice card moves from Config to Settings**, with the same fields: the voice,
  the speed, Preview and the voice's download notice. **BREAKING** for anyone used to the
  old place; no setting changes its name or meaning.
- **The Whisper model moves from Config → Assistant to Settings → Whisper**, with its
  download notice and Download. The Whisper card then has one notice for every missing file
  of the selected model and device (runtime, model, VAD model), in both assistant modes; it
  replaces the card's two notices of today (the files ahead of the switch, the CUDA
  runtime).
- **No dialog when something is picked or switched.** A Vosk model, Whisper model, device
  or voice that misses files is saved at once, and the notice under its field lists the
  files with Download. The dialog no longer opens when the assistant mode is switched to
  Whisper, when the activation is switched to wake word, or when an engine is picked that
  moves the mode to Whisper.
- **The dialog opens only when a `/join` was refused for missing files**, listing what is
  missing, with Download as today. After the download the member runs `/join` again.
- **A `/join` in the realtime mode with the wake word is refused while the Vosk library or
  the selected Vosk model is missing**, as a `/join` in the Whisper mode is refused today
  while Whisper's or the voice's files are missing. Today the bot joins and does not hear
  the activation phrase. In always-listening mode Vosk is not used and the join is not
  refused for it.
- **Config → Assistant, in the Whisper mode**: while the selected Whisper model or the voice
  misses files, a note says what is not downloaded and to download it first, with a button
  that opens the card in Settings. The mode can still be switched to Whisper without the
  files.
- The refused join's reply in Discord and the dialog's texts point to the new places.
- The Vosk model, its phrases and its download notice stay on Config → Assistant.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings-ui`: the Whisper model and the Local voice card move to the Settings tab; a
  missing model, device or voice is saved at once with its notice instead of a dialog; the
  Config → Assistant note with the button to Settings; the engine choice opens no dialog,
  and the Claude subscription dialog is no longer followed by a download dialog.
- `app-window`: the Config tab without the Local voice card; the install dialog opens only
  for a refused join, for the wake word as for the Whisper mode.
- `whisper-assistant-mode`: a switch to Whisper with files missing opens no dialog.
- `codex-assistant`: the refused join's reply in the Whisper mode points to Settings.
- `wake-word-detection`: a join in the realtime mode with the wake word is refused while the
  Vosk files are missing.

## Impact

- The window: `apps/desktop/ui/index.html`, `main.js`, `styles.css`; the command mock
  `ui-dev/mock.js` (a refused join for the wake word); the headless checks
  `check-wake-dialog.mjs`, `check-config-tab.mjs`, `check-whisper-config.mjs` and
  `check-engine-config.mjs`.
- The core: the join's preparation in `runtime.rs` checks the wake word's files; the reply
  texts (`stt::manager::JOIN_NEEDS_DOWNLOADS` and a new one for the wake word).
- Docs: `docs/ARCHITECTURE.md` (the window, the join), `docs/user-guide.md`, `AGENTS.md`
  (the window's cards, the mock, the checks).
- No new dependency, no settings change, no new command.
