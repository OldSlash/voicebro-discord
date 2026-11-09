//! How long the assistant takes to answer each turn, and where the time goes. The tracker
//! keeps no clock of its own: every call passes the time, so the rules are tested on
//! synthetic timelines.
//!
//! Two sides are tracked. The question side follows speech sent toward the assistant: speech
//! after more than 350 ms of quiet opens a new question, speech within that gap extends it.
//! The answer side follows the assistant's audio: audio within 1 s of the previous audio is
//! the same answer, anything later starts a new one, which belongs to the open question when
//! that question ended no more than 15 s earlier and has no answer yet, and is autonomous
//! otherwise. Speech while an answer is audible cuts into it: that is reported once per
//! answer and opens the question whose answer is labelled as coming after an interruption.
//! When an answer's first audio is played into Discord, its four durations are reported.

use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Speech separated by more than this starts a new turn.
pub const NEW_TURN_GAP: Duration = Duration::from_millis(350);
/// Answer audio closer than this to the previous answer audio continues that answer.
pub const RESPONSE_QUIET_GAP: Duration = Duration::from_millis(1_000);
/// Answer audio later than this after the last speech is not that turn's response.
pub const RESPONSE_MATCH_WINDOW: Duration = Duration::from_millis(15_000);
/// Peak at which the assistant's audio counts as a signal.
pub const SIGNAL_THRESHOLD: i32 = 256;
/// Turns kept for the status.
pub const RECENT_TURNS: usize = 50;

/// One finished measurement, as the log line and the status report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnReport {
    /// Local time of the playback, HH:MM:SS.
    pub at: String,
    /// `None` for autonomous speech.
    pub turn: Option<u64>,
    /// `response`, `response after interruption` or `autonomous`.
    pub kind: &'static str,
    /// First speech toward the assistant to the first answer audio.
    pub first_speech_to_answer_ms: Option<u64>,
    /// Last speech of the turn to the first answer audio.
    pub last_speech_to_answer_ms: Option<u64>,
    /// First answer audio received to its playback into Discord.
    pub answer_to_playback_ms: u64,
    /// First speech to the playback of the answer.
    pub first_speech_to_playback_ms: Option<u64>,
}

impl TurnReport {
    /// The log wording; "ChatGPT audio" is the answer.
    pub fn line(&self) -> String {
        let handoff = self.answer_to_playback_ms;
        match (
            self.turn,
            self.first_speech_to_answer_ms,
            self.last_speech_to_answer_ms,
            self.first_speech_to_playback_ms,
        ) {
            (Some(turn), Some(first), Some(last), Some(total)) => format!(
                "turn {turn} ({kind}): first Discord speech -> ChatGPT audio {first} ms; last Discord speech -> ChatGPT audio {last} ms; ChatGPT audio -> Discord relay handoff {handoff} ms; first speech -> Discord relay handoff {total} ms",
                kind = self.kind,
            ),
            _ => format!("autonomous ChatGPT speech: ChatGPT audio -> Discord relay handoff {handoff} ms"),
        }
    }
}

/// What a call to the tracker produced, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatencyEvent {
    /// Speech started while the assistant was speaking.
    Interrupted { turn: u64 },
    /// Answer audio arrived with no turn waiting for it.
    Autonomous,
    /// An answer was played; the turn's measurement is complete.
    Report(TurnReport),
}

impl LatencyEvent {
    pub fn line(&self) -> String {
        match self {
            Self::Interrupted { turn } => {
                format!("turn {turn}: Discord speech interrupted ChatGPT output")
            }
            Self::Autonomous => "ChatGPT initiated autonomous speech".to_owned(),
            Self::Report(report) => report.line(),
        }
    }
}

/// The span of speech that makes up one question.
#[derive(Debug, Clone, Copy)]
struct Question {
    number: u64,
    began: Instant,
    latest: Instant,
    /// The question was asked by talking over an answer.
    cuts_in: bool,
    /// An answer has been matched to it.
    answered: bool,
}

/// The assistant's current answer.
#[derive(Debug, Clone, Copy)]
struct Answer {
    /// The question it answers; `None` when the assistant spoke on its own.
    question: Option<Question>,
    first_audio: Instant,
    latest_audio: Instant,
    /// Labelled "response after interruption".
    after_cut_in: bool,
    /// Someone already talked over this answer; reported once.
    cut_into: bool,
    /// Its playback has been measured.
    measured: bool,
}

impl Answer {
    fn audible_at(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.latest_audio) <= RESPONSE_QUIET_GAP
    }

    fn kind(&self) -> &'static str {
        match self.question {
            None => "autonomous",
            Some(_) if self.after_cut_in => "response after interruption",
            Some(_) => "response",
        }
    }
}

fn millis(from: Instant, to: Instant) -> u64 {
    u64::try_from(to.saturating_duration_since(from).as_millis()).unwrap_or(u64::MAX)
}

#[derive(Default)]
pub struct LatencyTracker {
    turns: u64,
    question: Option<Question>,
    answer: Option<Answer>,
    measured: VecDeque<TurnReport>,
}

impl LatencyTracker {
    fn open_question(&mut self, now: Instant, cuts_in: bool) -> u64 {
        self.turns += 1;
        self.question = Some(Question {
            number: self.turns,
            began: now,
            latest: now,
            cuts_in,
            answered: false,
        });
        self.turns
    }

    fn extend_question(&mut self, now: Instant) {
        if let Some(question) = &mut self.question {
            question.latest = now;
        }
    }

    /// A frame with speech was sent toward the assistant at `now`.
    pub fn speech(&mut self, now: Instant) -> Option<LatencyEvent> {
        if let Some(answer) = self.answer.filter(|answer| answer.audible_at(now)) {
            if answer.cut_into {
                self.extend_question(now);
                return None;
            }
            let turn = self.open_question(now, true);
            if let Some(answer) = &mut self.answer {
                answer.cut_into = true;
                answer.after_cut_in = true;
            }
            return Some(LatencyEvent::Interrupted { turn });
        }
        let continues = self
            .question
            .is_some_and(|question| now.saturating_duration_since(question.latest) <= NEW_TURN_GAP);
        if continues {
            self.extend_question(now);
        } else {
            self.open_question(now, false);
        }
        None
    }

    /// The assistant's audio with a signal arrived at `now` (`received`: when its packet came
    /// from the network).
    pub fn answer_audio(&mut self, received: Instant) -> Option<LatencyEvent> {
        if let Some(answer) = &mut self.answer {
            if answer.audible_at(received) {
                answer.latest_audio = answer.latest_audio.max(received);
                return None;
            }
        }
        let question = self.question.as_mut().filter(|question| {
            !question.answered
                && received.saturating_duration_since(question.latest) <= RESPONSE_MATCH_WINDOW
        });
        let question = question.map(|question| {
            question.answered = true;
            *question
        });
        self.answer = Some(Answer {
            question,
            first_audio: received,
            latest_audio: received,
            after_cut_in: question.is_some_and(|question| question.cuts_in),
            cut_into: false,
            measured: false,
        });
        question.is_none().then_some(LatencyEvent::Autonomous)
    }

    /// The answer's audio was handed to Discord at `now` (`clock`: the local time of day for
    /// the report). The first time per answer it completes the measurement.
    pub fn played(&mut self, now: Instant, clock: &str) -> Option<LatencyEvent> {
        let answer = self.answer.as_mut().filter(|answer| !answer.measured)?;
        answer.measured = true;
        let answer = *answer;
        let began = answer.question.map(|question| question.began);
        let latest = answer.question.map(|question| question.latest);
        let report = TurnReport {
            at: clock.to_owned(),
            turn: answer.question.map(|question| question.number),
            kind: answer.kind(),
            first_speech_to_answer_ms: began.map(|began| millis(began, answer.first_audio)),
            last_speech_to_answer_ms: latest.map(|latest| millis(latest, answer.first_audio)),
            answer_to_playback_ms: millis(answer.first_audio, now),
            first_speech_to_playback_ms: began.map(|began| millis(began, now)),
        };
        while self.measured.len() >= RECENT_TURNS {
            self.measured.pop_front();
        }
        self.measured.push_back(report.clone());
        Some(LatencyEvent::Report(report))
    }

    /// An answer has started and its playback has not been reported yet.
    pub fn awaiting_playback(&self) -> bool {
        self.answer.is_some_and(|answer| !answer.measured)
    }

    /// The last measured answers, oldest first.
    pub fn recent(&self) -> Vec<TurnReport> {
        self.measured.iter().cloned().collect()
    }
}

/// One tracker per guild, fed by the gate (speech toward the assistant) and by the output
/// (the assistant's audio played into Discord).
pub type SharedLatency = std::sync::Arc<std::sync::Mutex<LatencyTracker>>;

/// The log line of an event, prefixed with the guild.
pub fn log_line(guild: u64, event: &LatencyEvent) -> String {
    format!("[latency:{guild}] {}", event.line())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    /// Speech every 20 ms from `from` to `to` milliseconds.
    fn speak(
        tracker: &mut LatencyTracker,
        start: Instant,
        from: u64,
        to: u64,
    ) -> Vec<LatencyEvent> {
        (from..to)
            .step_by(20)
            .filter_map(|ms| tracker.speech(at(start, ms)))
            .collect()
    }

    #[test]
    fn two_sentences_200_ms_apart_are_one_turn_and_the_answer_gives_four_durations() {
        let start = Instant::now();
        let mut tracker = LatencyTracker::default();
        assert!(speak(&mut tracker, start, 0, 1_000).is_empty());
        assert!(
            speak(&mut tracker, start, 1_200, 2_000).is_empty(),
            "a 200 ms pause"
        );
        assert_eq!(tracker.answer_audio(at(start, 3_480)), None);
        let Some(LatencyEvent::Report(report)) = tracker.played(at(start, 3_560), "12:00:03")
        else {
            panic!("the answer is reported when played");
        };
        assert_eq!(report.turn, Some(1), "one turn");
        assert_eq!(report.kind, "response");
        assert_eq!(report.first_speech_to_answer_ms, Some(3_480));
        assert_eq!(report.last_speech_to_answer_ms, Some(1_500));
        assert_eq!(report.answer_to_playback_ms, 80);
        assert_eq!(report.first_speech_to_playback_ms, Some(3_560));
        assert_eq!(
            report.line(),
            "turn 1 (response): first Discord speech -> ChatGPT audio 3480 ms; last Discord speech -> ChatGPT audio 1500 ms; ChatGPT audio -> Discord relay handoff 80 ms; first speech -> Discord relay handoff 3560 ms"
        );
        assert_eq!(report.at, "12:00:03");
        assert_eq!(
            tracker.played(at(start, 3_600), "12:00:03"),
            None,
            "once per answer"
        );
        // The rest of the same answer does not start another measurement.
        assert_eq!(tracker.answer_audio(at(start, 3_900)), None);
        assert_eq!(tracker.recent().len(), 1);
    }

    #[test]
    fn speech_during_an_answer_is_one_interruption_and_labels_the_next_answer() {
        let start = Instant::now();
        let mut tracker = LatencyTracker::default();
        speak(&mut tracker, start, 0, 600);
        tracker.answer_audio(at(start, 2_000));
        tracker.played(at(start, 2_060), "12:00:02");
        tracker.answer_audio(at(start, 2_500));
        let events = speak(&mut tracker, start, 2_600, 3_200);
        assert_eq!(
            events,
            vec![LatencyEvent::Interrupted { turn: 2 }],
            "reported once"
        );
        assert_eq!(
            events[0].line(),
            "turn 2: Discord speech interrupted ChatGPT output"
        );
        tracker.answer_audio(at(start, 5_000));
        let Some(LatencyEvent::Report(report)) = tracker.played(at(start, 5_050), "12:00:05")
        else {
            panic!("reported");
        };
        assert_eq!(report.turn, Some(2));
        assert_eq!(report.kind, "response after interruption");
    }

    #[test]
    fn an_answer_long_after_the_last_speech_is_autonomous() {
        let start = Instant::now();
        let mut tracker = LatencyTracker::default();
        speak(&mut tracker, start, 0, 500);
        assert_eq!(
            tracker.answer_audio(at(start, 20_500)),
            Some(LatencyEvent::Autonomous)
        );
        let Some(LatencyEvent::Report(report)) = tracker.played(at(start, 20_560), "12:00:20")
        else {
            panic!("reported");
        };
        assert_eq!(report.kind, "autonomous");
        assert_eq!(report.turn, None);
        assert_eq!(
            report.line(),
            "autonomous ChatGPT speech: ChatGPT audio -> Discord relay handoff 60 ms"
        );
    }

    #[test]
    fn the_status_keeps_the_last_50_turns() {
        let start = Instant::now();
        let mut tracker = LatencyTracker::default();
        for index in 0..60u64 {
            let base = index * 10_000;
            speak(&mut tracker, start, base, base + 200);
            tracker.answer_audio(at(start, base + 1_000));
            tracker.played(at(start, base + 1_050), "12:00:00");
        }
        let recent = tracker.recent();
        assert_eq!(recent.len(), RECENT_TURNS);
        assert_eq!(recent[0].turn, Some(11));
        assert_eq!(recent.last().unwrap().turn, Some(60));
    }
}
