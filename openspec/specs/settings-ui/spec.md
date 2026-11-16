# settings-ui Specification

## Purpose
Lets the user configure everything the app does from its window, with immediate effect and
clear messages, so the settings file never has to be edited by hand.

## Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Access (owner only); Audio (both gains, earcons and their volume);
Assistant (session mode, idle timeout, wake engine and model, activation and deactivation
phrases); Codex (refresh the lists at start, model, reasoning effort, voice, answer handoff,
speaker labels, save chat history, post answers to Discord and its channel, instructions);
Hotkey (mute), in that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Access, Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab
- **THEN** it shows the cards "Discord and Codex", Access, Audio, Assistant, Codex and Hotkey, in that order

### Requirement: Config tab fields
The Config tab's "Discord and Codex" card SHALL hold the Discord token (write-only, with set
and clear), the guild id and the Codex executable. The Access, Audio, Assistant, Codex and
Hotkey cards follow it on the same tab.

#### Scenario: Config fields
- **WHEN** the Config tab is opened
- **THEN** the token field shows whether a token is stored but never the token, and the guild id and the Codex executable show their stored values

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
control, with that model's name in their labels. They SHALL accept one phrase per line or
comma-separated, show them trimmed and lower-cased after save, and show the words the wake
model does not know. Selecting another model SHALL show that model's phrases.

#### Scenario: Unknown word
- **WHEN** a saved phrase contains a word the model does not know
- **THEN** the word is listed under the field as unknown to the model

#### Scenario: Switching the model
- **WHEN** the user has Russian phrases and selects the English model
- **THEN** the phrase fields show the English model's phrases (empty the first time) and their labels name the English model; selecting the Russian model again shows the Russian phrases

#### Scenario: Phrases saved for the selected model
- **WHEN** the user enters "bro" as an activation phrase with the English model selected and leaves the field
- **THEN** the phrase is saved for the English model only

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
The Codex card SHALL have a "Save chat history" switch, off by default, saved
automatically and applied at once. Its help line SHALL say that while it is on every chat
entry is appended to `logs\codex-chat-<date>.log`, that while it is off nothing of the chat
is saved and the Codex chat tab still shows it, and that files saved earlier stay.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `assistant.save_chat_history`
- **THEN** the switch is shown off

#### Scenario: Turned on
- **WHEN** the user turns "Save chat history" on
- **THEN** the setting is saved without a button and a restart shows it on

### Requirement: Choosing a model that is not installed
When the user picks a model in the wake model list whose files or the Vosk library are not
on disk, the window SHALL open the install dialog before saving. Download SHALL save the
choice and start that model's download; Cancel SHALL put the list back on the stored model
and save nothing. A model whose files are all on disk SHALL be saved at once, as before.

#### Scenario: Missing model picked
- **WHEN** the English model is installed and selected and the user picks the German model, which is not on disk
- **THEN** the install dialog lists the German model with its size and site, and the settings file still names the English model

#### Scenario: Download chosen
- **WHEN** the user presses Download in that dialog
- **THEN** the German model is saved as the wake model, its download starts, and the dialog shows the progress

#### Scenario: Cancel chosen
- **WHEN** the user presses Cancel in that dialog
- **THEN** the list shows the English model again, nothing is saved, and nothing is downloaded

#### Scenario: Installed model picked
- **WHEN** the user picks a model whose files and the Vosk library are on disk
- **THEN** no dialog opens and the choice is saved at once

#### Scenario: The list while the dialog is open
- **WHEN** the install dialog is open and the wake model list changes anyway (a script, a keyboard path)
- **THEN** the list goes back to the stored model, nothing is saved, and the dialog's Download keeps naming the model it asked about

#### Scenario: Local model without the library
- **WHEN** the user picks a local model while the Vosk library is not on disk
- **THEN** the dialog lists only the Vosk library

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
Every card of the Settings tab (Appearance, Diagnostics) and of the Config tab ("Discord and
Codex", Access, Audio, Assistant, Codex, Hotkey) SHALL collapse to its title row and expand
again when the user activates the title, by mouse or keyboard. The title SHALL show whether
the card is collapsed. The state SHALL be saved and restored at the next start; every card
SHALL be expanded by default.

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
- **THEN** every card of the Settings and Config tabs is expanded

#### Scenario: A collapsed card keeps its fields
- **WHEN** the guild id has an unsaved edit and the user collapses and expands "Discord and Codex"
- **THEN** the edit is still in the field and marked as changed

#### Scenario: A collapsed Access card keeps its code
- **WHEN** a link code is waiting and the user collapses and expands the Access card
- **THEN** the same code and its countdown are shown again

### Requirement: Switches are large
Every on/off setting SHALL be a switch control with an "On" or "Off" label next to it: Only
my Discord account, Earcons, Save chat history, Refresh the lists at start, Memory
measurement, Verbose log, and Performance monitoring both in the Diagnostics card and on the
Performance tab. The switch SHALL be at least 44 × 24 px, SHALL toggle when the switch or
its On/Off label is clicked, SHALL toggle with Space when focused, and SHALL show a visible
focus mark.

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
The Codex card SHALL have a "Post answers to Discord" list after "Save chat history", with
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
The Config tab SHALL have an Access card after "Discord and Codex" with the switch "Only my
Discord account", saved on change, and the link state: the owner's Discord name, or "not
linked" with the reason when a link ended. Its help line SHALL say that while on only the
linked account's voice reaches the assistant, that everyone can still use the bot's
commands and hears the answers, and that a change applies at once.

#### Scenario: First start
- **WHEN** the window opens with a settings file without `discord.owner_only` and no link file
- **THEN** the Access card shows the switch off, "not linked" and a Link button

#### Scenario: Linked
- **WHEN** an owner is linked
- **THEN** the card shows the owner's Discord name and an Unlink button, and no Link code

#### Scenario: Switch on without a link
- **WHEN** the user turns the switch on while no owner is linked
- **THEN** the setting is saved and the card says that the assistant hears nobody until an account is linked

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
