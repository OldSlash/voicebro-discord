//! A link code pasted into Discord (change `owner-link-paste`, designs D2 and D3): which
//! messages are link attempts, what the bot logs for one, and its answer. Nothing of a
//! message is kept, and its text never reaches the log or the assistant.

use crate::events::EventBus;
use crate::owner::CompleteOutcome;
use serenity::all::{CreateAllowedMentions, CreateMessage};

/// What the handler knows about a message before it reads its text (design D2's table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageFacts {
    /// The author is a bot account.
    pub from_bot: bool,
    /// A webhook posted it.
    pub webhook: bool,
    /// A regular message or a reply, not a system message.
    pub regular: bool,
    /// A direct message to the bot (no server).
    pub direct: bool,
    /// The message mentions the bot's user (a role mention or `@everyone` does not count).
    pub mentions_bot: bool,
}

/// Whether the message may be a link attempt: a person's regular message in a direct
/// message to the bot, or in a server with the bot's mention. Every other message is
/// dropped before its text is looked at.
pub fn is_candidate(facts: &MessageFacts) -> bool {
    !facts.from_bot && !facts.webhook && facts.regular && (facts.direct || facts.mentions_bot)
}

/// The code part of a link attempt in `text` for the bot `bot_id`, or `None` when the text
/// is no link attempt. Without the bot's mentions the text is `link` or `/link` (any case)
/// followed by the attempt, which may be empty or wrong, or a code alone.
pub fn link_attempt(text: &str, bot_id: u64) -> Option<String> {
    let rest = text
        .replace(&format!("<@!{bot_id}>"), " ")
        .replace(&format!("<@{bot_id}>"), " ");
    let rest = rest.trim();
    // ASCII lower case keeps every byte where it was, so the prefix length slices `rest`.
    let lower = rest.to_ascii_lowercase();
    for word in ["/link", "link"] {
        if let Some(after) = lower.strip_prefix(word) {
            if after.is_empty() || after.starts_with(char::is_whitespace) {
                return Some(rest[word.len()..].trim().to_owned());
            }
        }
    }
    crate::owner::normalize_code(rest).map(|_| rest.to_owned())
}

/// Hands a link attempt from a message to the owner state as a `/link`, after the log line
/// that names the sender and the place, never the text (task 1.4).
pub(crate) fn complete(
    host: &dyn super::VoiceHost,
    events: &EventBus,
    user: u64,
    name: &str,
    guild: Option<u64>,
    attempt: &str,
) -> CompleteOutcome {
    let place = match guild {
        None => "a direct message".to_owned(),
        Some(guild) => format!("server {guild}"),
    };
    events.log(format!("owner link: a link message from {name} in {place}"));
    host.complete_link(user, name, attempt)
}

/// The bot's answer in the chat the message came from: a plain message, no reference (so
/// Read Message History is not needed), notifying nobody.
pub fn answer(text: &str) -> CreateMessage {
    CreateMessage::new()
        .content(text)
        .allowed_mentions(CreateAllowedMentions::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOT: u64 = 1568533518601031690;

    #[test]
    fn the_copied_message_and_its_variants_are_link_attempts() {
        let attempt = |text: &str| link_attempt(text, BOT);
        assert_eq!(
            attempt(&format!("<@{BOT}> link K7QF-2M9X")).as_deref(),
            Some("K7QF-2M9X")
        );
        assert_eq!(
            attempt(&format!("<@!{BOT}> link K7QF-2M9X")).as_deref(),
            Some("K7QF-2M9X")
        );
        assert_eq!(attempt("/link k7qf2m9x").as_deref(), Some("k7qf2m9x"));
        assert_eq!(attempt("LINK k7qf 2m9x").as_deref(), Some("k7qf 2m9x"));
        assert_eq!(attempt("K7QF-2M9X").as_deref(), Some("K7QF-2M9X"));
        assert_eq!(
            attempt(&format!("k7qf2m9x <@{BOT}>")).as_deref(),
            Some("k7qf2m9x")
        );
        assert_eq!(attempt("link").as_deref(), Some(""));
        assert_eq!(attempt("link hello").as_deref(), Some("hello"));
    }

    #[test]
    fn other_texts_are_no_link_attempts() {
        for text in [
            "hello".to_owned(),
            "linked".to_owned(),
            String::new(),
            format!("<@{BOT}> hello"),
            "<@999> K7QF-2M9X".to_owned(),
            "<@&123> K7QF-2M9X".to_owned(),
        ] {
            assert_eq!(link_attempt(&text, BOT), None, "{text}");
        }
    }

    #[test]
    fn only_a_persons_message_to_the_bot_is_a_candidate() {
        let person = MessageFacts {
            from_bot: false,
            webhook: false,
            regular: true,
            direct: false,
            mentions_bot: true,
        };
        assert!(
            is_candidate(&person),
            "a server message with the bot's mention"
        );
        assert!(is_candidate(&MessageFacts {
            direct: true,
            mentions_bot: false,
            ..person
        }));
        for (facts, what) in [
            (
                MessageFacts {
                    from_bot: true,
                    ..person
                },
                "a bot",
            ),
            (
                MessageFacts {
                    webhook: true,
                    ..person
                },
                "a webhook",
            ),
            (
                MessageFacts {
                    regular: false,
                    ..person
                },
                "a system message",
            ),
            (
                MessageFacts {
                    mentions_bot: false,
                    ..person
                },
                "a server message without the bot's mention (a role or @everyone only)",
            ),
        ] {
            assert!(!is_candidate(&facts), "{what}");
        }
    }

    #[test]
    fn the_answer_notifies_nobody_and_refers_to_no_message() {
        let body = serde_json::to_value(answer("Linked: Alex is now the owner.")).unwrap();
        assert_eq!(body["content"], "Linked: Alex is now the owner.");
        assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));
        assert!(
            body.get("message_reference")
                .is_none_or(|value| value.is_null()),
            "{body}"
        );
    }
}
