//! Decides when a phrase counts. Engine-independent: it only sees [`Recognition`]s.
//!
//! One detector per phrase kind, so a wake word earlier in a sentence never hides the
//! deactivation phrase after it. A phrase counts in a final result at once, or once it stays
//! in enough recent intermediate results. Each occurrence is reported once; the recognizer
//! is never reset, so the words after a phrase are not lost.

use super::{Phrase, PhraseKind, PhraseSet, Recognition};
use crate::settings::{fold_word, phrase_words};
use std::collections::VecDeque;

/// Intermediate results an activation phrase must stay in, in a row, before it counts
/// (about 300 ms). This filters words that only briefly look like a wake word.
pub const STABLE_PARTIAL_UPDATES: usize = 15;
/// A deactivation phrase counts once it is in this many of the last
/// `DEACTIVATION_WINDOW` intermediate results: noisy speech keeps revising the newest word.
pub const DEACTIVATION_PARTIAL_UPDATES: usize = 5;
pub const DEACTIVATION_WINDOW: usize = 10;
/// Intermediate results lag the audio a little; a phrase first seen in one ended about
/// this long before.
pub const PARTIAL_LAG_SECONDS: f64 = 0.15;

/// Every occurrence of the phrases among `words` (already in the matching form), in spoken
/// order, as (index of its first word, number of words, phrase). Whole words only; at one
/// place the first phrase of the list that fits wins.
pub(crate) fn phrase_occurrences<'a, W: AsRef<str>>(
    words: &[W],
    phrases: &'a [Phrase],
) -> Vec<(usize, usize, &'a Phrase)> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let matched = phrases.iter().find_map(|phrase| {
            let target = &phrase.words;
            (!target.is_empty()
                && words.len() - index >= target.len()
                && words[index..index + target.len()]
                    .iter()
                    .zip(target)
                    .all(|(word, wanted)| word.as_ref() == wanted))
            .then_some((target.len(), phrase))
        });
        match matched {
            Some((length, phrase)) => {
                found.push((index, length, phrase));
                index += length;
            }
            None => index += 1,
        }
    }
    found
}

/// The occurrences of the phrases in a result, in spoken order, each with the time it
/// ended when the result carries word timings. The recognizer's words are brought to the
/// matching form first; a word that becomes several ("по-моему") keeps the original word's
/// end time for each.
pub fn find_occurrences(
    recognition: &Recognition,
    phrases: &[Phrase],
) -> Vec<(String, Option<f64>)> {
    if !recognition.words.is_empty() {
        let mut words: Vec<String> = Vec::new();
        let mut ends: Vec<f64> = Vec::new();
        for word in &recognition.words {
            for piece in phrase_words(&word.word) {
                words.push(piece);
                ends.push(word.end);
            }
        }
        return phrase_occurrences(&words, phrases)
            .into_iter()
            .map(|(start, length, phrase)| (phrase.text.clone(), Some(ends[start + length - 1])))
            .collect();
    }
    let words = phrase_words(&recognition.text);
    phrase_occurrences(&words, phrases)
        .into_iter()
        .map(|(_, _, phrase)| (phrase.text.clone(), None))
        .collect()
}

/// A deactivation phrase found in the call's transcript of the people's speech.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMatch {
    /// The phrase as the user typed it.
    pub phrase: String,
    /// The match ends at the text's last word and nothing after it shows that the word
    /// ended: the next piece may still extend it ("Good" + "bye", "stop" + "watch").
    pub open: bool,
}

/// The first deactivation phrase in a turn's transcript so far, matched in the matching
/// form as whole words (design D2, D3). A match that ends before the last word, or at a
/// last word followed by a space or punctuation, is closed and ends the session at once.
pub fn transcript_match(text: &str, phrases: &[Phrase]) -> Option<TranscriptMatch> {
    transcript_match_after(text, phrases, 0)
}

/// [`transcript_match`] for a text that starts with `earlier_words` words already checked
/// (the previous turn's): only a match that ends after them counts.
pub fn transcript_match_after(
    text: &str,
    phrases: &[Phrase],
    earlier_words: usize,
) -> Option<TranscriptMatch> {
    if phrases.is_empty() {
        return None;
    }
    let words = phrase_words(text);
    let mut occurrences = phrase_occurrences(&words, phrases);
    occurrences.retain(|(start, length, _)| start + length > earlier_words);
    let word_ended = text.chars().last().is_some_and(|c| !c.is_alphanumeric());
    let closed = occurrences
        .iter()
        .find(|(start, length, _)| start + length < words.len() || word_ended);
    closed
        .or(occurrences.first())
        .map(|(start, length, phrase)| TranscriptMatch {
            phrase: phrase.text.clone(),
            open: start + length == words.len() && !word_ended,
        })
}

/// Whether a model knows a word of a phrase as typed: the lower-case word, its folded form
/// ("всё" → "все"), or, for a word with an apostrophe or hyphen, each of its pieces.
/// `knows` is the model's vocabulary check; `None` (cannot tell) counts as known.
pub fn word_known(token: &str, knows: &dyn Fn(&str) -> Option<bool>) -> bool {
    let known = |word: &str| knows(word) != Some(false);
    let lower = token.to_lowercase();
    let trimmed = lower.trim_matches(|c: char| !c.is_alphanumeric());
    if trimmed.is_empty() {
        return true;
    }
    known(trimmed)
        || known(&fold_word(trimmed))
        || crate::settings::lower_words(trimmed)
            .iter()
            .all(|piece| known(piece) || known(&fold_word(piece)))
}

/// Confirms one kind of phrase for one speaker.
pub struct Detector {
    needed: usize,
    window: usize,
    /// Whether the next unreported occurrence was in each of the recent results.
    recent: VecDeque<bool>,
    /// Stream time when that occurrence first appeared in an intermediate result.
    first_seen: Option<f64>,
    /// Occurrences already reported in the current utterance.
    reported: usize,
    /// Intermediate results since the last confirmation that held the next unreported
    /// occurrence, and that occurrence's phrase: a final result that confirms nothing new
    /// while this is above 0 is a near miss (design D5).
    seen_partials: usize,
    seen_phrase: Option<String>,
    near_miss: Option<(String, usize)>,
}

impl Detector {
    pub fn new(needed: usize, window: usize) -> Self {
        Self {
            needed,
            window: window.max(needed),
            recent: VecDeque::new(),
            first_seen: None,
            reported: 0,
            seen_partials: 0,
            seen_phrase: None,
            near_miss: None,
        }
    }

    /// The phrase and the number of intermediate results it was in, when the last final
    /// result ended an utterance without confirming it.
    pub fn take_near_miss(&mut self) -> Option<(String, usize)> {
        self.near_miss.take()
    }

    pub fn activation() -> Self {
        Self::new(STABLE_PARTIAL_UPDATES, STABLE_PARTIAL_UPDATES)
    }

    pub fn deactivation() -> Self {
        Self::new(DEACTIVATION_PARTIAL_UPDATES, DEACTIVATION_WINDOW)
    }

    /// Returns a newly confirmed phrase and how many seconds ago it ended.
    pub fn update(
        &mut self,
        recognition: &Recognition,
        phrases: &[Phrase],
        stream_seconds: f64,
    ) -> Option<(String, f64)> {
        let occurrences = if phrases.is_empty() {
            Vec::new()
        } else {
            find_occurrences(recognition, phrases)
        };
        let since_end = |end: f64| (stream_seconds - end).max(0.0);
        if recognition.is_final {
            // A final result ends the utterance: report what was not reported yet.
            let first_seen = self.first_seen.take();
            let fresh = occurrences.into_iter().nth(self.reported);
            self.recent.clear();
            self.reported = 0;
            let seen = std::mem::take(&mut self.seen_partials);
            let seen_phrase = self.seen_phrase.take();
            if fresh.is_none() && seen > 0 {
                self.near_miss = seen_phrase.map(|phrase| (phrase, seen));
            }
            return fresh.map(|(phrase, end)| {
                let end = end
                    .or(first_seen.map(|seen| seen - PARTIAL_LAG_SECONDS))
                    .unwrap_or(stream_seconds);
                (phrase, since_end(end))
            });
        }
        let pending = occurrences.into_iter().nth(self.reported);
        self.recent.push_back(pending.is_some());
        while self.recent.len() > self.window {
            self.recent.pop_front();
        }
        let Some((phrase, end)) = pending else {
            if !self.recent.contains(&true) {
                self.first_seen = None;
            }
            return None;
        };
        self.seen_partials += 1;
        self.seen_phrase = Some(phrase.clone());
        let first_seen = *self.first_seen.get_or_insert(stream_seconds);
        if self.recent.iter().filter(|seen| **seen).count() < self.needed {
            return None;
        }
        self.reported += 1;
        self.recent.clear();
        self.first_seen = None;
        self.seen_partials = 0;
        self.seen_phrase = None;
        Some((
            phrase,
            since_end(end.unwrap_or(first_seen - PARTIAL_LAG_SECONDS)),
        ))
    }
}

/// A confirmed phrase: its kind, the phrase, and how many seconds ago it ended.
pub type Hit = (PhraseKind, String, f64);

/// Both detectors of one speaker.
pub struct PhraseSpotter {
    activation: Detector,
    deactivation: Detector,
    /// A deactivation phrase was heard in the current utterance; an activation phrase
    /// later in it is part of the goodbye, not a new request.
    deactivated: bool,
}

impl Default for PhraseSpotter {
    fn default() -> Self {
        Self::new()
    }
}

impl PhraseSpotter {
    pub fn new() -> Self {
        Self {
            activation: Detector::activation(),
            deactivation: Detector::deactivation(),
            deactivated: false,
        }
    }

    pub fn update(
        &mut self,
        recognition: &Recognition,
        phrases: &PhraseSet,
        seconds: f64,
    ) -> Vec<Hit> {
        let mut hits = Vec::new();
        if let Some((phrase, since_end)) =
            self.deactivation
                .update(recognition, &phrases.deactivation, seconds)
        {
            self.deactivated = true;
            hits.push((PhraseKind::Deactivate, phrase, since_end));
        }
        if let Some((phrase, since_end)) =
            self.activation
                .update(recognition, &phrases.activation, seconds)
        {
            if !self.deactivated {
                hits.push((PhraseKind::Activate, phrase, since_end));
            }
        }
        if recognition.is_final {
            self.deactivated = false;
        }
        hits
    }

    /// A deactivation phrase seen in intermediate results that the utterance's final result
    /// did not confirm: the phrase and in how many results it was. Activation phrases are
    /// not reported.
    pub fn take_near_miss(&mut self) -> Option<(String, usize)> {
        self.deactivation.take_near_miss()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::Word;

    fn phrases(values: &[&str]) -> Vec<Phrase> {
        values.iter().filter_map(|v| Phrase::new(v)).collect()
    }

    fn word(word: &str, end: f64) -> Word {
        Word {
            word: word.to_owned(),
            end,
        }
    }

    fn partial(text: &str) -> Recognition {
        Recognition {
            is_final: false,
            text: text.to_owned(),
            words: Vec::new(),
        }
    }

    fn set() -> PhraseSet {
        PhraseSet {
            activation: phrases(&["hey buddy", "luna"]),
            deactivation: phrases(&["dismiss"]),
        }
    }

    fn find_phrase<'a>(text: &str, list: &'a [Phrase]) -> Option<&'a str> {
        let words = phrase_words(text);
        phrase_occurrences(&words, list)
            .first()
            .map(|(_, _, p)| p.text.as_str())
    }

    fn final_result(text: &str) -> Recognition {
        Recognition {
            is_final: true,
            text: text.to_owned(),
            words: Vec::new(),
        }
    }

    #[test]
    fn phrases_match_whole_word_sequences_only() {
        let wake = phrases(&["hey buddy", "luna"]);
        assert_eq!(find_phrase("so hey buddy", &wake), Some("hey buddy"));
        assert_eq!(find_phrase("luna tell me", &wake), Some("luna"));
        assert_eq!(find_phrase("hey bud", &wake), None);
        assert_eq!(
            find_phrase("lunar night", &wake),
            None,
            "a longer word is not the phrase"
        );
        assert_eq!(find_phrase("", &wake), None);
        let finished = Recognition {
            is_final: true,
            text: "listen hey buddy and luna".to_owned(),
            words: vec![
                word("listen", 0.4),
                word("hey", 0.7),
                word("buddy", 1.1),
                word("and", 1.2),
                word("luna", 1.6),
            ],
        };
        assert_eq!(
            find_occurrences(&finished, &wake),
            vec![
                ("hey buddy".to_owned(), Some(1.1)),
                ("luna".to_owned(), Some(1.6))
            ]
        );
    }

    /// Task 4.2, scenario "A phrase of several words".
    #[test]
    fn a_phrase_of_several_words_counts_once_inside_a_sentence() {
        let set = PhraseSet {
            activation: phrases(&["братан"]),
            deactivation: phrases(&["Отбой братан", "Заканчивай", "Братан всё"]),
        };
        let mut spotter = PhraseSpotter::new();
        let mut hits = Vec::new();
        for _ in 0..30 {
            hits.extend(spotter.update(&partial("ну все отбой братан спасибо"), &set, 1.0));
        }
        hits.extend(spotter.update(&final_result("ну все отбой братан спасибо"), &set, 2.0));
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].0, PhraseKind::Deactivate);
        assert_eq!(hits[0].1, "Отбой братан", "the phrase as typed");
    }

    /// Scenario "Only part of a phrase".
    #[test]
    fn only_part_of_a_phrase_does_not_count() {
        let set = PhraseSet {
            activation: Vec::new(),
            deactivation: phrases(&["отбой братан"]),
        };
        let mut spotter = PhraseSpotter::new();
        for _ in 0..30 {
            assert!(spotter.update(&partial("отбой"), &set, 1.0).is_empty());
        }
        assert!(spotter.update(&final_result("отбой"), &set, 2.0).is_empty());
    }

    /// Scenario "«ё» typed, «е» recognized".
    #[test]
    fn a_phrase_typed_with_yo_matches_the_recognizers_ye() {
        let set = PhraseSet {
            activation: Vec::new(),
            deactivation: phrases(&["Братан всё"]),
        };
        let mut spotter = PhraseSpotter::new();
        let finished = Recognition {
            is_final: true,
            text: "братан все".to_owned(),
            words: vec![word("братан", 0.5), word("все", 0.9)],
        };
        let hits = spotter.update(&finished, &set, 1.2);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, "Братан всё");
        assert!((hits[0].2 - 0.3).abs() < 1e-9);
        let known = |word: &str| Some(word != "всё");
        assert!(word_known("всё", &known), "known through its folded form");
        assert!(word_known("Всё,", &known));
        assert!(!word_known("zyx", &|_| Some(false)));
        let dont = |word: &str| Some(word == "don't");
        assert!(
            word_known("don't", &dont),
            "a word with an apostrophe as a whole"
        );
    }

    #[test]
    fn a_recognized_word_that_splits_keeps_its_end_time() {
        let finished = Recognition {
            is_final: true,
            text: "по-моему всё".to_owned(),
            words: vec![word("по-моему", 0.6), word("всё", 1.0)],
        };
        assert_eq!(
            find_occurrences(&finished, &phrases(&["моему", "по моему все"])),
            vec![("по моему все".to_owned(), Some(1.0))]
        );
        assert_eq!(
            find_occurrences(&finished, &phrases(&["моему"])),
            vec![("моему".to_owned(), Some(0.6))]
        );
    }

    /// Task 5.3: the transcript match.
    #[test]
    fn the_transcript_match_waits_for_a_word_that_may_go_on() {
        let goodbye = phrases(&["goodbye"]);
        assert_eq!(
            transcript_match("Okay, GOODBYE!", &goodbye),
            Some(TranscriptMatch {
                phrase: "goodbye".to_owned(),
                open: false
            })
        );
        assert_eq!(
            transcript_match(" Good", &goodbye),
            None,
            "the first piece alone is not the phrase"
        );
        assert_eq!(
            transcript_match(" Goodbye", &goodbye),
            Some(TranscriptMatch {
                phrase: "goodbye".to_owned(),
                open: true
            }),
            "\" Good\" + \"bye\" may still go on"
        );
        assert_eq!(
            transcript_match(" Goodbye then", &goodbye).map(|m| m.open),
            Some(false)
        );
        let stop = phrases(&["stop"]);
        assert_eq!(transcript_match(" stop", &stop).map(|m| m.open), Some(true));
        assert_eq!(transcript_match(" stopwatch", &stop), None);
        let russian = phrases(&["отбой, братан"]);
        assert_eq!(transcript_match(" Отбой", &russian), None);
        assert_eq!(transcript_match(" Отбой, брат", &russian), None);
        assert_eq!(
            transcript_match(" Отбой, братан", &russian),
            Some(TranscriptMatch {
                phrase: "отбой, братан".to_owned(),
                open: true
            })
        );
        assert_eq!(
            transcript_match(" Отбой, братан.", &russian).map(|m| m.open),
            Some(false)
        );
        assert_eq!(
            transcript_match("goodbye", &[]),
            None,
            "an empty list never matches"
        );
        assert_eq!(
            transcript_match("goodbye and goodbye", &goodbye).map(|m| m.open),
            Some(false),
            "a closed occurrence wins over an open one"
        );
    }

    /// Task 6.2: near misses of the deactivation phrase.
    #[test]
    fn a_deactivation_seen_in_partials_but_never_confirmed_is_a_near_miss() {
        let set = PhraseSet {
            activation: phrases(&["luna"]),
            deactivation: phrases(&["dismiss"]),
        };
        let mut spotter = PhraseSpotter::new();
        for _ in 0..3 {
            assert!(spotter.update(&partial("ok dismiss"), &set, 1.0).is_empty());
        }
        assert!(spotter
            .update(&final_result("ok this miss"), &set, 1.5)
            .is_empty());
        assert_eq!(spotter.take_near_miss(), Some(("dismiss".to_owned(), 3)));
        assert_eq!(spotter.take_near_miss(), None, "reported once");

        // Confirmed by intermediate results: no near miss.
        for _ in 0..10 {
            spotter.update(&partial("dismiss"), &set, 2.0);
        }
        spotter.update(&final_result("dismiss"), &set, 2.5);
        assert_eq!(spotter.take_near_miss(), None);
        // Confirmed by the final result: none either.
        for _ in 0..2 {
            spotter.update(&partial("dismiss"), &set, 3.0);
        }
        let hits = spotter.update(&final_result("dismiss"), &set, 3.5);
        assert_eq!(hits.len(), 1);
        assert_eq!(spotter.take_near_miss(), None);
        // An activation phrase seen but not confirmed is not reported.
        for _ in 0..5 {
            spotter.update(&partial("luna"), &set, 4.0);
        }
        spotter.update(&final_result("lunar"), &set, 4.5);
        assert_eq!(spotter.take_near_miss(), None);
    }

    fn feed_until_hit(spotter: &mut PhraseSpotter, text: &str, limit: usize) -> Vec<Hit> {
        for _ in 0..limit {
            let hits = spotter.update(&partial(text), &set(), 1.0);
            if !hits.is_empty() {
                return hits;
            }
        }
        Vec::new()
    }

    #[test]
    fn a_wake_word_earlier_in_the_sentence_does_not_hide_the_deactivation() {
        let mut spotter = PhraseSpotter::new();
        let hits = feed_until_hit(&mut spotter, "ok luna dismiss", 30);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].0,
            PhraseKind::Deactivate,
            "deactivation is confirmed first"
        );
        assert!(feed_until_hit(&mut spotter, "ok luna dismiss", 30).is_empty());
        let finished = Recognition {
            is_final: true,
            text: "ok luna dismiss".to_owned(),
            words: vec![word("ok", 0.3), word("luna", 0.7), word("dismiss", 1.1)],
        };
        assert!(spotter.update(&finished, &set(), 1.5).is_empty());
        let hits = feed_until_hit(&mut spotter, "luna question", 30);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, PhraseKind::Activate);
    }

    #[test]
    fn deactivation_then_activation_in_one_utterance_starts_nothing() {
        let mut spotter = PhraseSpotter::new();
        let hits = feed_until_hit(&mut spotter, "dismiss luna", 30);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, PhraseKind::Deactivate);
        assert!(feed_until_hit(&mut spotter, "dismiss luna", 30).is_empty());
    }

    #[test]
    fn deactivation_tolerates_flickering_intermediate_results() {
        let mut detector = Detector::deactivation();
        let texts = [
            "dismiss",
            "dis miss",
            "dismiss",
            "dismissed",
            "dismiss",
            "dis miss",
            "dismiss",
            "dismiss",
        ];
        let mut confirmed = None;
        for (index, text) in texts.iter().enumerate() {
            if detector
                .update(&partial(text), &phrases(&["dismiss"]), 1.0)
                .is_some()
            {
                confirmed = Some(index);
                break;
            }
        }
        assert_eq!(
            confirmed,
            Some(7),
            "five of the last ten results contain the phrase"
        );
        for _ in 0..20 {
            assert!(detector
                .update(&partial("dismiss"), &phrases(&["dismiss"]), 1.2)
                .is_none());
        }
        let mut second = None;
        for _ in 0..20 {
            second = second.or(detector.update(
                &partial("dismiss dismiss"),
                &phrases(&["dismiss"]),
                1.4,
            ));
        }
        assert!(second.is_some(), "a second occurrence is new");
    }

    #[test]
    fn a_final_result_reports_only_what_was_not_reported() {
        let mut detector = Detector::activation();
        for _ in 0..STABLE_PARTIAL_UPDATES {
            let _ = detector.update(&partial("luna"), &phrases(&["luna"]), 1.0);
        }
        let finished = Recognition {
            is_final: true,
            text: "luna".to_owned(),
            words: vec![word("luna", 0.8)],
        };
        assert!(detector
            .update(&finished, &phrases(&["luna"]), 1.5)
            .is_none());
        let finished_twice = Recognition {
            is_final: true,
            text: "luna luna".to_owned(),
            words: vec![word("luna", 0.8), word("luna", 1.6)],
        };
        let (_, since_end) = detector
            .update(&finished_twice, &phrases(&["luna"]), 2.0)
            .expect("a final result alone confirms a phrase");
        assert!((since_end - 1.2).abs() < 1e-9);
    }

    #[test]
    fn detector_needs_a_stable_partial_or_a_final_result() {
        let wake = phrases(&["luna"]);
        let mut detector = Detector::activation();
        let partial_hit = partial("hey luna");
        assert!(detector.update(&partial_hit, &wake, 1.0).is_none());
        for _ in 2..STABLE_PARTIAL_UPDATES {
            assert!(detector.update(&partial_hit, &wake, 1.2).is_none());
        }
        let (phrase, since_end) = detector
            .update(&partial_hit, &wake, 1.3)
            .expect("stable phrase");
        assert_eq!(phrase, "luna");
        assert!((since_end - (0.3 + PARTIAL_LAG_SECONDS)).abs() < 1e-9);

        let mut detector = Detector::activation();
        for _ in 0..5 {
            assert!(detector.update(&partial_hit, &wake, 1.2).is_none());
        }
        let changed = Recognition {
            is_final: false,
            text: "lunar".to_owned(),
            words: vec![word("lunar", 1.4)],
        };
        for _ in 0..30 {
            assert!(
                detector.update(&changed, &wake, 1.4).is_none(),
                "a brief look-alike never fires"
            );
        }

        let finished = Recognition {
            is_final: true,
            text: "luna what time is it".to_owned(),
            words: vec![word("luna", 0.5), word("what", 1.0), word("time", 1.5)],
        };
        let (_, since_end) = detector
            .update(&finished, &wake, 2.5)
            .expect("final result");
        assert!((since_end - 2.0).abs() < 1e-9);
    }
}
