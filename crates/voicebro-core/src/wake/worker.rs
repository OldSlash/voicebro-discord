//! The wake worker thread: one recognizer per speaker, the trailing silence counted in the
//! guild's receive ticks, and the log lines that diagnose most wake-word reports.
//!
//! A speaker's absence is counted in receive ticks, never by when this thread processes the
//! audio (change `voice-recognition-rework`, design D1): songbird reports a speaker silent for
//! 100 ms or more while it refills a playout buffer and then plays the delayed audio, a late
//! receive tick holds frames back, and a busy worker processes them late. None of that is
//! silence, so none of it gets zeros fed into the word. Silence is fed only once a speaker
//! has been absent from [`SILENCE_AFTER_TICKS`] ticks in a row.

use super::spotter::{word_known, Hit, PhraseSpotter};
use super::{
    LoadedModel, Phrase, PhraseKind, PhraseOutcome, PhraseSet, PhraseSink, SpeechRecognizer,
    WakeManager, WakeMessage,
};
use crate::audio::dsp::StereoDownsampler;
use crate::events::EventBus;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A speaker absent from this many receive ticks in a row (300 ms) has stopped talking:
/// songbird's refill gap (100 ms after packets resume) plus up to about 200 ms of lateness
/// stays below it.
pub const SILENCE_AFTER_TICKS: u64 = 15;
/// Trailing silence fed after a speaker stops, in 20 ms chunks: 1.5 s, enough for the
/// recognizer to end the utterance and confirm a phrase at the end of a sentence.
pub const MAX_SILENCE_CHUNKS: u64 = 75;
/// A gap of this many ticks or more that the speaker's audio came back from is counted as
/// bridged for the diagnostics line.
pub const BRIDGED_GAP_TICKS: u64 = 3;
/// The worker wakes at least this often to report and to drop idle speakers.
pub const WORKER_TICK: Duration = Duration::from_millis(20);
/// A speaker without a frame for this long (wall time) loses its recognizer.
pub const SPEAKER_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// The bridged-gaps line, the lag line and the drop line come at most this often.
pub const REPORT_INTERVAL: Duration = Duration::from_secs(10);
/// Processing a frame later than this after the gate fed it is reported as lag.
pub const LAG_REPORT_AFTER: Duration = Duration::from_millis(500);
/// 20 ms of 16 kHz mono silence.
const SILENCE_CHUNK: [i16; 320] = [0; 320];
const TICK_MS: u64 = 20;

/// The gaps a speaker's audio came back from within one report window.
#[derive(Debug, Default)]
struct GapLog {
    count: u32,
    longest_ticks: u64,
    since: Option<Instant>,
}

/// One speaker's recognizer, spotter and timing.
pub struct SpeakerState {
    recognizer: Box<dyn SpeechRecognizer>,
    downsampler: StereoDownsampler,
    spotter: PhraseSpotter,
    heard: Option<String>,
    stream_samples: u64,
    /// When the last frame was processed: only for the idle drop.
    last_audio: Instant,
    /// The receive tick of the last frame.
    last_tick: Option<u64>,
    /// Silence chunks fed since the last frame.
    silence_fed: u64,
    gaps: GapLog,
}

impl SpeakerState {
    pub fn new(recognizer: Box<dyn SpeechRecognizer>, spotter: PhraseSpotter) -> Self {
        Self {
            recognizer,
            downsampler: StereoDownsampler::new(3),
            spotter,
            heard: None,
            stream_samples: 0,
            last_audio: Instant::now(),
            last_tick: None,
            silence_fed: 0,
            gaps: GapLog::default(),
        }
    }

    /// Feeds 16 kHz mono audio; returns the phrases confirmed by it.
    pub fn accept_mono(&mut self, mono: &[i16], phrases: &PhraseSet) -> Vec<Hit> {
        self.stream_samples += mono.len() as u64;
        let Some(recognition) = self.recognizer.accept(mono) else {
            return Vec::new();
        };
        let stream_seconds = self.stream_samples as f64 / super::RECOGNIZER_SAMPLE_RATE as f64;
        if recognition.is_final && !recognition.text.trim().is_empty() {
            self.heard = Some(recognition.text.clone());
        }
        self.spotter.update(&recognition, phrases, stream_seconds)
    }

    /// Feeds one 48 kHz stereo Discord frame of receive tick `tick`. A frame after a short
    /// absence continues the stream with nothing inserted.
    pub fn accept_discord(
        &mut self,
        samples: &[i16],
        phrases: &PhraseSet,
        tick: u64,
        now: Instant,
    ) -> Vec<Hit> {
        if let Some(last) = self.last_tick {
            let missing = tick.saturating_sub(last + 1);
            if self.silence_fed == 0 && missing >= BRIDGED_GAP_TICKS {
                self.gaps.count += 1;
                self.gaps.longest_ticks = self.gaps.longest_ticks.max(missing);
                self.gaps.since.get_or_insert(now);
            }
        }
        self.last_tick = Some(tick);
        self.silence_fed = 0;
        self.last_audio = now;
        let mono = self.downsampler.process(samples);
        self.accept_mono(&mono, phrases)
    }

    /// Receive tick `tick` of the speaker's guild ended: feeds the trailing silence due by
    /// it. Nothing below [`SILENCE_AFTER_TICKS`] of absence; from there one chunk per absent
    /// tick, caught up at once, up to [`MAX_SILENCE_CHUNKS`].
    pub fn on_tick(&mut self, tick: u64, phrases: &PhraseSet) -> Vec<Hit> {
        let mut hits = Vec::new();
        let Some(last) = self.last_tick else {
            return hits;
        };
        let absent = tick.saturating_sub(last);
        if absent < SILENCE_AFTER_TICKS {
            return hits;
        }
        let due = absent.min(MAX_SILENCE_CHUNKS);
        while self.silence_fed < due {
            self.silence_fed += 1;
            hits.extend(self.accept_mono(&SILENCE_CHUNK, phrases));
        }
        hits
    }

    /// Silence chunks fed since the speaker's last frame.
    pub fn silence_fed(&self) -> u64 {
        self.silence_fed
    }

    pub fn take_heard(&mut self) -> Option<String> {
        self.heard.take()
    }

    /// A deactivation phrase the last utterance showed but never confirmed.
    pub fn take_near_miss(&mut self) -> Option<(String, usize)> {
        self.spotter.take_near_miss()
    }

    /// The gaps bridged since the first one of the window, with the longest in ms, once
    /// `interval` has passed since that first gap.
    pub fn take_gap_report(&mut self, now: Instant, interval: Duration) -> Option<(u32, u64)> {
        let since = self.gaps.since?;
        if now.saturating_duration_since(since) < interval {
            return None;
        }
        let report = (self.gaps.count, self.gaps.longest_ticks * TICK_MS);
        self.gaps = GapLog::default();
        Some(report)
    }

    pub fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_audio)
    }
}

/// The phrases whose every word the model knows, and the words it does not. A word counts
/// as known in its lower-case form, its folded form ("всё" or "все") or, written with an
/// apostrophe or a hyphen, piece by piece.
pub fn usable_phrases(model: &dyn LoadedModel, phrases: &[Phrase]) -> (Vec<Phrase>, Vec<String>) {
    let knows = |word: &str| model.knows_word(word);
    let mut usable = Vec::new();
    let mut unknown = Vec::new();
    for phrase in phrases {
        let missing: Vec<String> = phrase
            .text
            .split_whitespace()
            .filter(|token| !word_known(token, &knows))
            .map(|token| {
                token
                    .to_lowercase()
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_owned()
            })
            .collect();
        if missing.is_empty() {
            usable.push(phrase.clone());
        } else {
            for word in missing {
                if !unknown.contains(&word) {
                    unknown.push(word);
                }
            }
        }
    }
    (usable, unknown)
}

fn kind_label(kind: PhraseKind) -> &'static str {
    match kind {
        PhraseKind::Activate => "activation",
        PhraseKind::Deactivate => "deactivation",
    }
}

/// The decision line of a confirmed phrase: what it did and how long after the phrase's
/// end it reached the gate, split into the recognizer's part (stream time from the phrase's
/// end to the result) and the time its audio waited in the queue.
pub fn decision_line(
    guild: u64,
    ssrc: u32,
    kind: PhraseKind,
    phrase: &str,
    outcome: PhraseOutcome,
    recognizing: f64,
    queued: Duration,
) -> String {
    let queued = queued.as_secs_f64();
    format!(
        "[wake] {guild}/{ssrc}: {} «{phrase}» {}, {:.2} s after the phrase ended ({recognizing:.2} s recognizing, {queued:.2} s queued)",
        kind_label(kind),
        outcome.label(),
        recognizing + queued
    )
}

/// A deactivation phrase seen in intermediate results that the utterance never confirmed.
pub fn near_miss_line(guild: u64, ssrc: u32, phrase: &str, partials: usize) -> String {
    format!(
        "[wake] {guild}/{ssrc}: deactivation «{phrase}» was in {partials} partial results but was not confirmed"
    )
}

/// The gaps a speaker's audio came back from within the report window.
pub fn gaps_line(guild: u64, ssrc: u32, count: u32, longest_ms: u64) -> String {
    format!(
        "[wake] {guild}/{ssrc}: {count} {} in the audio bridged in the last 10 s, longest {longest_ms} ms",
        if count == 1 { "gap" } else { "gaps" }
    )
}

pub fn lag_line(lag: Duration) -> String {
    format!(
        "[wake] the recognizer runs {} ms behind the audio",
        lag.as_millis()
    )
}

/// Reports processing lag above [`LAG_REPORT_AFTER`] at most once per interval.
#[derive(Debug)]
pub struct LagReport {
    interval: Duration,
    last: Option<Instant>,
}

impl LagReport {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// The lag line for a message processed `lag` after the gate fed it, if one is due.
    pub fn check(&mut self, lag: Duration, now: Instant) -> Option<String> {
        if lag <= LAG_REPORT_AFTER {
            return None;
        }
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < self.interval)
        {
            return None;
        }
        self.last = Some(now);
        Some(lag_line(lag))
    }
}

pub struct Worker {
    model: Box<dyn LoadedModel>,
    phrases: PhraseSet,
    sink: Arc<dyn PhraseSink>,
    events: EventBus,
    dropped: Arc<AtomicU64>,
    manager: WakeManager,
    report_interval: Duration,
}

impl Worker {
    pub(crate) fn new(
        model: Box<dyn LoadedModel>,
        phrases: PhraseSet,
        sink: Arc<dyn PhraseSink>,
        events: EventBus,
        dropped: Arc<AtomicU64>,
        manager: WakeManager,
    ) -> Self {
        Self {
            model,
            phrases,
            sink,
            events,
            dropped,
            manager,
            report_interval: REPORT_INTERVAL,
        }
    }

    /// Checks the vocabulary and reports it; returns the usable phrase set.
    fn configure(&self, phrases: &PhraseSet) -> PhraseSet {
        let (activation, mut unknown) = usable_phrases(self.model.as_ref(), &phrases.activation);
        let (deactivation, unknown_deactivation) =
            usable_phrases(self.model.as_ref(), &phrases.deactivation);
        for word in unknown_deactivation {
            if !unknown.contains(&word) {
                unknown.push(word);
            }
        }
        let mut message = format!(
            "wake word ready; activation: {}; deactivation: {}",
            if activation.is_empty() {
                "none".to_owned()
            } else {
                Phrase::texts(&activation).join(", ")
            },
            if deactivation.is_empty() {
                "none (sessions end on the idle timeout)".to_owned()
            } else {
                Phrase::texts(&deactivation).join(", ")
            }
        );
        if !unknown.is_empty() {
            message.push_str(&format!(
                "; not in the model vocabulary (ignored): {}",
                unknown.join(", ")
            ));
        }
        self.events.log(message);
        self.manager.set_unknown_words(unknown);
        PhraseSet {
            activation,
            deactivation,
        }
    }

    fn dispatch(&self, guild: u64, ssrc: u32, hit: Hit, queued: Duration) {
        let (kind, phrase, since_end) = hit;
        let recognizing = since_end.max(0.0);
        let outcome = self.sink.on_phrase(
            guild,
            kind,
            &phrase,
            ssrc,
            Duration::from_secs_f64(recognizing) + queued,
        );
        log::info!(
            "{}",
            decision_line(guild, ssrc, kind, &phrase, outcome, recognizing, queued)
        );
    }

    /// What the recognizer heard (a detail line only) and a near miss, after each result.
    fn after_recognition(guild: u64, ssrc: u32, state: &mut SpeakerState) {
        if let Some(text) = state.take_heard() {
            crate::detail!("wake", "[wake] {guild}/{ssrc} heard: {text}");
        }
        if let Some((phrase, partials)) = state.take_near_miss() {
            log::info!("{}", near_miss_line(guild, ssrc, &phrase, partials));
        }
    }

    pub(crate) fn run(self, receiver: Receiver<WakeMessage>) {
        let mut usable = self.configure(&self.phrases);
        let mut speakers: HashMap<(u64, u32), SpeakerState> = HashMap::new();
        let mut lag = LagReport::new(self.report_interval);
        let mut last_drop_report = Instant::now();
        loop {
            match receiver.recv_timeout(WORKER_TICK) {
                Ok(WakeMessage::Audio {
                    guild,
                    ssrc,
                    tick,
                    at,
                    samples,
                }) => {
                    let now = Instant::now();
                    let queued = now.saturating_duration_since(at);
                    if let Some(line) = lag.check(queued, now) {
                        log::info!("{line}");
                    }
                    if usable.is_empty() {
                        continue;
                    }
                    let state = match speakers.entry((guild, ssrc)) {
                        std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            match self.model.new_recognizer() {
                                Ok(recognizer) => entry
                                    .insert(SpeakerState::new(recognizer, PhraseSpotter::new())),
                                Err(error) => {
                                    log::error!("[wake] recognizer for {guild}/{ssrc}: {error}");
                                    continue;
                                }
                            }
                        }
                    };
                    for hit in state.accept_discord(&samples, &usable, tick, now) {
                        self.dispatch(guild, ssrc, hit, queued);
                    }
                    Self::after_recognition(guild, ssrc, state);
                }
                Ok(WakeMessage::Tick { guild, tick, at }) => {
                    let now = Instant::now();
                    let queued = now.saturating_duration_since(at);
                    for ((speaker_guild, ssrc), state) in speakers.iter_mut() {
                        if *speaker_guild != guild {
                            continue;
                        }
                        for hit in state.on_tick(tick, &usable) {
                            self.dispatch(guild, *ssrc, hit, queued);
                        }
                        Self::after_recognition(guild, *ssrc, state);
                    }
                }
                Ok(WakeMessage::SetPhrases(phrases)) => {
                    speakers.clear();
                    usable = self.configure(&phrases);
                }
                Ok(WakeMessage::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            for ((guild, ssrc), state) in speakers.iter_mut() {
                if let Some((count, longest)) = state.take_gap_report(now, self.report_interval) {
                    log::info!("{}", gaps_line(*guild, *ssrc, count, longest));
                }
            }
            if now.saturating_duration_since(last_drop_report) >= self.report_interval {
                last_drop_report = now;
                let dropped = self.dropped.swap(0, Ordering::Relaxed);
                if dropped > 0 {
                    self.events.log(format!(
                        "the recognizer fell behind and skipped {:.1} s of speech; phrases said then can be missed",
                        dropped as f64 * 0.02
                    ));
                }
            }
            speakers.retain(|_, state| state.idle_for(now) < SPEAKER_IDLE_TIMEOUT);
        }
        drop(speakers);
        self.events.log("wake word engine stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::{Recognition, WakeError};
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::sync_channel;
    use std::sync::Mutex;

    /// Counts what a recognizer was fed: every chunk, and the all-zero ones (silence).
    #[derive(Default)]
    struct Fed {
        chunks: AtomicUsize,
        zero_chunks: AtomicUsize,
    }

    /// Returns a scripted sequence of recognitions, one per `accept`.
    struct Scripted {
        results: Arc<Mutex<VecDeque<Recognition>>>,
        fed: Arc<Fed>,
    }

    impl SpeechRecognizer for Scripted {
        fn accept(&mut self, pcm: &[i16]) -> Option<Recognition> {
            self.fed.chunks.fetch_add(1, Ordering::Relaxed);
            if pcm.iter().all(|sample| *sample == 0) {
                self.fed.zero_chunks.fetch_add(1, Ordering::Relaxed);
            }
            self.results.lock().unwrap().pop_front()
        }
    }

    fn scripted(results: Vec<Recognition>) -> (Box<Scripted>, Arc<Fed>) {
        let fed = Arc::new(Fed::default());
        (
            Box::new(Scripted {
                results: Arc::new(Mutex::new(results.into())),
                fed: Arc::clone(&fed),
            }),
            fed,
        )
    }

    fn partial(text: &str) -> Recognition {
        Recognition {
            is_final: false,
            text: text.to_owned(),
            words: Vec::new(),
        }
    }

    fn final_result(text: &str) -> Recognition {
        Recognition {
            is_final: true,
            text: text.to_owned(),
            words: Vec::new(),
        }
    }

    /// One 48 kHz stereo frame with a signal (never all zeros after downsampling).
    fn voiced() -> Vec<i16> {
        vec![1_000i16; 1920]
    }

    fn set(activation: &[&str], deactivation: &[&str]) -> PhraseSet {
        let owned =
            |list: &[&str]| -> Vec<String> { list.iter().map(|p| (*p).to_owned()).collect() };
        PhraseSet::new(&owned(activation), &owned(deactivation))
    }

    /// Task 2.2: an absence below 15 ticks feeds nothing, and the frame follows directly.
    #[test]
    fn an_absence_of_fourteen_ticks_feeds_nothing() {
        let (recognizer, fed) = scripted(Vec::new());
        let mut speaker = SpeakerState::new(recognizer, PhraseSpotter::new());
        let now = Instant::now();
        speaker.accept_discord(&voiced(), &PhraseSet::default(), 1, now);
        for tick in 2..=15 {
            assert!(speaker.on_tick(tick, &PhraseSet::default()).is_empty());
        }
        assert_eq!(speaker.silence_fed(), 0, "14 absent ticks feed nothing");
        speaker.accept_discord(&voiced(), &PhraseSet::default(), 16, now);
        assert_eq!(fed.chunks.load(Ordering::Relaxed), 2);
        assert_eq!(
            fed.zero_chunks.load(Ordering::Relaxed),
            0,
            "the frame follows directly"
        );
    }

    /// Task 2.2: 15 absent ticks feed 15 chunks at once, then one per tick, capped at 75.
    #[test]
    fn fifteen_absent_ticks_feed_fifteen_chunks_then_one_per_tick_up_to_75() {
        let (recognizer, fed) = scripted(Vec::new());
        let mut speaker = SpeakerState::new(recognizer, PhraseSpotter::new());
        speaker.accept_discord(&voiced(), &PhraseSet::default(), 1, Instant::now());
        speaker.on_tick(1, &PhraseSet::default());
        speaker.on_tick(16, &PhraseSet::default());
        assert_eq!(speaker.silence_fed(), 15);
        speaker.on_tick(17, &PhraseSet::default());
        assert_eq!(speaker.silence_fed(), 16);
        for tick in 18..=300 {
            speaker.on_tick(tick, &PhraseSet::default());
        }
        assert_eq!(speaker.silence_fed(), MAX_SILENCE_CHUNKS, "capped at 1.5 s");
        assert_eq!(fed.zero_chunks.load(Ordering::Relaxed), 75);
    }

    /// Task 2.2: a dropped `Tick` is caught up at the next one.
    #[test]
    fn a_dropped_tick_is_caught_up_at_the_next_one() {
        let (recognizer, _fed) = scripted(Vec::new());
        let mut speaker = SpeakerState::new(recognizer, PhraseSpotter::new());
        speaker.accept_discord(&voiced(), &PhraseSet::default(), 1, Instant::now());
        for tick in 2..=15 {
            speaker.on_tick(tick, &PhraseSet::default());
        }
        // Tick 16 was lost to a full queue.
        speaker.on_tick(17, &PhraseSet::default());
        assert_eq!(speaker.silence_fed(), 16);
    }

    /// Task 2.2: the phrase at the end of a sentence, confirmed by the final result that
    /// the trailing silence brings, on ticks.
    #[test]
    fn a_final_result_from_silence_confirms_the_phrase_at_the_end_of_a_sentence() {
        let mut results = vec![partial("so long dismiss"); 4];
        results.push(final_result("so long dismiss"));
        let (recognizer, _fed) = scripted(results);
        let mut speaker = SpeakerState::new(recognizer, PhraseSpotter::new());
        let phrases = set(&["luna"], &["dismiss"]);
        let hits = speaker.accept_discord(&voiced(), &phrases, 1, Instant::now());
        assert!(hits.is_empty(), "one partial is not enough");
        assert!(speaker.on_tick(14, &phrases).is_empty(), "no silence yet");
        let hits = speaker.on_tick(16, &phrases);
        assert_eq!(
            hits.len(),
            1,
            "the final result fed by silence confirms it: {hits:?}"
        );
        assert_eq!(hits[0].0, PhraseKind::Deactivate);
        assert_eq!(speaker.take_heard().as_deref(), Some("so long dismiss"));
    }

    /// Task 6.3: gaps of 6 and 3 ticks within 10 s give one line with 2 gaps and 120 ms;
    /// a gap of 2 ticks is not counted, and a stop followed by silence is not a gap.
    #[test]
    fn bridged_gaps_are_reported_once_per_window() {
        let (recognizer, _fed) = scripted(Vec::new());
        let mut speaker = SpeakerState::new(recognizer, PhraseSpotter::new());
        let start = Instant::now();
        let none = PhraseSet::default();
        speaker.accept_discord(&voiced(), &none, 10, start);
        speaker.accept_discord(&voiced(), &none, 17, start); // 6 ticks missing
        speaker.accept_discord(&voiced(), &none, 20, start + Duration::from_secs(1)); // 2
        speaker.accept_discord(&voiced(), &none, 24, start + Duration::from_secs(2)); // 3
        assert_eq!(
            speaker.take_gap_report(start + Duration::from_secs(5), REPORT_INTERVAL),
            None
        );
        assert_eq!(
            speaker.take_gap_report(start + Duration::from_secs(10), REPORT_INTERVAL),
            Some((2, 120))
        );
        assert_eq!(
            speaker.take_gap_report(start + Duration::from_secs(30), REPORT_INTERVAL),
            None,
            "nothing new since"
        );
        speaker.on_tick(24 + SILENCE_AFTER_TICKS, &none);
        speaker.accept_discord(&voiced(), &none, 60, start + Duration::from_secs(31));
        assert_eq!(
            speaker.take_gap_report(start + Duration::from_secs(60), REPORT_INTERVAL),
            None,
            "audio after the trailing silence is a new utterance, not a bridged gap"
        );
        assert_eq!(
            gaps_line(5, 7, 2, 120),
            "[wake] 5/7: 2 gaps in the audio bridged in the last 10 s, longest 120 ms"
        );
    }

    /// Task 6.3: a message 700 ms old gives a lag line; a second one 3 s later gives none.
    #[test]
    fn lag_is_reported_at_most_once_per_window() {
        let mut lag = LagReport::new(REPORT_INTERVAL);
        let now = Instant::now();
        assert_eq!(lag.check(Duration::from_millis(400), now), None);
        assert_eq!(
            lag.check(Duration::from_millis(700), now).as_deref(),
            Some("[wake] the recognizer runs 700 ms behind the audio")
        );
        assert_eq!(
            lag.check(Duration::from_millis(900), now + Duration::from_secs(3)),
            None
        );
        assert!(lag
            .check(Duration::from_millis(900), now + Duration::from_secs(11))
            .is_some());
    }

    /// Task 6.1: the line builders with known values.
    #[test]
    fn the_decision_and_near_miss_lines_carry_the_phrase_and_the_numbers() {
        assert_eq!(
            decision_line(
                5,
                7,
                PhraseKind::Deactivate,
                "отбой",
                PhraseOutcome::Applied,
                0.15,
                Duration::from_millis(80)
            ),
            "[wake] 5/7: deactivation «отбой» applied, 0.23 s after the phrase ended (0.15 s recognizing, 0.08 s queued)"
        );
        assert_eq!(
            decision_line(
                5,
                7,
                PhraseKind::Activate,
                "bro",
                PhraseOutcome::RightAfterDeactivation,
                0.4,
                Duration::ZERO
            ),
            "[wake] 5/7: activation «bro» ignored right after a deactivation, 0.40 s after the phrase ended (0.40 s recognizing, 0.00 s queued)"
        );
        assert_eq!(
            near_miss_line(5, 7, "отбой", 3),
            "[wake] 5/7: deactivation «отбой» was in 3 partial results but was not confirmed"
        );
    }

    /// A model whose recognizers share one script and one feed counter.
    struct FakeModel {
        results: Arc<Mutex<VecDeque<Recognition>>>,
        fed: Arc<Fed>,
    }

    impl LoadedModel for FakeModel {
        fn new_recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, WakeError> {
            Ok(Box::new(Scripted {
                results: Arc::clone(&self.results),
                fed: Arc::clone(&self.fed),
            }))
        }
        fn knows_word(&self, word: &str) -> Option<bool> {
            Some(word != "zyx" && word != "всё")
        }
    }

    /// Records the phrases the worker hands over.
    #[derive(Default)]
    struct Phrases(Mutex<Vec<(u64, PhraseKind, String)>>);

    impl PhraseSink for Phrases {
        fn on_phrase(
            &self,
            guild: u64,
            kind: PhraseKind,
            phrase: &str,
            _: u32,
            _: Duration,
        ) -> PhraseOutcome {
            self.0
                .lock()
                .unwrap()
                .push((guild, kind, phrase.to_owned()));
            PhraseOutcome::Applied
        }
    }

    /// Runs a worker over `messages` on this thread, `wait` after they were queued.
    fn run_worker(
        messages: Vec<WakeMessage>,
        wait: Duration,
        results: Vec<Recognition>,
        phrases: PhraseSet,
    ) -> (Arc<Fed>, Arc<Phrases>) {
        let dir = tempfile::tempdir().unwrap();
        let fed = Arc::new(Fed::default());
        let sink = Arc::new(Phrases::default());
        let (sender, receiver) = sync_channel(4_096);
        for message in messages {
            sender.send(message).unwrap();
        }
        sender.send(WakeMessage::Stop).unwrap();
        std::thread::sleep(wait);
        let manager = WakeManager::new(
            Vec::new(),
            dir.path().to_path_buf(),
            dir.path().join("models"),
            EventBus::new(64),
            sink.clone(),
        );
        let mut worker = Worker::new(
            Box::new(FakeModel {
                results: Arc::new(Mutex::new(results.into())),
                fed: Arc::clone(&fed),
            }),
            phrases,
            sink.clone(),
            EventBus::new(64),
            Arc::new(AtomicU64::new(0)),
            manager,
        );
        worker.report_interval = Duration::ZERO;
        worker.run(receiver);
        (fed, sink)
    }

    fn audio(guild: u64, ssrc: u32, tick: u64, at: Instant) -> WakeMessage {
        WakeMessage::Audio {
            guild,
            ssrc,
            tick,
            at,
            samples: voiced(),
        }
    }

    /// Task 2.2, scenario "The engine runs behind": four speakers whose messages are
    /// processed 300 ms after their ticks, each with a frame in every tick, get no silence.
    #[test]
    fn four_speakers_processed_late_get_no_silence() {
        let at = Instant::now();
        let mut messages = Vec::new();
        for tick in 1..=50 {
            for ssrc in 1..=4 {
                messages.push(audio(9, ssrc, tick, at));
            }
            messages.push(WakeMessage::Tick { guild: 9, tick, at });
        }
        let (fed, _) = run_worker(
            messages,
            Duration::from_millis(300),
            Vec::new(),
            set(&["bro"], &[]),
        );
        assert_eq!(fed.chunks.load(Ordering::Relaxed), 200);
        assert_eq!(
            fed.zero_chunks.load(Ordering::Relaxed),
            0,
            "no silence inside speech"
        );
    }

    #[test]
    fn ticks_of_another_guild_feed_no_silence() {
        let at = Instant::now();
        let mut messages = vec![audio(1, 7, 1, at)];
        for tick in 2..=40 {
            messages.push(WakeMessage::Tick { guild: 2, tick, at });
        }
        let (fed, _) = run_worker(messages, Duration::ZERO, Vec::new(), set(&["bro"], &[]));
        assert_eq!(fed.zero_chunks.load(Ordering::Relaxed), 0);
    }

    /// Task 6.4: the decision, near-miss, gap and lag lines hold only the phrase and
    /// numbers; the recognized text appears only in the `heard:` detail line.
    #[test]
    fn recognized_speech_stays_in_detail_lines() {
        crate::test_log::install();
        let guild = 918_273_645_u64;
        let marker = format!("{guild}/");
        let old = Instant::now()
            .checked_sub(Duration::from_millis(700))
            .unwrap_or_else(Instant::now);
        let results = vec![
            partial("secretword dismiss"),
            partial("secretword dismiss"),
            partial("secretword dismiss"),
            final_result("secretword this miss"),
            final_result("secretword dismiss"),
        ];
        let messages = vec![
            audio(guild, 3, 1, old),
            audio(guild, 3, 2, Instant::now()),
            audio(guild, 3, 3, Instant::now()),
            audio(guild, 3, 4, Instant::now()),
            // 6 ticks missing: a bridged gap.
            audio(guild, 3, 11, Instant::now()),
        ];
        let (_, sink) = run_worker(
            messages,
            Duration::ZERO,
            results,
            set(&["luna"], &["dismiss"]),
        );
        assert_eq!(
            *sink.0.lock().unwrap(),
            vec![(guild, PhraseKind::Deactivate, "dismiss".to_owned())]
        );
        let lines = crate::test_log::containing(&marker);
        let info: Vec<&String> = lines
            .iter()
            .filter(|(target, _, _)| !target.starts_with(crate::DETAIL_TARGET_PREFIX))
            .map(|(_, _, text)| text)
            .collect();
        assert!(
            info.iter()
                .any(|line| line.contains("was in 3 partial results")),
            "{info:?}"
        );
        assert!(info.iter().any(|line| line.contains("applied")), "{info:?}");
        assert!(info.iter().any(|line| line.contains("bridged")), "{info:?}");
        assert!(
            info.iter().all(|line| !line.contains("secretword")),
            "recognized text in an info line: {info:?}"
        );
        let heard: Vec<&(String, log::Level, String)> = lines
            .iter()
            .filter(|(_, _, text)| text.contains("heard:"))
            .collect();
        assert!(!heard.is_empty());
        assert!(heard
            .iter()
            .all(|(target, _, text)| target == "detail::wake" && text.contains("secretword")));
        let lag = crate::test_log::containing("ms behind the audio");
        assert!(!lag.is_empty(), "a 700 ms old message gives a lag line");
    }

    #[test]
    fn usable_phrases_drop_those_with_unknown_words_and_list_the_words() {
        let model = FakeModel {
            results: Arc::default(),
            fed: Arc::default(),
        };
        let phrases = Phrase::list(&[
            "hey luna".to_owned(),
            "Zyx now".to_owned(),
            "dismiss".to_owned(),
            "Братан всё".to_owned(),
        ]);
        let (usable, unknown) = usable_phrases(&model, &phrases);
        assert_eq!(
            Phrase::texts(&usable),
            vec!["hey luna", "dismiss", "Братан всё"],
            "«всё» is known through «все»"
        );
        assert_eq!(unknown, vec!["zyx"]);
    }
}
