//! The phrases and the questions in Whisper's text (change `whisper-mode`, design D4).
//! Whisper writes whole utterances with punctuation; the comparison uses the matching form
//! of change `voice-recognition-rework` ([`crate::settings::phrase_words`]), and the
//! question keeps the words as Whisper wrote them.

use crate::settings::{phrase_words, SessionMode};
use crate::wake::spotter::phrase_occurrences;
use crate::wake::PhraseSet;

/// What one transcribed utterance does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// An activation phrase while waiting: the session starts, and the words after the
    /// phrase in the same utterance, if any, are the first question.
    Activate {
        phrase: String,
        question: Option<String>,
    },
    /// A deactivation phrase while active: the session ends; the words before it are
    /// dropped.
    Deactivate { phrase: String },
    /// A question for the assistant.
    Question(String),
    /// Talk while waiting, or nothing to say: its text goes only to the detail log.
    Ignore,
}

/// The words of `text` in the matching form, each with the byte range of the original word
/// it came from (a word such as "по-моему" gives two words with their own ranges).
fn words_with_spans(text: &str) -> Vec<(String, usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices().chain([(text.len(), ' ')]) {
        match (character.is_alphanumeric(), start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                for word in phrase_words(&text[begin..index]) {
                    words.push((word, begin, index));
                }
                start = None;
            }
            _ => {}
        }
    }
    words
}

/// The text after byte `end`, without the punctuation and spaces in front of it; `None`
/// when it has no word.
fn rest_after(text: &str, end: usize) -> Option<String> {
    let rest = text[end..].trim_start_matches(|c: char| !c.is_alphanumeric());
    (!phrase_words(rest).is_empty()).then(|| rest.trim_end().to_owned())
}

/// What a transcribed utterance does in `mode`, with a session `active` or not.
pub fn decide(text: &str, mode: SessionMode, active: bool, phrases: &PhraseSet) -> Decision {
    let words = words_with_spans(text);
    if words.is_empty() {
        return Decision::Ignore;
    }
    let forms: Vec<&str> = words.iter().map(|(word, _, _)| word.as_str()).collect();
    if mode == SessionMode::Always {
        return Decision::Question(text.trim().to_owned());
    }
    if active {
        return match phrase_occurrences(&forms, &phrases.deactivation).first() {
            Some((_, _, phrase)) => Decision::Deactivate {
                phrase: phrase.text.clone(),
            },
            None => Decision::Question(text.trim().to_owned()),
        };
    }
    match phrase_occurrences(&forms, &phrases.activation).first() {
        Some(&(first, length, phrase)) => Decision::Activate {
            phrase: phrase.text.clone(),
            question: rest_after(text, words[first + length - 1].2),
        },
        None => Decision::Ignore,
    }
}

/// Whisper's `initial_prompt` from the phrases (design D4): every phrase in effect, joined
/// with commas, so they are spelled the way the user typed them; `None` without phrases.
pub fn prompt(phrases: &PhraseSet) -> Option<String> {
    let texts: Vec<&str> = phrases
        .activation
        .iter()
        .chain(&phrases.deactivation)
        .map(|phrase| phrase.text.as_str())
        .collect();
    (!texts.is_empty()).then(|| texts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrases(activation: &[&str], deactivation: &[&str]) -> PhraseSet {
        let owned = |list: &[&str]| {
            list.iter()
                .map(|text| (*text).to_owned())
                .collect::<Vec<_>>()
        };
        PhraseSet::new(&owned(activation), &owned(deactivation))
    }

    #[test]
    fn the_question_in_the_same_breath_follows_the_activation() {
        let set = phrases(&["hey bro"], &["goodbye"]);
        assert_eq!(
            decide(
                "Hey, bro! What is the weather in Bangkok?",
                SessionMode::WakeWord,
                false,
                &set
            ),
            Decision::Activate {
                phrase: "hey bro".to_owned(),
                question: Some("What is the weather in Bangkok?".to_owned()),
            }
        );
    }

    #[test]
    fn the_phrase_alone_starts_a_session_without_a_question() {
        let set = phrases(&["hey bro"], &["goodbye"]);
        assert_eq!(
            decide("Hey bro.", SessionMode::WakeWord, false, &set),
            Decision::Activate {
                phrase: "hey bro".to_owned(),
                question: None,
            }
        );
    }

    #[test]
    fn talk_while_waiting_is_ignored_and_a_deactivation_does_nothing() {
        let set = phrases(&["hey bro"], &["goodbye"]);
        for text in ["I think we should push left.", "Goodbye!", "   ", "..."] {
            assert_eq!(
                decide(text, SessionMode::WakeWord, false, &set),
                Decision::Ignore,
                "{text}"
            );
        }
    }

    #[test]
    fn a_goodbye_at_the_end_ends_the_session_and_drops_the_words_before_it() {
        let set = phrases(&["hey bro"], &["goodbye"]);
        assert_eq!(
            decide("Ok, thanks, goodbye.", SessionMode::WakeWord, true, &set),
            Decision::Deactivate {
                phrase: "goodbye".to_owned()
            }
        );
        assert_eq!(
            decide(
                "And how long is the flight to London?",
                SessionMode::WakeWord,
                true,
                &set
            ),
            Decision::Question("And how long is the flight to London?".to_owned())
        );
    }

    #[test]
    fn a_russian_phrase_matches_across_yo_and_punctuation() {
        let set = phrases(&["братан"], &["Отбой, братан!", "всё, хватит"]);
        assert_eq!(
            decide("Ладно. Отбой братан", SessionMode::WakeWord, true, &set),
            Decision::Deactivate {
                phrase: "Отбой, братан!".to_owned()
            }
        );
        assert_eq!(
            decide("Все, хватит.", SessionMode::WakeWord, true, &set),
            Decision::Deactivate {
                phrase: "всё, хватит".to_owned()
            }
        );
        assert_eq!(
            decide(
                "Братан, какая погода в Лондоне?",
                SessionMode::WakeWord,
                false,
                &set
            ),
            Decision::Activate {
                phrase: "братан".to_owned(),
                question: Some("какая погода в Лондоне?".to_owned()),
            }
        );
    }

    #[test]
    fn a_word_inside_another_word_is_no_phrase() {
        let set = phrases(&["bro"], &["stop"]);
        assert_eq!(
            decide("My brother says hi", SessionMode::WakeWord, false, &set),
            Decision::Ignore
        );
        assert_eq!(
            decide("Grab the stopwatch", SessionMode::WakeWord, true, &set),
            Decision::Question("Grab the stopwatch".to_owned())
        );
    }

    #[test]
    fn always_mode_sends_every_utterance_and_uses_no_phrase() {
        let set = phrases(&["hey bro"], &["goodbye"]);
        assert_eq!(
            decide("Goodbye, everyone.", SessionMode::Always, true, &set),
            Decision::Question("Goodbye, everyone.".to_owned())
        );
        assert_eq!(
            decide(" ", SessionMode::Always, true, &set),
            Decision::Ignore
        );
    }

    #[test]
    fn the_prompt_lists_every_phrase_as_typed() {
        assert_eq!(
            prompt(&phrases(&["братан", "hey bro"], &["Отбой, братан"])),
            Some("братан, hey bro, Отбой, братан".to_owned())
        );
        assert_eq!(prompt(&phrases(&[], &[])), None);
    }
}
