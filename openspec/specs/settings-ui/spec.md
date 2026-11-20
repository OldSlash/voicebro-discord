# settings-ui Specification

## Purpose
Lets the user configure everything the app does from its window, with immediate effect and
clear messages, so the settings file never has to be edited by hand.

## Requirements

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

### Requirement: No auto-connect control
The window SHALL have no control for connecting at start-up: the bot always connects when a
token is stored.

#### Scenario: No auto-connect control
- **WHEN** the Settings or Config tab is opened
- **THEN** no control for connecting at start-up is shown, and the Config tab says auto-start is enabled

### Requirement: Save on change with validation
A field of the Settings tab and of the Config tab's Audio, Assistant, Codex and Hotkey cards
SHALL be saved automatically: a text field when it loses focus or on Enter, a slider when
released, a list or switch when changed. The fields of the "Discord and Codex" card SHALL
be saved with Save changes and discarded with Reset. A rejected value SHALL stay in the
field with the validation message next to it while the stored value stays unchanged; an
accepted value SHALL be applied at once.

#### Scenario: Invalid timeout
- **WHEN** the user enters 3 as the idle timeout and leaves the field
- **THEN** a message naming the allowed range appears at the field, the field still shows 3, and the stored timeout is unchanged

#### Scenario: Phrase overlap
- **WHEN** the user enters a deactivation phrase that is also an activation phrase
- **THEN** the message names the shared phrase and both lists

#### Scenario: List choice saved
- **WHEN** the user picks another Codex model from its list
- **THEN** the choice is saved without pressing a button and a restart shows it

#### Scenario: Two ways of saving on one tab
- **WHEN** the user moves the output gain slider on the Config tab while the guild id has an unsaved edit
- **THEN** the gain is saved at once, and the guild id stays unsaved and marked as changed until Save changes or Reset

### Requirement: Lists come from the live sources when available
The model and reasoning effort controls SHALL offer the values from Codex's model list,
read from Codex in this run or saved from the last read, and the voice control the voices
saved with it; they SHALL accept free text only while no list was ever read. The wake model
control SHALL offer the entries of the wake model list. Next to the model and reasoning
controls the tab SHALL say that they affect only the work the voice model delegates to the
Codex agent.

#### Scenario: Codex available
- **WHEN** the assistant process is running and signed in
- **THEN** the model control lists the models with their reasoning efforts and the current one is selected

#### Scenario: Saved lists at start
- **WHEN** the app starts with saved lists, the refresh at start off and no Codex process running
- **THEN** the model, reasoning and voice controls are lists filled from the saved lists, with the stored values selected, before any join or Check Codex

#### Scenario: Codex unavailable
- **WHEN** the assistant process is not running and no list was ever read
- **THEN** the model and effort controls accept free text and the last saved values are shown

### Requirement: Phrases are lists
The phrase controls SHALL show and save the phrases of the model selected in the wake model
control, with that model's name in their labels. Phrases SHALL be separated by commas (a line
break also separates); a phrase is one or more words separated by spaces. After a save they
SHALL show the phrases trimmed and joined with ", ", the model's default phrases for a list
never set, and the words the wake model does not know. Selecting another model SHALL show
that model's phrases.

#### Scenario: Unknown word
- **WHEN** a saved phrase contains a word the model does not know
- **THEN** the word is listed under the field as unknown to the model

#### Scenario: Switching the model
- **WHEN** the user has Russian phrases and selects the English model
- **THEN** the phrase fields show the English model's phrases (its default phrases the first time) and their labels name the English model; selecting the Russian model again shows the Russian phrases

#### Scenario: Phrases saved for the selected model
- **WHEN** the user enters "bro" as an activation phrase with the English model selected and leaves the field
- **THEN** the phrase is saved for the English model only

#### Scenario: Several phrases of several words
- **WHEN** the user enters "Отбой братан, Заканчивай, Братан всё" in the deactivation field of the Russian model and leaves the field
- **THEN** three phrases are saved in that order ("Отбой братан", "Заканчивай", "Братан всё") and the field shows "Отбой братан, Заканчивай, Братан всё"

#### Scenario: Default phrases shown
- **WHEN** the window opens on a first start with the English model selected
- **THEN** the activation field shows "bro" and the deactivation field shows "peace out"

#### Scenario: A field emptied
- **WHEN** the user empties the deactivation field and leaves it
- **THEN** an empty list is saved, the field stays empty, and the default does not come back

#### Scenario: Help text
- **WHEN** the user reads the help under the phrase fields
- **THEN** it says that phrases are separated by commas, that a phrase may have several words, that every word must be in the model's vocabulary, and that each model comes with default phrases

### Requirement: Audio card
The Config tab SHALL have an "Audio" card with a slider for "What the assistant hears"
(the input gain) and one for "How loud the assistant speaks" (the output gain), each from 0
to 400 % with its current percentage shown, plus the earcon switch and the earcon volume. A
change SHALL be saved automatically, without a Save button.

#### Scenario: Moving a slider
- **WHEN** the user moves "What the assistant hears" to 339 %
- **THEN** the label shows 339 %, the setting is saved as 3.39 without pressing a button, and a restart shows the same value

#### Scenario: Rejected value
- **WHEN** a save fails
- **THEN** the card shows the error next to the slider and the slider returns to the stored value

### Requirement: Edits survive refreshes
A field SHALL not be changed by periodic or event refreshes while it has focus, while it
holds a value that differs from the stored one, or while its list is open. Lists SHALL keep
their current selection when they are rebuilt.

#### Scenario: Events during a choice
- **WHEN** the user opens the wake model list and status events arrive for 30 s
- **THEN** the list stays open and the highlighted choice does not change

#### Scenario: Unsaved text
- **WHEN** the user types into the Config tab's guild id and an event arrives before Save changes
- **THEN** the typed text stays in the field

### Requirement: Save state is visible
A field whose value was just saved SHALL be marked on the field itself, without any text
line at the field: its name takes the success colour for about two seconds and then returns
to normal. The Settings tab and the Config tab SHALL each have one feedback line for their
automatically saved fields, which SHALL name the last field saved on that tab for the same
two seconds and then read "Changes save automatically" again. A rejected value SHALL show
its message at the field. A field of the "Discord and Codex" card with an unsaved change
SHALL be marked as changed until Save changes or Reset.

#### Scenario: Successful save
- **WHEN** the font size is saved
- **THEN** the field's name shows the success colour for about two seconds, the Settings tab's feedback line reads "Saved: Font size" for the same time, and no text appears at the field

#### Scenario: Save on the Config tab
- **WHEN** the mute hotkey is saved
- **THEN** the Config tab's feedback line reads "Saved: Mute hotkey" for about two seconds, and the Settings tab's line is unchanged

#### Scenario: Rejected value
- **WHEN** a value is rejected
- **THEN** the message appears at that field and stays until the field is corrected

#### Scenario: Unsaved Config change
- **WHEN** the user edits the Codex executable on the Config tab
- **THEN** the field is marked as changed until Save changes or Reset

### Requirement: Answer handoff control
The Codex card of the Config tab SHALL have an "Answer handoff" list with the four modes,
the default marked as recommended, and under the list a short hint for the chosen mode
saying what it does and how it behaved in the tests. A choice SHALL be saved automatically,
like the other lists of the card.

#### Scenario: Choosing a mode
- **WHEN** the user picks "Commentary" in the "Answer handoff" list
- **THEN** the hint under the list changes to the commentary hint, the choice is saved without a button, and a restart shows it

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `codex.handoff_mode`
- **THEN** the list shows the conversation-items mode, marked as recommended, with its hint

#### Scenario: Hint for the built-in routing
- **WHEN** the user picks the built-in routing
- **THEN** the hint says the voice model retells the agent's answer in its own words and, in the tests, sometimes answered with made-up facts before the agent had finished

### Requirement: A save moves no other field
Marking a field as saved SHALL NOT change the position or size of any field, note or card.
The two fields of a row SHALL keep their names and controls at the same height, whatever
their control type.

#### Scenario: Save in a two-column row
- **WHEN** "What the assistant hears" is saved while "How loud the assistant speaks" sits next to it
- **THEN** the second field, the note under the row and the cards below keep their positions

#### Scenario: Switch next to a slider
- **WHEN** the Audio card is shown
- **THEN** the Earcons switch and the Earcon volume slider have their names on one line and their controls on one line

### Requirement: Every setting has a help line
Every control on the Settings and Config tabs SHALL have its own help line under it that
says what the setting affects, when a change takes effect (at once, from the next call, at
the next session, or at the next connection) and the default where one exists.

#### Scenario: Every control described
- **WHEN** the Settings and Config tabs are opened
- **THEN** every control has a non-empty help line of its own

#### Scenario: Voice
- **WHEN** the user reads the help of the Voice control
- **THEN** it says the voice is the one the realtime call speaks with, that an empty value is the server's default voice, and that a change applies from the next call

#### Scenario: Activation
- **WHEN** the user reads the help of the Activation control
- **THEN** it says that in wake-word mode a session starts at an activation phrase and ends at a deactivation phrase or the auto-stop with nothing sent between sessions, that in always-listening mode the assistant hears the channel and the call runs the whole time the bot is in it, and that a change applies at once

#### Scenario: Gain
- **WHEN** the user reads the help of "What the assistant hears"
- **THEN** it says the gain scales only what the assistant receives during a session, that the wake word and the speech detection hear the channel unchanged, that 100 % is unchanged, and that a change applies at once, also in a running call

### Requirement: Diagnostics card
The Settings tab SHALL have a Diagnostics card with the switches "Performance monitoring"
and "Memory measurement", both off by default, each with a help line that names what it
measures, what it costs while on, and that it is off by default. A change SHALL be saved
automatically like the other switches and SHALL take effect at once. "Performance
monitoring" is the same setting as the switch on the Performance tab: the two SHALL always
show the same value.

#### Scenario: Mirrored switch
- **WHEN** the user turns performance monitoring on from the Performance tab and opens Settings
- **THEN** the Diagnostics card shows "Performance monitoring" on

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `diagnostics` section
- **THEN** both switches are shown off

#### Scenario: Turning performance monitoring on
- **WHEN** the user turns "Performance monitoring" on
- **THEN** the setting is saved without a button, and the Performance tab starts sampling within a second without a restart

#### Scenario: Turning memory measurement off
- **WHEN** the user turns "Memory measurement" off
- **THEN** the setting is saved without a button, the status bar's memory item disappears, and no further memory reading is taken

### Requirement: Wake model list
The wake model control SHALL list the catalog models and the local models found at start,
each with its language and whether it is downloaded, still to be downloaded (with its size)
or from the models folder. Its help line SHALL say that a model not on this PC is
downloaded only after the user agrees, where the models folder is, that small Vosk models
are the ones to use and that a restart picks up a new folder, and an "Open models folder"
button SHALL open the folder.

#### Scenario: Fresh install
- **WHEN** the app runs for the first time
- **THEN** the list shows the ten catalog models, each marked as still to be downloaded with its size, and the English model is selected

#### Scenario: Local model
- **WHEN** a model folder was found in `models\` at start
- **THEN** the list shows it marked as from the models folder

#### Scenario: Opening the folder
- **WHEN** the user presses "Open models folder"
- **THEN** the folder `models\` next to the executable opens in Explorer

### Requirement: Download notice
While the wake engine waits for a download, the Assistant card SHALL show under the wake
model control what will be downloaded, from which site and how many megabytes, with a
Download button, whether or not the selected model has phrases. The download SHALL start
only when the button is pressed; the notice SHALL then show the progress, and an error in
place with the button again.

#### Scenario: Notice
- **WHEN** wake-word mode is on and the English model is selected while nothing is on disk, with or without phrases
- **THEN** the notice lists the Vosk library (15 MB, github.com) and the English model (41 MB, alphacephei.com), and nothing downloads

#### Scenario: Download pressed
- **WHEN** the user presses Download
- **THEN** the notice shows the progress of each file and disappears when the files are installed

#### Scenario: Download failed
- **WHEN** a download fails or its hash does not match
- **THEN** the notice shows the error and the Download button again

### Requirement: Verbose log switch
The Diagnostics card SHALL have a "Verbose log" switch, off by default, saved automatically
and applied at once. Its help line SHALL say that it adds what the recognizer heard, the
latency and call counters, and the chat text to the log file, that the log folder stays
within about 2 MB, and that it is meant for diagnosing a problem.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `diagnostics.verbose_log`
- **THEN** the switch is shown off

#### Scenario: Turned on
- **WHEN** the user turns "Verbose log" on
- **THEN** the setting is saved without a button and the help line is shown under the switch

### Requirement: Chat history switch
The engine card SHALL have a "Save chat history" switch, off by default, saved
automatically and applied at once. Its help line SHALL say that while it is on every chat
entry is appended to `logs\chat-<date>.log`, that while it is off nothing of the chat is
saved and the Chat tab still shows it, and that files saved earlier stay.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.save_chat_history`
- **THEN** the switch is shown off

#### Scenario: Turned on
- **WHEN** the user turns "Save chat history" on
- **THEN** the setting is saved without a button and a restart shows it on

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

### Requirement: The download notice offers the download by hand
Next to its Download button the Assistant card's download notice SHALL show the same
download-by-hand part as the install dialog: each missing file's archive name with a link
to the official page it is published on, an "Open assets folder" button and the line on
putting the `.zip` files there as they are and pressing Download. The part SHALL be hidden
while a download runs and shown again after a failure.

#### Scenario: Notice with the links
- **WHEN** wake-word mode is on and the English model is selected while nothing is on disk
- **THEN** the notice names `vosk-win64-0.3.45.zip` with a link to the Vosk release page on github.com and `vosk-model-small-en-us-0.15.zip` with a link to the Vosk models page on alphacephei.com, with the "Open assets folder" button

#### Scenario: A link is clicked
- **WHEN** the user clicks one of the notice's links
- **THEN** the page opens in the default browser, the window stays on Config → Assistant, and no setting changes

#### Scenario: Downloading
- **WHEN** the user presses the notice's Download
- **THEN** the links and the folder button are hidden while the progress shows

#### Scenario: Download failed
- **WHEN** a download fails or its hash does not match
- **THEN** the notice shows the error, the Download button, the links and the folder button again

### Requirement: Opus codec control
The Diagnostics card SHALL have an "Opus codec (experiment)" list with the two codecs,
libopus first and opus-rs second, and a help line that says what the codec does, that the choice takes effect
at the next join and the next call, and that libopus is the default. Next to the list the
card SHALL show the codec in use on the Discord side and on the call, or that none is in use
while the bot is in no channel and no call is open. A change SHALL be saved automatically
like the other fields.

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `diagnostics.opus_codec`
- **THEN** the list shows libopus

#### Scenario: Changed while in a channel
- **WHEN** the bot is in a voice channel with libopus and the user picks opus-rs
- **THEN** the choice is saved without a button, and the card still shows libopus in use until the next join

#### Scenario: After the next join
- **WHEN** the user picked opus-rs, then `/leave` and `/join` ran
- **THEN** the card shows opus-rs in use on the Discord side

### Requirement: Collapsible cards on the Settings and Config tabs
Every card of the Settings tab (Appearance, Diagnostics), of the Config tab ("Discord and
Codex", Access, Audio, Assistant, Codex, Hotkey) and of the Whisper tab (Mode, Speech
recognition, Phrases, Voice) SHALL collapse to its title row and expand again when the
user activates the title, by mouse or keyboard. The title SHALL show whether the card is
collapsed. The state SHALL be saved and restored at the next start; every card SHALL be
expanded by default.

#### Scenario: Collapsing a card
- **WHEN** the user clicks the title of the Audio card
- **THEN** the card shows only its title row, and the cards below move up

#### Scenario: Collapsing a Settings card
- **WHEN** the user clicks the title of the Appearance card on the Settings tab
- **THEN** the card shows only its title row, the Diagnostics card moves up, and a restart shows Appearance collapsed

#### Scenario: Keyboard
- **WHEN** the title of the Hotkey card has the focus and the user presses Enter or Space
- **THEN** the card collapses, and the same key expands it again

#### Scenario: After a restart
- **WHEN** the user collapsed the Assistant and Hotkey cards and restarts the app
- **THEN** the Config tab opens with those two cards collapsed and the others expanded

#### Scenario: First start
- **WHEN** the window opens with a settings file that has no `ui.collapsed_cards`
- **THEN** every card of the Settings, Config and Whisper tabs is expanded

#### Scenario: A collapsed card keeps its fields
- **WHEN** the guild id has an unsaved edit and the user collapses and expands "Discord and Codex"
- **THEN** the edit is still in the field and marked as changed

#### Scenario: A collapsed Access card keeps its code
- **WHEN** a link code is waiting and the user collapses and expands the Access card
- **THEN** the same code and its countdown are shown again

#### Scenario: A Whisper card
- **WHEN** the user collapses the Voice card on the Whisper tab and restarts the app
- **THEN** the Whisper tab opens with the Voice card collapsed

### Requirement: Switches are large
Every on/off setting SHALL be a switch control with an "On" or "Off" label next to it:
Earcons, One speaker at a time, Save chat history, Refresh the lists at start, Memory
measurement, Verbose log, and Performance monitoring both in the Diagnostics card and on
the Performance tab. The switch SHALL be at least 44 × 24 px, SHALL toggle when the switch
or its On/Off label is clicked, SHALL toggle with Space when focused, and SHALL show a
visible focus mark.

#### Scenario: Size
- **WHEN** the Diagnostics card is shown at the default font size
- **THEN** the "Performance monitoring" switch measures at least 44 × 24 px

#### Scenario: Clicking the label
- **WHEN** the user clicks the "Off" label next to the "Memory measurement" switch
- **THEN** the switch turns on, the label reads "On", and the setting is saved without a button

#### Scenario: Keyboard
- **WHEN** the "Verbose log" switch has the focus and the user presses Space
- **THEN** the switch changes and the setting is saved

#### Scenario: Switch next to a slider
- **WHEN** the Audio card is shown
- **THEN** the Earcons switch and the Earcon volume slider have their names on one line and their controls on one line

### Requirement: Codex lists controls
The Codex card SHALL have a "Refresh the lists at start" switch, saved automatically, with
a help line saying that while on every start runs Codex briefly to read the models,
reasoning levels and voices, that while off the saved lists are shown, and that it is off by
default. Next to it the card SHALL say when the lists were last read, or that they were
never read, and SHALL have a "Refresh lists" button.

#### Scenario: Read time
- **WHEN** the lists were last read on 2026-11-14 at 18:30 and the app starts with the switch off
- **THEN** the card says the lists were read on 2026-11-14 at 18:30

#### Scenario: Never read
- **WHEN** the app starts with no saved lists and no Codex executable
- **THEN** the card says the lists were never read, and the model, reasoning and voice controls accept free text

#### Scenario: Refresh lists pressed
- **WHEN** the user presses "Refresh lists" with Codex installed and signed in
- **THEN** the button is disabled while Codex is read, the lists show what Codex returned, the read time becomes the current time, and the stored model, effort and voice stay selected

#### Scenario: Refresh failed
- **WHEN** the user presses "Refresh lists" and Codex cannot be started
- **THEN** the card shows the error next to the button, and the saved lists stay in the controls

#### Scenario: Switch turned on
- **WHEN** the user turns "Refresh the lists at start" on
- **THEN** the setting is saved without a button, nothing starts at once, and the next start reads the lists

### Requirement: Answer posting control
The engine card SHALL have a "Post answers to Discord" list after "Save chat history", with
Off, The voice channel's chat, The channel of /join and A chosen channel, saved on change.
Its help line SHALL say that the agent's final answers are posted there and that the bot
needs View Channel, Send Messages and Embed Links in that channel.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.post_answers`
- **THEN** the list shows Off and no channel list is shown

#### Scenario: Voice channel's chat chosen
- **WHEN** the user picks The voice channel's chat
- **THEN** it is saved without a button, no channel list is shown, and a restart shows the choice

### Requirement: Channel list for a chosen channel
While A chosen channel is picked, the card SHALL show a list of the text channels of every
server the bot is in, grouped by server. A channel the bot may not post in SHALL be marked
so. While the bot is not connected, the list SHALL show the stored channel by its id and say
that the names appear once the bot is connected.

#### Scenario: Picking a channel
- **WHEN** the user picks A chosen channel, then the channel "answers" of server A
- **THEN** `assistant.post_answers` and `assistant.answers_channel` are saved together with that channel's id, and the list shows "answers" under server A

#### Scenario: No channel picked yet
- **WHEN** the user picks A chosen channel while no channel is stored and leaves the channel list empty
- **THEN** nothing is saved, the stored destination stays as it was, and the card says to pick a channel

#### Scenario: A channel the bot may not post in
- **WHEN** the bot's role may not send messages in the channel "rules"
- **THEN** "rules" is shown in the list marked as a channel the bot cannot post in

#### Scenario: Bot not connected
- **WHEN** the window opens while the bot is not connected and a channel id is stored
- **THEN** the list shows that id as the chosen channel and a note that the channel names appear once the bot is connected

### Requirement: Access card
The Config tab SHALL have an Access card after "Discord and Codex" with the list "Who the
assistant hears" (Everyone, People who agreed, Only my Discord account), the field "Privacy
policy URL", both saved on change, and the link state: the owner's Discord name, or "not
linked" with the reason when a link ended. The list's help line SHALL say what each choice
lets through, that everyone can still use the bot's commands and hears the answers, and
that a change applies at once.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `discord.access` or `discord.owner_only` and no link file
- **THEN** the Access card shows "Everyone", an empty privacy policy URL, "not linked" and a Link button

#### Scenario: Linked
- **WHEN** an owner is linked
- **THEN** the card shows the owner's Discord name and an Unlink button, and no Link code

#### Scenario: Switch on without a link
- **WHEN** the user picks "Only my Discord account" while no owner is linked
- **THEN** the setting is saved and the card says that the assistant hears nobody until an account is linked

#### Scenario: Agreed mode without a policy
- **WHEN** the user picks "People who agreed" while the privacy policy URL is empty
- **THEN** the setting is saved and the card warns that `/optin` is refused until a privacy policy URL is set

#### Scenario: A wrong URL
- **WHEN** the user types `example.org/privacy` into the privacy policy URL and leaves the field
- **THEN** nothing is saved and the field says the URL must start with `https://`

### Requirement: Link code in the Access card
While a code is waiting, the Access card SHALL show the code, the command to type with it,
the time left and a Cancel button. When the code ends without a link, the card SHALL say why:
expired, cancelled or ended after 5 wrong tries.

#### Scenario: Code shown
- **WHEN** the user presses Link while Codex is signed in
- **THEN** the card shows an 8-character code, `/link` with that code, a countdown from 10 minutes and a Cancel button

#### Scenario: Link completed
- **WHEN** the owner sends the code from Discord while the card shows it
- **THEN** the code disappears and the card shows the owner's name within a second

#### Scenario: Cancel
- **WHEN** the user presses Cancel
- **THEN** the code disappears, a later `/link` with it links nothing, and the card shows the previous link state

### Requirement: Assistant mode in the Assistant card
The Config tab's Assistant card SHALL hold the assistant mode as its first field: "Realtime
voice" or "Whisper", with a help line saying what runs on the PC and what leaves it in each
mode. Below it a grey Recognition field SHALL show "Vosk" in the realtime mode and
"Whisper" in the Whisper mode; it follows the mode and cannot be edited.

#### Scenario: Switching to Whisper
- **WHEN** the user picks Whisper as the assistant mode on the Config tab
- **THEN** the mode is saved at once, and the Recognition field shows "Whisper"

### Requirement: Phrase fields follow the mode
The Assistant card's activation and deactivation phrase fields SHALL hold the selected Vosk
model's phrases in the realtime mode and the Whisper phrases (`whisper.phrases`) in the
Whisper mode. A save SHALL write the list of the mode shown. The help line SHALL say which
set the fields hold. The activation control (wake word or always) SHALL stay below them.

#### Scenario: Phrases per mode
- **WHEN** the user sets the activation phrase "hey bro" in the Whisper mode and switches back to the realtime mode
- **THEN** the fields show the Vosk model's phrases again, and "hey bro" is kept in `whisper.phrases`

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

### Requirement: The Local voice card says when the voice is used
The Local voice card SHALL show a note that follows the assistant mode. In the realtime
voice mode it SHALL say that the local voice is not used, because the Codex call speaks
with its own voice. In the Whisper mode it SHALL say that this voice reads Codex's answers
into the channel.

#### Scenario: Realtime mode
- **WHEN** the assistant mode is realtime voice
- **THEN** the Local voice card says the local voice is not used in this mode, and its fields and Download still work

### Requirement: Whisper card on the Settings tab
The Settings tab SHALL have a Whisper card after Appearance and Diagnostics. It SHALL hold:

- a label "On" or "Off" that follows the assistant mode, saying where to switch it;
- the Whisper model, grouped, each entry with its size, note and download state;
- the device (CPU, NVIDIA GPU), the CPU threads and the language;
- the card's download notice;
- the line with what is loaded, on which device, and the last answer's timings;
- "Open Whisper models folder".

#### Scenario: Off
- **WHEN** the assistant mode is realtime voice and the user opens the Settings tab
- **THEN** the Whisper card's label says it is off and to switch it on in Config → Assistant, and its fields can still be changed

#### Scenario: GPU fallback shown
- **WHEN** the device is NVIDIA GPU and the status says the CPU is used
- **THEN** the Whisper card's line says "GPU requested, CPU used" with the reason

#### Scenario: A model that is not downloaded
- **WHEN** the mode is Whisper and the user picks the model "small", which is not on disk
- **THEN** no dialog opens, the model is saved, and the card's notice lists the model with its size and a Download button

#### Scenario: The model list's groups
- **WHEN** the user opens the Whisper model list
- **THEN** it is grouped as Multilingual, English only, Russian-tuned and Your models

#### Scenario: A device that misses files in the realtime mode
- **WHEN** the mode is realtime voice and the user picks NVIDIA GPU without the CUDA runtime on disk
- **THEN** no dialog opens, the device is saved, and the card's notice lists the CUDA runtime

### Requirement: Realtime-only Codex fields say so
The Codex card's Voice and Answer handoff fields SHALL say in their help lines that they
apply to the realtime voice mode only.

#### Scenario: Voice help
- **WHEN** the user reads the help line of the Codex card's Voice field
- **THEN** it says the field applies to the realtime voice mode only

### Requirement: People who agreed in the Access card
The Access card SHALL list every person with a consent record, by Discord name with the
time they agreed and a Remove button, in every access mode, and SHALL say "Nobody yet"
when the list is empty. The list SHALL follow `/optin`, `/optout` and Remove within a
second. When the records file could not be read, the card SHALL say so.

#### Scenario: A new record
- **WHEN** a member presses "I agree" in Discord while the window is open
- **THEN** the card lists their name and time within a second

#### Scenario: Empty list
- **WHEN** nobody has a record
- **THEN** the list says "Nobody yet" and that people agree with `/optin` in Discord

#### Scenario: Unreadable file
- **WHEN** the app started with a records file it could not read
- **THEN** the card says the records could not be read and lists nobody

### Requirement: One speaker at a time in the Assistant card
The Assistant card SHALL hold the switch "One speaker at a time", off by default and saved
on change, in both assistant modes. Its help line SHALL say that while on the assistant
hears one person at a time in the voice mode (others wait and follow), that a question
during an answer waits instead of stopping it in the Whisper mode, and that the
deactivation phrase and Mute still stop an answer.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.one_speaker_at_a_time`
- **THEN** the Assistant card shows the switch off

#### Scenario: Turned on
- **WHEN** the user clicks the switch
- **THEN** it reads "On" and `assistant.one_speaker_at_a_time` is saved as `true` without a button

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

### Requirement: The engine card follows the engine
The engine card SHALL be titled "Codex", "Claude Code" or "OpenCode" after the selected
engine and SHALL show only that engine's own fields with the fields every engine shares. A
change of the engine SHALL change the card at once without losing an unsaved edit of a
shared field.

#### Scenario: Switch to OpenCode
- **WHEN** the user picks OpenCode as the engine
- **THEN** the engine card is titled "OpenCode" and shows the OpenCode executable and model with the shared fields

### Requirement: Claude Code card
With Claude Code as the engine, the engine card SHALL hold the Claude Code executable
(empty means found automatically) and the model: a list of the models read at the last
Check with "Claude Code's default" first, and a free entry for a name or alias. Their help
lines SHALL say where the app looks for Claude Code and that a model change applies from
the next conversation.

#### Scenario: No models read yet
- **WHEN** the user opens the Claude Code card before any Check
- **THEN** the model list offers "Claude Code's default" and the free entry

### Requirement: OpenCode card
With OpenCode as the engine, the engine card SHALL hold the OpenCode executable (empty
means found automatically) and the model: a list of the `provider/model` ids read at the
last Check with "OpenCode's default" first. The help lines SHALL say that providers and
keys are set up in OpenCode itself and that web searches go to OpenCode's search service.

#### Scenario: Models read
- **WHEN** the user presses Check with OpenCode installed and opens the OpenCode card
- **THEN** the model list offers the models OpenCode listed as `provider/model`

### Requirement: The Claude subscription notice in the window
While requirement "The Claude subscription notice" of `claude-assistant` calls for it, the
Assistant card SHALL show the notice under the Engine list and the Access card under "Who
the assistant hears". It SHALL link to Anthropic's Consumer Terms and to Claude Code's legal
page, and offer a button "Hear only me" that acts as in the dialog below.

#### Scenario: Notice in both cards
- **WHEN** the engine is Claude Code with a subscription and the access mode is "Everyone"
- **THEN** the Assistant and Access cards show the notice with both links and the button

#### Scenario: Notice gone
- **WHEN** the user picks "Only my Discord account" in the Access card
- **THEN** the notice disappears from both cards

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
