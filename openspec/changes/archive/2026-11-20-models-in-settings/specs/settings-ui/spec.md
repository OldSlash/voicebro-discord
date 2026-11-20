# Spec Delta

## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings and what runs on this PC:
Appearance (theme, font size), Diagnostics (performance monitoring, memory measurement,
verbose log, Opus codec), Whisper (the Whisper model, device, CPU threads, language) and
Local voice (voice, speed). The Config tab holds the bot's settings: "Discord and Codex"
(token, guild id, Codex executable), then Access (who the assistant hears, privacy policy
URL); Audio (both gains, earcons and their volume); Assistant (assistant mode, engine, wake
model, activation and deactivation phrases, session mode, idle timeout, one speaker at a
time); the engine card, titled after the selected engine: for Codex refresh the lists at
start, model, reasoning effort, voice and answer handoff, for Claude Code its executable and
model, for OpenCode its executable and model, and for every engine speaker labels, save chat
history, post answers to Discord and its channel, and instructions; Hotkey (mute), in that
order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance, Diagnostics, Whisper and Local voice cards, in that order, and no Access, Audio, Assistant, engine or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab with Codex as the engine
- **THEN** it shows the cards "Discord and Codex", Access, Audio, Assistant, Codex and Hotkey, in that order, and no Local voice card

#### Scenario: The engine card with Claude Code
- **WHEN** the user opens the Config tab with Claude Code as the engine
- **THEN** the card after Assistant is titled "Claude Code" and holds the executable, the model, speaker labels, save chat history, post answers and instructions, and no Codex voice, handoff or reasoning field

### Requirement: Config tab fields
The Config tab's "Discord and Codex" card SHALL hold the Discord token (write-only, with set
and clear), the guild id and the Codex executable. The Access, Audio, Assistant, engine and
Hotkey cards follow it on the same tab, in that order.

#### Scenario: Config fields
- **WHEN** the Config tab is opened
- **THEN** the token field shows whether a token is stored but never the token, and the guild id and the Codex executable show their stored values

#### Scenario: Card order
- **WHEN** the user opens the Config tab
- **THEN** the cards after "Discord and Codex" are Access, Audio, Assistant, the engine card and Hotkey

### Requirement: Choosing a model that is not installed
When the user picks a model in the wake model list whose files or the Vosk library are not
on disk, the window SHALL save the choice at once and open no dialog. The Assistant card's
download notice SHALL then list the missing files with Download, and nothing SHALL be
downloaded until Download is pressed.

#### Scenario: Missing model picked
- **WHEN** the English model is installed and selected and the user picks the German model, which is not on disk
- **THEN** no dialog opens, the settings file names the German model, and the notice under Wake model lists the German model with its size and site and a Download button

#### Scenario: Download chosen
- **WHEN** the user presses Download in that notice
- **THEN** the German model's download starts and the notice shows the progress

#### Scenario: Cancel chosen
- **WHEN** the user picks the English model again before pressing Download
- **THEN** the English model is saved, the notice disappears, and nothing is downloaded

#### Scenario: The list while the dialog is open
- **WHEN** the install dialog of a refused join is open and the wake model list changes anyway (a script, a keyboard path)
- **THEN** the list goes back to the stored model, nothing is saved, and the dialog's Download keeps naming the model it asked about

#### Scenario: Installed model picked
- **WHEN** the user picks a model whose files and the Vosk library are on disk
- **THEN** no dialog opens, the choice is saved at once, and no notice shows

#### Scenario: Local model without the library
- **WHEN** the user picks a local model while the Vosk library is not on disk
- **THEN** the choice is saved and the notice lists only the Vosk library

### Requirement: Local voice card
The Settings tab SHALL have a Local voice card after the Whisper card. It SHALL hold:

- the voice, grouped by engine and language, each entry with its size and whether it is
  downloaded or built into Windows, and the voice's data license;
- the speed;
- Preview;
- the voice's download notice with a Download button.

A voice that misses files SHALL be saved at once when picked, in either assistant mode, and
its notice SHALL list the missing files; no dialog SHALL open.

#### Scenario: Preview
- **WHEN** the user presses Preview with a downloaded voice
- **THEN** a short neutral sample sentence in the voice's language plays in the window, and nothing is sent to Discord

#### Scenario: A voice that is not downloaded
- **WHEN** the mode is Whisper and the user picks a Piper voice whose files and phonemizer are not on disk
- **THEN** no dialog opens, the voice is saved, and the card's notice lists the phonemizer and the voice with their sizes and a Download button

### Requirement: Whisper card on the Settings tab
The Settings tab SHALL have a Whisper card after Appearance and Diagnostics. It SHALL hold:

- a label "On" or "Off" that follows the assistant mode, saying where to switch it;
- the Whisper model, grouped as Multilingual, English only, Russian-tuned and Your models,
  each entry with its size, its note and whether it is downloaded;
- the device (CPU, NVIDIA GPU), the CPU threads and the language;
- the card's download notice;
- the line with what is loaded, on which device, and the last answer's timings;
- "Open Whisper models folder".

A model or device that misses files SHALL be saved at once when picked, in either assistant
mode; no dialog SHALL open.

#### Scenario: Off
- **WHEN** the assistant mode is realtime voice and the user opens the Settings tab
- **THEN** the Whisper card's label says it is off and to switch it on in Config → Assistant, and its fields can still be changed

#### Scenario: GPU fallback shown
- **WHEN** the device is NVIDIA GPU and the status says the CPU is used
- **THEN** the Whisper card's line says "GPU requested, CPU used" with the reason

#### Scenario: A model that is not downloaded
- **WHEN** the mode is Whisper and the user picks the model "small", which is not on disk
- **THEN** no dialog opens, the model is saved, and the card's notice lists the model with its size and a Download button

### Requirement: Engine choice in the Assistant card
The Assistant card's Engine list SHALL offer Codex, Claude Code and OpenCode, saved on
change. Its help line SHALL say that only Codex has the voice mode and what each engine
needs. While Claude Code or OpenCode is chosen, the mode's "Realtime voice" choice SHALL be
disabled with "The voice mode works only with Codex".

#### Scenario: Claude Code chosen in the realtime mode
- **WHEN** the mode is Realtime voice and the user picks Claude Code
- **THEN** one save stores the engine Claude Code and the mode Whisper, the Recognition field shows "Whisper", and "Realtime voice" is disabled

#### Scenario: Whisper files missing
- **WHEN** the mode is Realtime voice, the Whisper files are not downloaded and the user picks OpenCode
- **THEN** no download dialog opens, and the Assistant card shows the note that the Whisper files are not downloaded with its button to Settings

#### Scenario: Back to Codex
- **WHEN** the engine is Claude Code in the Whisper mode and the user picks Codex
- **THEN** the engine is saved as Codex, the mode stays Whisper, and "Realtime voice" can be chosen again

### Requirement: The Claude subscription dialog
The first time the user picks Claude Code in the window while its sign-in is a subscription
or not known yet and the access mode is not `owner`, a dialog SHALL say that a Claude
subscription may be used only by its holder and ask whether only the user's own voice
should reach the assistant, with "Hear only me" and "Not now". Either answer SHALL save
`claude.subscription_notice_shown` as `true`; the engine choice SHALL be saved either way.

#### Scenario: Hear only me, nobody linked
- **WHEN** the dialog is shown, no owner is linked, and the user presses "Hear only me"
- **THEN** `discord.access` is saved as `owner`, the Access card opens with a link code shown, and the card says to type `/link` with the code in Discord

#### Scenario: Hear only me, owner linked
- **WHEN** the dialog is shown, an owner is linked, and the user presses "Hear only me"
- **THEN** `discord.access` is saved as `owner` and no code is shown

#### Scenario: Not now
- **WHEN** the user presses "Not now" or closes the dialog
- **THEN** the access mode stays as it was, and the notice stays in the Assistant and Access cards

#### Scenario: Shown once
- **WHEN** the dialog was answered before and the user picks Codex and then Claude Code again
- **THEN** no dialog opens

#### Scenario: API key
- **WHEN** Claude Code is signed in with an API key and the user picks Claude Code for the first time
- **THEN** no dialog opens

#### Scenario: Both dialogs
- **WHEN** the user picks Claude Code for the first time in the realtime mode with the Whisper files missing
- **THEN** the subscription dialog opens, no download dialog follows it, and Config → Assistant shows the note that the Whisper files are not downloaded

## ADDED Requirements

### Requirement: The Whisper card's download notice
The Settings tab's Whisper card SHALL show one download notice while the selected Whisper
model misses files for the selected device, in both assistant modes. It SHALL list each
missing file (the runtime for the device, the model, the VAD model) with its size and site,
the total, the download by hand and a Download button. Nothing SHALL be downloaded until
Download is pressed.

#### Scenario: Realtime mode with nothing of Whisper downloaded
- **WHEN** the assistant mode is realtime voice and nothing of Whisper is on disk
- **THEN** the Whisper card's notice lists the CPU runtime, the model "base (q8_0)" and the VAD model with their sizes, the total and a Download button

#### Scenario: GPU chosen without the CUDA runtime
- **WHEN** the user chooses NVIDIA GPU on a PC with only the CPU runtime and the selected model on disk
- **THEN** the notice lists the CUDA runtime, 685 MB from github.com, with Download, and nothing is fetched until Download is pressed

#### Scenario: Everything on disk
- **WHEN** the selected model, the runtime for the device and the VAD model are on disk
- **THEN** the Whisper card shows no download notice

### Requirement: Config → Assistant says what the Whisper mode misses
In the Whisper mode, while the selected Whisper model, its runtime or VAD model, or the
voice misses files, the Assistant card SHALL show a note under the assistant mode naming
what is not downloaded and saying to download it first, with an "Open Settings" button. The
button SHALL show the Settings tab with the first card that misses files expanded and in
view. The note SHALL go once nothing is missing, and SHALL not show in the realtime mode.

#### Scenario: Switched to Whisper with nothing downloaded
- **WHEN** the user picks Whisper as the assistant mode and nothing of Whisper or the voice is on disk
- **THEN** the mode is saved, and the Assistant card says that the Whisper model and the voice are not downloaded and to download them first in Settings, with "Open Settings"

#### Scenario: Open Settings
- **WHEN** the user presses "Open Settings" in that note while the Whisper card is collapsed
- **THEN** the Settings tab shows with the Whisper card expanded and its download notice in view

#### Scenario: Only the voice missing
- **WHEN** the Whisper files are on disk and the selected voice is not
- **THEN** the note names only the voice, and "Open Settings" shows the Local voice card

#### Scenario: Downloads done
- **WHEN** the downloads of the Whisper files and the voice finish
- **THEN** the note disappears from the Assistant card

## REMOVED Requirements

### Requirement: Whisper model in the Assistant card
**Reason**: The Whisper model moves to the Settings tab's Whisper card (the user's request
of 2026-11-20).
**Migration**: The model, its list and its notice are in Settings → Whisper; see
requirements "Whisper card on the Settings tab" and "The Whisper card's download notice".

### Requirement: Whisper's files can be downloaded before the switch
**Reason**: With the model on the Whisper card, the card's one notice lists the missing
files in both modes.
**Migration**: See requirement "The Whisper card's download notice".

### Requirement: The CUDA runtime's notice sits under the device
**Reason**: The CUDA runtime is listed in the Whisper card's one notice with the other
missing files.
**Migration**: See requirement "The Whisper card's download notice".
