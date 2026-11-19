# VoiceBro privacy policy template

## For the operator

You run your own copy of VoiceBro with your own Discord bot, so for Discord you are the
bot's developer. Discord's Developer Terms of Service (section 5a) require every bot to have
a privacy policy that says what data it collects, how it uses and shares that data, and how
people can ask for it to be deleted, with a public link to it in the Developer Portal and
from the bot itself.

The policy below describes what VoiceBro does. Some paragraphs depend on your settings,
first of all on **Config → Access → Who the assistant hears**. To use it:

1. Replace every `{{PLACEHOLDER}}` (see the table).
2. Keep or delete each paragraph marked *[Optional: …]* so that the text matches your
   settings, then delete the markers. Where a paragraph is marked *[Access: …]*, keep only
   the one for the access mode you use.
3. Publish the result at a public URL, for example a GitHub Gist or GitHub Pages.
4. Put the URL in the Discord Developer Portal (your application → General Information →
   Privacy Policy URL) and in **Config → Access → Privacy policy URL**, so that `/privacy`
   shows it and the `/optin` text links to it. `/optin` is refused while that field is
   empty.
5. Update the policy and its date whenever you change a setting it describes. A change of
   the AI provider or of the consent text also cancels the consents given so far: people
   run `/optin` again.

What people agree to with `/optin` (consent text version 1, shown with your policy URL):

> This bot runs an AI assistant. If you agree:
> - After someone in the channel says the activation phrase, what you say is sent to
>   OpenAI, through Codex so that the assistant can answer: your voice in the voice mode,
>   or, in the Whisper mode, the text of your speech written down on the operator's PC.
> - Your Discord id, your name and the time you agreed are kept on the operator's PC until
>   you withdraw.
> - /optout withdraws at once. What you said before was already sent and cannot be called
>   back.
>
> Privacy policy: (your URL)

This template is not legal advice. The laws where you and the people in your channel live
may ask for more.

| Placeholder | What to put there |
|---|---|
| `{{BOT_NAME}}` | Your bot's name in Discord |
| `{{OPERATOR}}` | Your name or nickname |
| `{{CONTACT_EMAIL}}` | An email address you read |
| `{{EFFECTIVE_DATE}}` | The date this version takes effect |
| `{{AI_PROVIDER}}` | The company behind the assistant: OpenAI (Codex), Anthropic (Claude) or Google (Gemini) |
| `{{AI_PROVIDER_PRIVACY_URL}}` | OpenAI: https://openai.com/policies/privacy-policy/ · Anthropic: https://www.anthropic.com/legal/privacy · Google: https://policies.google.com/privacy |
| `{{ANSWERS_CHANNEL}}` | Only with answer posting on: where answers are posted |
| `{{MINIMUM_AGE}}` | At least 13 (Discord's minimum). Use 18 with Google's Gemini API, whose terms bar apps likely to be used by people under 18; check your provider's terms |

---

# Privacy policy for {{BOT_NAME}}

Effective {{EFFECTIVE_DATE}}

{{BOT_NAME}} is a Discord bot that I, {{OPERATOR}}, run on my own computer with VoiceBro,
an open-source app. It lets people in a Discord voice channel talk to an AI assistant from
{{AI_PROVIDER}}. {{BOT_NAME}} is not made, endorsed or run by Discord or {{AI_PROVIDER}}.

Contact: {{CONTACT_EMAIL}}

## In short

- The bot is in a voice channel only after someone runs `/join`, until someone runs
  `/leave` or removes it.
- *[Access: People who agreed]* Your voice reaches the assistant only if you ran `/optin`
  and agreed. Without that, the bot drops your audio as it arrives.
- *[Access: Everyone]* The voice of everyone in the bot's voice channel can reach the
  assistant after the activation phrase.
- *[Access: Only my Discord account]* Only my own voice reaches the assistant; the bot
  drops everyone else's audio as it arrives.
- Your speech is not stored on my computer.
- *[Access: People who agreed]* `/optout` stops it at once and deletes your consent
  record.

## What the bot receives from Discord

- While the bot is in a voice channel, Discord sends it the audio of everyone who speaks
  there, as it does for every member of the channel.
- Your Discord user ID and display name, and whether you are in the bot's voice channel.
- The commands you send the bot, with your user ID.

## What happens to your voice

*[Access: People who agreed]* **If you have not run `/optin`:** your audio is dropped as it
arrives. It is not listened to, not stored and not sent anywhere. **If you have run
`/optin`,** the steps below apply to you.

*[Access: Only my Discord account]* **Everyone but me:** your audio is dropped as it
arrives. It is not listened to, not stored and not sent anywhere. The steps below apply
to my voice only.

*[Access: Everyone]* The steps below apply to everyone in the bot's voice channel.

1. VoiceBro listens on my computer for the activation phrase. This runs locally: nothing
   you say leaves my computer before the phrase.
2. From the phrase until the farewell phrase, a pause, or the end of the session, what you
   say is sent to {{AI_PROVIDER}} so that the assistant can answer:
   - *[Optional: keep if you use the voice mode]* in the voice mode, your voice as audio,
     starting shortly before the phrase;
   - *[Optional: keep if you use the Whisper mode]* in the Whisper mode, the text that
     VoiceBro writes down from your speech on my computer; no audio leaves my computer.
3. *[Optional: delete if Speaker labels is Off]* Your display name is sent with it, so
   that the assistant can tell speakers apart. *[Access: People who agreed or Only my
   Discord account]* Only the names of the people whose voice the bot lets through are
   sent.
4. The answer is spoken in the voice channel. *[Optional: keep if answer posting is on]*
   It is also posted as a message in {{ANSWERS_CHANNEL}}, where it is public and stays until
   it is deleted.

*[Optional: replace steps 1 and 2 with this if Session mode is Always]* There is no
activation phrase: everything you say while the bot is in the channel is sent to
{{AI_PROVIDER}}.

The bot tells people apart by their own Discord audio stream, not by the sound of their
voice. A voice that someone's microphone picks up from the room (a person sitting next to
them, or speakers without headphones) reaches the assistant together with that person's
voice.

## What {{AI_PROVIDER}} does with it

{{AI_PROVIDER}} handles what is sent under its own terms and privacy policy:
{{AI_PROVIDER_PRIVACY_URL}}. It may keep it for a limited time, for example to detect
abuse, and may process it outside your country.

*[Optional: keep if you sign in with a personal subscription rather than an API key]* The
bot uses my personal {{AI_PROVIDER}} subscription. Depending on my data settings there,
{{AI_PROVIDER}} may use the conversations to improve its models. *[Keep one:]* I have
turned this off. / This is on.

## What is stored on my computer

- **Your consent** (if you ran `/optin`): your user ID, display name, the time you agreed,
  the version of the consent text and the AI provider. It is encrypted for my Windows
  account and kept until you run `/optout`, until I remove it, or until I change the AI
  provider or the consent text, when everyone is asked to agree again.
- **The conversation:** held in memory only while the app runs, and gone when it closes.
  VoiceBro asks the assistant not to save the conversation.
- **The app's log:** technical events such as joins, errors and timings, which can include
  your display name. It does not contain what you said. Log files stay on my computer until
  I delete them.
- *[Optional: keep if "Save chat history" is on]* **Chat history:** who said what and the
  answers are written to text files on my computer and kept until I delete them.
- *[Optional: keep if the verbose log is on]* **Recognized speech:** what the speech
  recognizer heard is written to the log files.

## Who else gets your data

Only {{AI_PROVIDER}}, as described above, and Discord, which carries the bot's voice and
messages as it does for any member. I do not sell your data, share it for advertising, or
use it to train AI models.

## Your choices and rights

- Run `/optin` to let the bot hear you and `/optout` to stop. `/optout` takes effect at
  once and deletes your consent record.
- Run `/privacy` to see this policy.
- Email {{CONTACT_EMAIL}} to get a copy of what I hold about you or to have it deleted. I
  answer within 30 days.
- You can also leave the channel, block the bot, or ask a server admin to remove it.
- I cannot delete what {{AI_PROVIDER}} keeps (see its policy) or what Discord keeps.
- Depending on where you live, you may have more rights, such as those under the GDPR,
  including the right to complain to your data protection authority.

*[Access: People who agreed]* If you are in the EEA, the UK or Switzerland: I process your
voice on the basis of your consent (GDPR Article 6(1)(a)), which you can withdraw at any
time with `/optout`.

## Age

The bot is for people aged {{MINIMUM_AGE}} or older.

## Problems and reports

To report a problem with the bot or a breach of Discord's rules, email {{CONTACT_EMAIL}}.

## Changes

The date at the top shows when this policy last changed. If I change what is sent or who
receives it, the consents given before no longer count and the bot asks for a new `/optin`.
