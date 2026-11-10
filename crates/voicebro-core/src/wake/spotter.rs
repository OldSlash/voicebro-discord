//! Decides when a phrase counts. Engine-independent: it only sees [`Recognition`]s.
//!
//! One detector per phrase kind, so a wake word earlier in a sentence never hides the
//! deactivation phrase after it. A phrase counts in a final result at once, or once it stays
//! in enough recent intermediate results. Each occurrence is reported once; the recognizer
//! is never reset, so the words after a phrase are not lost.

use super::{PhraseKind, PhraseSet, Recognition};
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

/// Every occurrence of the phrases among `words`, in spoken order, as (index of its first
/// word, number of words, phrase). Whole words only.
fn phrase_occurrences<'a>(
    words: &[&str],
    phrases: &'a [String],
) -> Vec<(usize, usize, &'a String)> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let matched = phrases.iter().find_map(|phrase| {
            let target: Vec<&str> = phrase.split_whitespace().collect();
            (!target.is_empty()
                && words.len() - index >= target.len()
                && words[index..index + target.len()] == target[..])
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
/// ended when the result carries word timings.
pub fn find_occurrences(
    recognition: &Recognition,
    phrases: &[String],
) -> Vec<(String, Option<f64>)> {
    if !recognition.words.is_empty() {
        let words: Vec<&str> = recognition.words.iter().map(|w| w.word.as_str()).collect();
        return phrase_occurrences(&words, phrases)
            .into_iter()
            .map(|(start, length, phrase)| {
                (
                    phrase.clone(),
                    Some(recognition.words[start + length - 1].end),
                )
            })
            .collect();
    }
    let words: Vec<&str> = recognition.text.split_whitespace().collect();
    phrase_occurrences(&words, phrases)
        .into_iter()
        .map(|(_, _, phrase)| (phrase.clone(), None))
        .collect()
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
}

impl Detector {
    pub fn new(needed: usize, window: usize) -> Self {
        Self {
            needed,
            window: window.max(needed),
            recent: VecDeque::new(),
            first_seen: None,
            reported: 0,
        }
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
        phrases: &[String],
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
        let first_seen = *self.first_seen.get_or_insert(stream_seconds);
        if self.recent.iter().filter(|seen| **seen).count() < self.needed {
            return None;
        }
        self.reported += 1;
        self.recent.clear();
        self.first_seen = None;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::Word;

    fn phrases(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
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

    fn find_phrase<'a>(text: &str, list: &'a [String]) -> Option<&'a String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        phrase_occurrences(&words, list).first().map(|(_, _, p)| *p)
    }

    #[test]
    fn phrases_match_whole_word_sequences_only() {
        let wake = phrases(&["hey buddy", "luna"]);
        assert_eq!(
            find_phrase("so hey buddy", &wake).map(String::as_str),
            Some("hey buddy")
        );
        assert_eq!(
            find_phrase("luna tell me", &wake).map(String::as_str),
            Some("luna")
        );
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
