## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Audio (both gains, earcons and their volume); Assistant (session mode,
idle timeout, wake engine and model, activation and deactivation phrases); Codex (refresh
the lists at start, model, reasoning effort, voice, answer handoff, speaker labels, save
chat history, instructions); Hotkey (mute), in that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab
- **THEN** it shows the cards "Discord and Codex", Audio, Assistant, Codex and Hotkey, in that order

### Requirement: Config tab fields
The Config tab's "Discord and Codex" card SHALL hold the Discord token (write-only, with set
and clear), the guild id and the Codex executable. The Audio, Assistant, Codex and Hotkey
cards follow it on the same tab.

#### Scenario: Config fields
- **WHEN** the Config tab is opened
- **THEN** the token field shows whether a token is stored but never the token, and the guild id and the Codex executable show their stored values

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

## ADDED Requirements

### Requirement: Collapsible cards on the Settings and Config tabs
Every card of the Settings tab (Appearance, Diagnostics) and of the Config tab ("Discord and
Codex", Audio, Assistant, Codex, Hotkey) SHALL collapse to its title row and expand again
when the user activates the title, by mouse or keyboard. The title SHALL show whether the
card is collapsed. The state SHALL be saved and restored at the next start; every card SHALL
be expanded by default.

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

### Requirement: Switches are large
Every on/off setting SHALL be a switch control with an "On" or "Off" label next to it: Earcons,
Save chat history, Refresh the lists at start, Memory measurement, Verbose log, and
Performance monitoring both in the Diagnostics card and on the Performance tab. The switch
SHALL be at least 44 × 24 px, SHALL toggle when the switch or its On/Off label is clicked,
SHALL toggle with Space when focused, and SHALL show a visible focus mark.

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
