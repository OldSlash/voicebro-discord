# app-settings Specification

## Purpose
Keeps every user-facing setting of the app in one validated document that survives upgrades
and is applied without a restart.

## Requirements

### Requirement: Settings live in one versioned document
The app SHALL keep all settings in a single JSON document with a schema `version` field,
stored as `settings.json` in the folder the executable runs from. The document SHALL contain
no secrets.

#### Scenario: First start without a settings file
- **WHEN** the app starts and no settings file exists
- **THEN** it runs with the default settings and writes them to the settings file with the current schema version

#### Scenario: Settings file never contains a secret
- **WHEN** a settings file is written for any reason
- **THEN** it contains no Discord token or other credential, only a flag that one is set is available through the status

### Requirement: Settings are validated in one place
The app SHALL validate a settings document before storing or applying it, rejecting the
whole document with a message that names the offending field.

#### Scenario: Overlapping phrase lists
- **WHEN** a saved document has a phrase that appears in both the activation and the deactivation list of one model
- **THEN** the save is rejected, the message names the model and both lists, and the previous settings stay in effect

#### Scenario: The same phrase for two models
- **WHEN** a saved document has the same activation phrase for the English and the Russian model
- **THEN** the save succeeds

#### Scenario: Idle timeout out of range
- **WHEN** a saved document sets the idle timeout to a value other than 0 or 5..3600 seconds
- **THEN** the save is rejected and the message names the field and the allowed range

#### Scenario: Unknown wake model
- **WHEN** a saved document names a wake model that is neither in the model catalog nor a `local:` id with a plain folder name, or whose engine differs from the chosen engine
- **THEN** the save is rejected and the message names the model and the engine

### Requirement: Settings are written atomically
The app SHALL write settings to a temporary file in the same directory and rename it over the
previous file, so an interrupted write never leaves a missing or truncated settings file.

#### Scenario: Interrupted write
- **WHEN** the process is killed while a settings save is in progress
- **THEN** on the next start the settings file is either the previous complete document or the new complete document, never empty or partial

### Requirement: Older settings documents are migrated
The app SHALL load a settings document from an older schema version, fill missing fields with
defaults, keep unknown fields, and write the current version back.

#### Scenario: Missing field from an older version
- **WHEN** the settings file lacks a field that the current schema has
- **THEN** the app uses the field's default and the next save writes the field with the current schema version

#### Scenario: Unknown field from a newer version
- **WHEN** the settings file has a field the current schema does not know
- **THEN** the app keeps the field unchanged through load and save

### Requirement: Saved settings apply at once
The app SHALL apply a successfully saved document immediately, reconfiguring only the parts
whose section changed, without a restart.

#### Scenario: Only one section changed
- **WHEN** a save changes the idle timeout and nothing else
- **THEN** the session timeout is updated and the Discord connection, wake engine and assistant are not restarted

#### Scenario: Status reflects the new values
- **WHEN** a save succeeds
- **THEN** the next status snapshot and settings read return the saved values

### Requirement: Discord guild id setting
The settings SHALL hold `discord.guild_id`, a string of digits or empty, used to scope the
slash command registration. A value that is not empty and not all digits SHALL be rejected
naming the field.

#### Scenario: Valid guild id
- **WHEN** the user saves `discord.guild_id` as a string of digits
- **THEN** the save succeeds and the next connection registers the commands in that guild

#### Scenario: Invalid guild id
- **WHEN** the user saves `discord.guild_id` containing letters
- **THEN** the save is rejected and the message names `discord.guild_id`

### Requirement: Audio gain settings
The settings SHALL hold `audio.input_gain` and `audio.output_gain`, each a number from 0.0
to 4.0 with the default 1.0. A value outside that range SHALL be rejected with a message
naming the field. A settings file without the `audio` section SHALL load with the defaults.

#### Scenario: Valid gains
- **WHEN** the user saves an input gain of 3.39 and an output gain of 2.52
- **THEN** the save succeeds and the next status shows both values

#### Scenario: Out of range
- **WHEN** a saved document sets `audio.input_gain` to 5
- **THEN** the save is rejected and the message names `audio.input_gain` and the range 0 to 4

#### Scenario: Older file
- **WHEN** the settings file has no `audio` section
- **THEN** both gains are 1.0 and the next save writes the section

### Requirement: Appearance settings
The settings SHALL hold `ui.theme` (one of `dark`, `darcula`, `light`, `intellij`, default
`dark`) and `ui.font_size` (11 to 20, default 13). Other values SHALL be rejected naming the
field.

#### Scenario: Valid appearance
- **WHEN** the user saves the theme `intellij` and the font size 15
- **THEN** the save succeeds and the window applies both

#### Scenario: Unknown theme
- **WHEN** a saved document sets `ui.theme` to `pink`
- **THEN** the save is rejected and the message names `ui.theme` and the allowed themes

### Requirement: Answer handoff mode setting
The settings SHALL hold `codex.handoff_mode`, one of `items`, `commentary`, `bem_tags` and
`thinking`, with the default `items`. Any other value SHALL be rejected with a message naming
the field and the allowed values. A settings file without the field SHALL load with `items`.

#### Scenario: Valid choice
- **WHEN** the user saves `codex.handoff_mode` as `commentary`
- **THEN** the save succeeds and the stored settings hold `commentary`

#### Scenario: Unknown value
- **WHEN** a saved document sets `codex.handoff_mode` to `verbatim`
- **THEN** the save is rejected and the message names `codex.handoff_mode` and the four allowed values

#### Scenario: Older file
- **WHEN** the settings file has a `codex` section without `handoff_mode`
- **THEN** the field is `items` and the next save writes it

### Requirement: Diagnostics settings
The settings SHALL hold a `diagnostics` section with `performance` and `memory`, each a
boolean with the default `false`. A settings file without the section SHALL load with both
off, and the next save SHALL write the section. A document whose value is not a boolean
SHALL be rejected with the stored settings unchanged.

#### Scenario: Older file
- **WHEN** the settings file has no `diagnostics` section
- **THEN** both switches are off and the next save writes `diagnostics` with both fields

#### Scenario: Valid change
- **WHEN** the user saves `diagnostics.memory` as `true`
- **THEN** the save succeeds and the stored settings hold it

#### Scenario: Wrong type
- **WHEN** a saved document sets `diagnostics.performance` to the string `yes`
- **THEN** the save is rejected and the previous settings stay in effect

### Requirement: Diagnostics changes apply at once
A save that changes only the `diagnostics` section SHALL take effect without a restart and
SHALL restart nothing else: not the Discord connection, the wake engine, the assistant or a
running call.

#### Scenario: Only diagnostics changed
- **WHEN** a save changes `diagnostics.memory` and nothing else
- **THEN** the memory reading starts or stops, and the Discord connection, wake engine, assistant and call keep running untouched

### Requirement: Wake phrases per model
The `wake` section SHALL hold `phrases`, a map from a model id to its `activation` and
`deactivation` lists. A list that is absent SHALL mean the model's default phrases; a list
that is present, even empty, SHALL be used as stored. A save SHALL change only the lists it
names. The phrases of a model SHALL stay stored while another model is selected and while a
local model's folder is missing.

#### Scenario: Switching models keeps the phrases
- **WHEN** the user sets Russian phrases, selects the English model and later the Russian model again
- **THEN** the Russian phrases are the ones set before

#### Scenario: One model's list saved
- **WHEN** a save sets the English model's activation list to `["bro"]`
- **THEN** only that list changes, the English deactivation list stays absent and keeps the default, and the Russian model's phrases are untouched

#### Scenario: A settings file without phrases
- **WHEN** the app loads a settings file whose `wake.phrases` has no entry for the selected model
- **THEN** the file is not rewritten for it, and the model's default phrases are in effect

#### Scenario: An empty list stays empty
- **WHEN** a save sets the English model's deactivation list to `[]`
- **THEN** the file holds the empty list and no default deactivation phrase is used for the English model

### Requirement: Settings schema version 2
The app SHALL write schema version 2. Loading a version-1 document with phrases SHALL move
`wake.activation_phrases` and `wake.deactivation_phrases` under the entry of the model in
`wake.model`, or of `vosk-model-small-ru-0.22`, the only model of version 1, when it names
none. A version-1 document without phrases SHALL get the default English model. The two old
fields SHALL be removed, not kept as unknown fields.

#### Scenario: Version 1 with phrases
- **WHEN** a version-1 file selects the Russian model and has activation and deactivation phrases
- **THEN** after loading, both lists are under the Russian model's entry, the Russian model stays selected, the old fields are gone, and the next save writes version 2

#### Scenario: Version 1 without a model
- **WHEN** a version-1 file has phrases but no `wake.model`
- **THEN** the wake model becomes `vosk-model-small-ru-0.22` and the phrases are under its entry

#### Scenario: Version 1 without phrases
- **WHEN** a version-1 file has no phrase lists, or only empty ones, whether or not it names the Russian model
- **THEN** `wake.phrases` is empty, the wake model is `vosk-model-small-en-us-0.15`, and the old fields are gone

### Requirement: Log settings
The settings SHALL hold `diagnostics.verbose_log` and `assistant.save_chat_history`, each a
boolean with the default `false`. A settings file without them SHALL load with both off. A
document whose value is not a boolean SHALL be rejected with the stored settings unchanged.
A change SHALL apply at once and SHALL restart nothing else: not the Discord connection, the
wake engine, the assistant or a running call.

#### Scenario: Older file
- **WHEN** the settings file has neither field
- **THEN** both are off and the next save writes them

#### Scenario: Wrong type
- **WHEN** a saved document sets `assistant.save_chat_history` to the string `yes`
- **THEN** the save is rejected and the previous settings stay in effect

#### Scenario: Only the verbose log changed
- **WHEN** a save changes `diagnostics.verbose_log` and nothing else during a call
- **THEN** the file log follows the new value, and the Discord connection, wake engine, assistant and call keep running untouched

### Requirement: Opus codec setting
The `diagnostics` section SHALL hold `opus_codec`, a string that is `libopus` or `opus-rs`,
with the default `libopus`. A settings file without the field SHALL load with `libopus`,
and the next save SHALL write it. A saved document with any other value SHALL be rejected
with an error that lists the two values, and the stored settings SHALL stay unchanged. A
save that changes only this field SHALL restart nothing.

#### Scenario: Older file
- **WHEN** the settings file has a `diagnostics` section without `opus_codec`
- **THEN** the codec is `libopus` and the next save writes `"opus_codec": "libopus"`

#### Scenario: Valid change
- **WHEN** the user saves `diagnostics.opus_codec` as `opus-rs`
- **THEN** the save succeeds, the stored settings hold it, and the Discord connection, wake engine, assistant and any running call keep running untouched

#### Scenario: Unknown codec
- **WHEN** a saved document sets `diagnostics.opus_codec` to `opus2`
- **THEN** the save is rejected with an error naming `libopus` and `opus-rs`, and the previous settings stay in effect

### Requirement: A stored codec this build does not have loads as libopus
A settings file whose `diagnostics.opus_codec` names a codec this build does not have, such
as `ropus` or `mousiki` from a trial build of the four-codec comparison, SHALL load with
`libopus` and a log line naming the value it replaced, and the next save SHALL write
`libopus`. Such a file SHALL NOT make the load or a later save fail.

#### Scenario: ropus from a trial build
- **WHEN** the app starts with a settings file that has `"opus_codec": "ropus"`
- **THEN** the codec is libopus, the log says the stored codec ropus is not in this build and libopus is used, and saving any other setting succeeds and writes `"opus_codec": "libopus"`

### Requirement: Codex lists refresh setting
The settings SHALL hold `codex.refresh_lists_at_start`, a boolean with the default `false`.
A settings file without it SHALL load with `false`, and the next save SHALL write it. A
document whose value is not a boolean SHALL be rejected with the stored settings unchanged.
A save that changes only this field SHALL restart nothing and SHALL take effect at the next
start.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.0.1
- **THEN** `codex.refresh_lists_at_start` is `false`, and the next save writes it

#### Scenario: Turned on
- **WHEN** a save sets `codex.refresh_lists_at_start` to `true` while the bot is in a voice channel
- **THEN** the setting is stored, and the Discord connection, the wake engine, the assistant and a running call keep running

### Requirement: Collapsed cards setting
The settings SHALL hold `ui.collapsed_cards`, a list of the ids of the collapsed cards of
the Settings and Config tabs, empty by default. A settings file without it SHALL load with an empty list.
An id the window does not know SHALL be kept in the file and ignored by the window. A save
that changes only this field SHALL restart nothing.

#### Scenario: Older file
- **WHEN** the app loads a settings file without `ui.collapsed_cards`
- **THEN** the list is empty and every card of the Settings and Config tabs is expanded

#### Scenario: Unknown id
- **WHEN** the file lists `["audio", "later-card"]`
- **THEN** the file loads, the Audio card is collapsed, and the next save keeps both ids

#### Scenario: Not a list
- **WHEN** a save sends `ui.collapsed_cards` as a number
- **THEN** the save is rejected and the stored settings are unchanged

### Requirement: Answer posting settings
The settings SHALL hold `assistant.post_answers`, one of `off`, `voice_channel`,
`command_channel` and `channel` with the default `off`, and `assistant.answers_channel`, a
string of digits or empty with the default empty. A file without them SHALL load with the
defaults. A save SHALL be rejected, naming the field, when `post_answers` has another value,
when `answers_channel` is not digits, or when `post_answers` is `channel` and
`answers_channel` is empty.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.0.1
- **THEN** `assistant.post_answers` is `off`, `assistant.answers_channel` is empty, and the next save writes both

#### Scenario: Unknown destination
- **WHEN** a saved document sets `assistant.post_answers` to `thread`
- **THEN** the save is rejected with an error that names the field and lists the four values, and the previous settings stay in effect

#### Scenario: A channel without an id
- **WHEN** a saved document sets `assistant.post_answers` to `channel` and leaves `assistant.answers_channel` empty
- **THEN** the save is rejected with an error naming `assistant.answers_channel`

#### Scenario: Not an id
- **WHEN** a saved document sets `assistant.answers_channel` to `#general`
- **THEN** the save is rejected with an error naming `assistant.answers_channel`

#### Scenario: A stored channel kept while off
- **WHEN** a saved document sets `assistant.post_answers` to `off` and keeps a channel id in `assistant.answers_channel`
- **THEN** the save succeeds and the id stays stored for when `channel` is chosen again

### Requirement: Answer posting settings apply at once
A save that changes only `assistant.post_answers` or `assistant.answers_channel` SHALL
restart nothing: not the Discord connection, the wake engine, the assistant or a running
call. The next answer SHALL go where the new settings say.

#### Scenario: Changed during a call
- **WHEN** the user changes `assistant.post_answers` from `off` to `voice_channel` while a call runs and an agent task completes after the save
- **THEN** the answer is posted in the voice channel's chat, and the call, the Discord connection and the wake engine kept running through the save

### Requirement: Phrases are compared in their matching form
Validation SHALL compare wake phrases in the matching form the wake engine uses
(lower-case, "ё" as "е", the words separated by single spaces). The phrases SHALL be stored
as entered.

#### Scenario: The same phrase written two ways in both lists
- **WHEN** a saved document has "Братан всё" as an activation phrase and "братан  все" as a deactivation phrase of one model
- **THEN** the save is rejected, the message names the model and both lists, and the previous settings stay in effect

#### Scenario: A phrase kept as typed
- **WHEN** a saved document has "Отбой братан" as a deactivation phrase
- **THEN** the save succeeds and the settings file holds "Отбой братан"

### Requirement: Assistant mode setting
`assistant.mode` SHALL be `realtime`, `whisper` or `text`, default `realtime`. Any other
value SHALL be refused, naming the field and the three values. `realtime` SHALL be refused
while `assistant.engine` is not `codex`, with a message that the voice mode works only with
Codex. A change SHALL apply at once, as the requirement "The mode switches at once" of
`whisper-assistant-mode` says.

#### Scenario: Older file
- **WHEN** a settings file from 1.0.1 is loaded
- **THEN** the assistant mode is realtime, and the next save writes the field

#### Scenario: Unknown value
- **WHEN** a save sets `assistant.mode` to "local"
- **THEN** it is refused with a message naming `assistant.mode`, `realtime`, `whisper` and `text`, and the stored settings stay

#### Scenario: Realtime with Claude
- **WHEN** a save sets `assistant.engine` to `claude` and `assistant.mode` to `realtime`
- **THEN** it is refused with a message that the voice mode works only with Codex, and the stored settings stay

#### Scenario: Text answers with Claude
- **WHEN** a save sets `assistant.engine` to `claude` and `assistant.mode` to `text`
- **THEN** it is saved

### Requirement: Whisper settings
The `whisper` section SHALL hold:

| Field | Values | Default |
|---|---|---|
| `model` | a catalog id, or `local:<file name>` of a plain file name in `whisper-models\` | `whisper-base-q8_0` |
| `device` | `cpu` or `gpu` | `cpu` |
| `threads` | 0 (automatic) to 64 | 0 |
| `language` | `auto` or an ISO 639-1 code that Whisper knows | `auto` |

A value outside these SHALL be refused, naming the field.

#### Scenario: A path as a local model
- **WHEN** a save sets `whisper.model` to `local:..\other.bin`
- **THEN** it is refused, saying a local model is a plain file name of the Whisper models folder

### Requirement: Whisper phrase settings
The `whisper` section SHALL also hold `phrases.activation` and `phrases.deactivation`, each
a list of phrases, kept apart from the wake models' phrases. A list never set SHALL mean
the activation phrase "bro" or the deactivation phrase "peace out", whatever
`whisper.language` is. A value that is not a list of phrases SHALL be refused, naming the
field. The two phrase lists SHALL not share a phrase in the matching form. A phrase with no
letter or digit SHALL be refused, naming it.

#### Scenario: Shared phrase
- **WHEN** a save puts "Goodbye!" in the activation list and "goodbye" in the deactivation list
- **THEN** it is refused with a message naming `whisper.phrases.deactivation` and the phrase

#### Scenario: Defaults whatever the language
- **WHEN** the settings file has no `whisper.phrases` and `whisper.language` is `ru`
- **THEN** the Whisper phrases in effect are the activation "bro" and the deactivation "peace out"

### Requirement: Voice settings
The `tts` section SHALL hold `voice` (a voice catalog id, or `windows:<voice id>` for a
voice Windows has) and `speed` (0.5 to 2.0, default 1.0). The default voice SHALL be the
one the design names. A voice that is neither in the catalog nor of the form
`windows:<id>` SHALL be refused, naming the field.

#### Scenario: Speed out of range
- **WHEN** a save sets `tts.speed` to 3
- **THEN** it is refused with a message naming `tts.speed` and the range 0.5 to 2.0

### Requirement: Whisper and voice changes apply without a restart
A saved change of the `whisper` or `tts` section SHALL apply without a restart:

- the phrases, the language and the CPU threads from the next utterance;
- the model, device and voice by a background reload, while the bot is in a channel in
  Whisper mode, and otherwise at the next join;
- the speed from the next sentence.

Such a change SHALL rebuild no gate and no realtime call.

#### Scenario: Phrase added during a session
- **WHEN** the user adds a deactivation phrase while a session is active in Whisper mode
- **THEN** the next utterance holding it ends the session, and nothing was reloaded

### Requirement: Access mode setting
The settings SHALL hold `discord.access`, one of `everyone`, `agreed` or `owner`, with the
default `everyone`. A save that changes only this field SHALL restart nothing and SHALL apply
from the next voice frame. The settings SHALL hold no owner account and no consent record.

#### Scenario: Default
- **WHEN** the app starts with no settings file
- **THEN** `discord.access` is `everyone`

#### Scenario: Not a known value
- **WHEN** a save sends `discord.access` as `"friends"`
- **THEN** the save is rejected with an error naming `discord.access` and its three values, and the stored settings are unchanged

#### Scenario: Changed while in a channel
- **WHEN** the user saves `discord.access` as `agreed` while the bot is in a voice channel
- **THEN** the bot stays in the channel, no gate, wake engine or call is rebuilt, and the next frame is filtered

### Requirement: The owner-only switch of older files
A settings file without `discord.access` SHALL load the access mode from
`discord.owner_only`: `true` as `owner`, `false` or a missing field as `everyone`. The next
save SHALL write `discord.access` and SHALL NOT write `discord.owner_only`. When a file has
both, `discord.access` SHALL decide.

#### Scenario: Owner-only on in an older file
- **WHEN** the app loads a settings file from build 1.1.0 with `discord.owner_only: true`
- **THEN** the access mode is `owner`, and the next save writes `"access": "owner"` and no `owner_only`

#### Scenario: A file from build 1.0.1
- **WHEN** the app loads a settings file without either field
- **THEN** the access mode is `everyone`

### Requirement: Privacy policy URL setting
The settings SHALL hold `discord.privacy_url`, empty by default. A value SHALL be empty or
an `https://` URL with a host, at most 512 characters, stored trimmed. Any other value SHALL
be rejected with an error naming the field. A save that changes only this field SHALL
restart nothing.

#### Scenario: A valid URL
- **WHEN** the user saves `discord.privacy_url` as ` https://example.org/voicebro-privacy `
- **THEN** it is stored as `https://example.org/voicebro-privacy` and the next `/privacy` replies with it

#### Scenario: Not https
- **WHEN** a save sends `discord.privacy_url` as `http://example.org/privacy`
- **THEN** the save is rejected with an error naming `discord.privacy_url` and saying it must start with `https://`

### Requirement: One speaker at a time setting
The settings SHALL hold `assistant.one_speaker_at_a_time`, a boolean with the default
`false`. A file without it SHALL load with `false`. A save that changes only this field
SHALL rebuild no wake engine and no call, and SHALL apply from the next session in the
voice mode and from the next question in the Whisper mode.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.2.0
- **THEN** `assistant.one_speaker_at_a_time` is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `assistant.one_speaker_at_a_time` as the string `"yes"`
- **THEN** the save is rejected with an error naming the field and the stored settings are unchanged

### Requirement: Assistant engine setting
`assistant.engine` SHALL be `codex`, `claude` or `opencode`, default `codex`. Any other
value SHALL be refused, naming the field and the three values. A change SHALL apply at
once, as the requirement "The engine switches at once" of `assistant-engines` says, and
SHALL restart no Discord connection.

#### Scenario: Unknown engine
- **WHEN** a save sets `assistant.engine` to "gemini"
- **THEN** it is refused with a message naming `assistant.engine` and its three values, and the stored settings stay

#### Scenario: Engine and mode in one save
- **WHEN** the stored settings are Codex in the realtime mode and one save sets the engine to `opencode` and the mode to `whisper`
- **THEN** the save succeeds and both apply at once

### Requirement: Assistant instruction setting
`assistant.instructions` SHALL hold the user's instruction for the assistant, empty by
default, and SHALL apply to whichever engine runs, from its next conversation. The app
SHALL ship no instruction of its own.

#### Scenario: Instruction kept across engines
- **WHEN** the user saves an instruction with Codex as the engine and then picks Claude Code
- **THEN** Claude Code's next conversation gets the same instruction

### Requirement: The Codex instruction of older files
A settings file with `codex.instructions` and without `assistant.instructions` SHALL load
with the old field's text as `assistant.instructions`. The next save SHALL write
`assistant.instructions` and SHALL NOT write `codex.instructions`. When a file has both,
`assistant.instructions` SHALL decide.

#### Scenario: A file from build 1.2.0
- **WHEN** the app loads a settings file from 1.2.0 whose `codex.instructions` is "Answer in two sentences."
- **THEN** `assistant.instructions` is "Answer in two sentences.", and the next save writes it there and no `codex.instructions`

### Requirement: Claude Code settings
The settings SHALL hold a `claude` section with `executable` (empty means found
automatically), `model` (empty means Claude Code's default),
`subscription_notice_shown` (default `false`) and `subscription_notice_hidden` (default
`false`, a boolean only). A model SHALL have no spaces and at most 200 characters. A change
of the model SHALL apply from the guild's next Claude Code conversation.

#### Scenario: Defaults
- **WHEN** the app starts with no settings file
- **THEN** the `claude` section has an empty executable, an empty model, `subscription_notice_shown` false and `subscription_notice_hidden` false

#### Scenario: A model with a space
- **WHEN** a save sets `claude.model` to "sonnet 5"
- **THEN** it is refused with a message naming `claude.model`

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.3.0 without `claude.subscription_notice_hidden`
- **THEN** the field is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `claude.subscription_notice_hidden` as the string `"yes"`
- **THEN** the save is refused with an error naming the field, and the stored settings stay

### Requirement: OpenCode settings
The settings SHALL hold an `opencode` section with `executable` (empty means found
automatically) and `model` (empty means OpenCode's default, else `provider/model`, where
the model part may hold more `/`). A model SHALL have no spaces and a non-empty provider
and model part around its first `/`. A change of the model SHALL apply from the guild's
next question.

#### Scenario: A model without a provider
- **WHEN** a save sets `opencode.model` to "gpt-5"
- **THEN** it is refused with a message that the model must be written as `provider/model`

### Requirement: Interrupt on speech setting
The settings SHALL hold `assistant.interrupt_on_speech`, a boolean with the default
`false`. A file without it SHALL load with `false`. A value that is not a boolean SHALL be
refused, naming the field. A save that changes only this field SHALL rebuild no wake
engine, no gate and no call, and SHALL apply from the next tick.

#### Scenario: Older file
- **WHEN** the app loads a settings file from build 1.3.0
- **THEN** `assistant.interrupt_on_speech` is `false` and the next save writes it

#### Scenario: Not a boolean
- **WHEN** a save sends `assistant.interrupt_on_speech` as the string `"yes"`
- **THEN** the save is refused with an error naming the field and the stored settings are unchanged

#### Scenario: Turned on during an answer
- **WHEN** the user turns the setting on while the voice reads an answer in a voice channel
- **THEN** nothing is rebuilt, and a member who then talks for 600 ms stops the answer

### Requirement: Linux encryption setting
The settings SHALL hold `linux.encrypt_files`, a boolean, `true` by default; a settings file without it SHALL read as `true`, and a value that is not a boolean SHALL be rejected with an error naming the field. It SHALL decide only whether Linux headless (not tested) encrypts the stored Discord token, the owner link and the consent records, as the requirement "Optional encryption on Linux" of `headless-runtime` says. Windows SHALL ignore it. The window SHALL NOT show it, and a save from the window SHALL keep its stored value.

#### Scenario: Older settings file
- **WHEN** the app loads a settings file written before this field existed
- **THEN** `linux.encrypt_files` is `true`, and the file is not rewritten for it

#### Scenario: Not a boolean
- **WHEN** `settings.json` holds `"linux": {"encrypt_files": "no"}`
- **THEN** the settings are rejected with an error naming `linux.encrypt_files`

#### Scenario: Desktop save keeps the value
- **WHEN** the settings file holds `linux.encrypt_files` set to `false` and the user changes a setting in the window
- **THEN** the saved file still holds `linux.encrypt_files` set to `false`
