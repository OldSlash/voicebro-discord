## ADDED Requirements

### Requirement: The bot needs no privileged intent
The app SHALL connect to the Discord gateway without any privileged intent (presence,
server members, message content). It SHALL still receive the text of the messages a link
code can come in: direct messages to the bot and server messages that mention it.

#### Scenario: Privileged intents off
- **WHEN** all three privileged intents are off for the bot in the Discord Developer Portal and the token is stored
- **THEN** the bot connects without Discord refusing its intents, and a link code sent in a direct message to it, or in a server channel with a mention of it, reaches the app with its text
