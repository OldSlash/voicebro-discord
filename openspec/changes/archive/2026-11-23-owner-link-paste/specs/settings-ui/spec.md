## MODIFIED Requirements

### Requirement: Link code in the Access card
While a code is waiting, the Access card SHALL show the code, a Copy button, where to paste
the copied message in Discord, the time left and a Cancel button. Copy SHALL put on the
clipboard one message of the bot's mention, the word `link` and the code, and the card
SHALL then say it was copied. While the bot is not connected to Discord, Copy SHALL be
unavailable and the card SHALL say that it works once the bot is connected. When the code
ends without a link, the card SHALL say why: expired, cancelled or ended after 5 wrong
tries.

#### Scenario: Code shown
- **WHEN** the user presses Link while Codex is signed in and the bot is connected
- **THEN** the card shows an 8-character code, a Copy button, where to paste the copied message (a direct message to the bot, the chat of its voice channel or another channel it can read), a countdown from 10 minutes and a Cancel button

#### Scenario: Copy
- **WHEN** the code `K7QF-2M9X` waits, the bot's account id is `1234`, and the user presses Copy
- **THEN** the clipboard holds `<@1234> link K7QF-2M9X`, and the card says the message was copied and to paste it in Discord and send it

#### Scenario: Copy while the bot is not connected
- **WHEN** a code waits and the bot is not connected to Discord
- **THEN** the code is shown, Copy is unavailable, and the card says Copy works once the bot is connected

#### Scenario: Link completed
- **WHEN** the owner sends the code from Discord while the card shows it
- **THEN** the code disappears and the card shows the owner's name within a second

#### Scenario: Cancel
- **WHEN** the user presses Cancel
- **THEN** the code disappears, a later `/link` or message with it links nothing, and the card shows the previous link state

### Requirement: The Claude subscription dialog
The first time the user picks Claude Code in the window while its sign-in is a subscription
or not known yet and the access mode is not `owner`, a dialog SHALL say that a Claude
subscription may be used only by its holder and ask whether only the user's own voice
should reach the assistant, with "Hear only me" and "Not now". Either answer SHALL save
`claude.subscription_notice_shown` as `true`; the engine choice SHALL be saved either way.

#### Scenario: Hear only me, nobody linked
- **WHEN** the dialog is shown, no owner is linked, and the user presses "Hear only me"
- **THEN** `discord.access` is saved as `owner`, and the Access card opens with a link code shown, its Copy button and where to paste the copied message in Discord

#### Scenario: Hear only me, owner linked
- **WHEN** the dialog is shown, an owner is linked, and the user presses "Hear only me"
- **THEN** `discord.access` is saved as `owner` and no code is shown

#### Scenario: Not now
- **WHEN** the user presses "Not now" or closes the dialog
- **THEN** the access mode stays as it was, and the notice stays in the Claude Code card

#### Scenario: Shown once
- **WHEN** the dialog was answered before and the user picks Codex and then Claude Code again
- **THEN** no dialog opens

#### Scenario: API key
- **WHEN** Claude Code is signed in with an API key and the user picks Claude Code for the first time
- **THEN** no dialog opens

#### Scenario: Both dialogs
- **WHEN** the user picks Claude Code for the first time in the realtime mode with the Whisper files missing
- **THEN** the subscription dialog opens, no download dialog follows it, and Config → Assistant shows the note that the Whisper files are not downloaded
