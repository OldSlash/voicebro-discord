## MODIFIED Requirements

### Requirement: Every setting is editable
Every field of the settings document SHALL have a control, split between the Settings and
Config tabs. The Settings tab holds the window's own settings: Appearance (theme, font
size) and Diagnostics (performance monitoring, memory measurement, verbose log, Opus
codec). The Config tab holds the bot's settings: "Discord and Codex" (token, guild id, Codex
executable), then Access (who the assistant hears, privacy policy URL); Audio (both gains,
earcons and their volume); Assistant (session mode, idle timeout, engine, wake engine and
model, activation and deactivation phrases, one speaker at a time); the engine card, titled
after the selected engine: for Codex refresh the lists at start, model, reasoning effort,
voice and answer handoff, for Claude Code its executable and model, for OpenCode its
executable and model, and for every engine speaker labels, save chat history, post answers
to Discord and its channel, and instructions; Hotkey (mute), in that order.

#### Scenario: Field for each setting
- **WHEN** the Settings and Config tabs are opened
- **THEN** each field above is present on its tab and shows the current value

#### Scenario: The Settings tab
- **WHEN** the user opens the Settings tab
- **THEN** it shows the Appearance and Diagnostics cards and no Access, Audio, Assistant, Codex or Hotkey card

#### Scenario: The Config tab
- **WHEN** the user opens the Config tab with Codex as the engine
- **THEN** it shows the cards "Discord and Codex", Access, Audio, Assistant, Codex and Hotkey, in that order

#### Scenario: The engine card with Claude Code
- **WHEN** the user opens the Config tab with Claude Code as the engine
- **THEN** the card after Local voice is titled "Claude Code" and holds the executable, the model, speaker labels, save chat history, post answers and instructions, and no Codex voice, handoff or reasoning field

### Requirement: Config tab fields
The Config tab's "Discord and Codex" card SHALL hold the Discord token (write-only, with set
and clear), the guild id and the Codex executable. The Access, Audio, Assistant, Local
voice, engine and Hotkey cards follow it on the same tab, in that order.

#### Scenario: Config fields
- **WHEN** the Config tab is opened
- **THEN** the token field shows whether a token is stored but never the token, and the guild id and the Codex executable show their stored values

#### Scenario: Card order
- **WHEN** the user opens the Config tab
- **THEN** the cards after "Discord and Codex" are Access, Audio, Assistant, Local voice, the engine card and Hotkey

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

## ADDED Requirements

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
- **THEN** the install dialog for the Whisper files opens as it does when Whisper is picked as the mode

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
- **THEN** the subscription dialog opens first and the Whisper files dialog after it is answered
