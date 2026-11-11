//! The wake worker thread: one recognizer per speaker, trailing silence, the reactivation
//! guard, and the log lines that diagnose most wake-word reports.

use super::spotter::{Hit, PhraseSpotter};
use super::{
    LoadedModel, PhraseKind, PhraseSet, PhraseSink, SpeechRecognizer, WakeManager, WakeMessage,
};
use crate::audio::dsp::StereoDownsampler;
use crate::events::EventBus;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// After a deactivation, activation phrases are ignored this long, so the end of a goodbye
/// heard by another member's microphone does not start a new session.
pub const REACTIVATION_GUARD: Duration = Duration::from_millis(1_500);
/// Discord stops sending a speaker's audio when they stop talking; silence is fed for this
/// long afterwards so a phrase at the end of a sentence settles.
pub const TRAILING_SILENCE: Duration = Duration::from_millis(1_500);
/// Silence feeding starts once a speaker has sent nothing for this long.
pub const SILENCE_AFTER: Duration = Duration::from_millis(60);
pub const WORKER_TICK: Duration = Duration::from_millis(20);
pub const SPEAKER_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const DROP_REPORT_INTERVAL: Duration = Duration::from_secs(10);
/// 20 ms of 16 kHz mono silence.
const SILENCE_CHUNK: [i16; 320] = [0; 320];

/// One speaker's recognizer, spotter and timing.
pub struct SpeakerState {
    recognizer: Box<dyn SpeechRecognizer>,
    downsampler: StereoDownsampler,
    spotter: PhraseSpotter,
    heard: Option<String>,
    stream_samples: u64,
    last_audio: Instant,
    silence_fed: Duration,
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
            silence_fed: Duration::ZERO,
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

    /// Feeds 48 kHz stereo Discord audio.
    pub fn accept_discord(
        &mut self,
        samples: &[i16],
        phrases: &PhraseSet,
        now: Instant,
    ) -> Vec<Hit> {
        self.last_audio = now;
        self.silence_fed = Duration::ZERO;
        let mono = self.downsampler.process(samples);
        self.accept_mono(&mono, phrases)
    }

    /// Feeds the trailing silence that is due by `now`.
    pub fn feed_trailing_silence(&mut self, now: Instant, phrases: &PhraseSet) -> Vec<Hit> {
        let mut hits = Vec::new();
        let quiet_for = now.saturating_duration_since(self.last_audio);
        if quiet_for < SILENCE_AFTER {
            return hits;
        }
        let due = quiet_for.min(TRAILING_SILENCE);
        while self.silence_fed + WORKER_TICK <= due {
            self.silence_fed += WORKER_TICK;
            hits.extend(self.accept_mono(&SILENCE_CHUNK, phrases));
        }
        hits
    }

    pub fn take_heard(&mut self) -> Option<String> {
        self.heard.take()
    }

    pub fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_audio)
    }
}

/// The phrases whose every word the model knows, and the words it does not.
pub fn usable_phrases(model: &dyn LoadedModel, phrases: &[String]) -> (Vec<String>, Vec<String>) {
    let mut usable = Vec::new();
    let mut unknown = Vec::new();
    for phrase in phrases {
        let missing: Vec<String> = phrase
            .split_whitespace()
            .filter(|word| model.knows_word(word) == Some(false))
            .map(str::to_owned)
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

pub struct Worker {
    model: Box<dyn LoadedModel>,
    phrases: PhraseSet,
    sink: Arc<dyn PhraseSink>,
    events: EventBus,
    dropped: Arc<AtomicU64>,
    manager: WakeManager,
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
                activation.join(", ")
            },
            if deactivation.is_empty() {
                "none (sessions end on the idle timeout)".to_owned()
            } else {
                deactivation.join(", ")
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

    fn dispatch(&self, quiet_until: &mut HashMap<u64, Instant>, guild: u64, ssrc: u32, hit: Hit) {
        let (kind, phrase, since_end) = hit;
        let now = Instant::now();
        let label = match kind {
            PhraseKind::Activate => "activation",
            PhraseKind::Deactivate => "deactivation",
        };
        if kind == PhraseKind::Activate && quiet_until.get(&guild).is_some_and(|until| now < *until)
        {
            log::info!(
                "[wake] {guild}/{ssrc}: {label} «{phrase}» ignored right after a deactivation"
            );
            return;
        }
        let applied = self.sink.on_phrase(
            guild,
            kind,
            &phrase,
            ssrc,
            Duration::from_secs_f64(since_end.max(0.0)),
        );
        log::info!(
            "[wake] {guild}/{ssrc}: {label} «{phrase}» {}",
            if applied {
                "applied"
            } else {
                "ignored (the session is already in that state)"
            }
        );
        if applied && kind == PhraseKind::Deactivate {
            quiet_until.insert(guild, now + REACTIVATION_GUARD);
        }
    }

    pub(crate) fn run(self, receiver: Receiver<WakeMessage>) {
        let mut usable = self.configure(&self.phrases);
        let mut speakers: HashMap<(u64, u32), SpeakerState> = HashMap::new();
        let mut quiet_until: HashMap<u64, Instant> = HashMap::new();
        let mut last_drop_report = Instant::now();
        loop {
            match receiver.recv_timeout(WORKER_TICK) {
                Ok(WakeMessage::Audio {
                    guild,
                    ssrc,
                    samples,
                }) => {
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
                    let now = Instant::now();
                    for hit in state.accept_discord(&samples, &usable, now) {
                        self.dispatch(&mut quiet_until, guild, ssrc, hit);
                    }
                    if let Some(text) = state.take_heard() {
                        crate::detail!("wake", "[wake] {guild}/{ssrc} heard: {text}");
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
                for hit in state.feed_trailing_silence(now, &usable) {
                    self.dispatch(&mut quiet_until, *guild, *ssrc, hit);
                }
                if let Some(text) = state.take_heard() {
                    crate::detail!("wake", "[wake] {guild}/{ssrc} heard: {text}");
                }
            }
            if now.saturating_duration_since(last_drop_report) >= DROP_REPORT_INTERVAL {
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
    use crate::wake::Recognition;

    /// Returns a scripted sequence of recognitions, one per `accept`.
    struct Scripted {
        results: std::collections::VecDeque<Recognition>,
        fed: usize,
    }

    impl SpeechRecognizer for Scripted {
        fn accept(&mut self, pcm: &[i16]) -> Option<Recognition> {
            self.fed += pcm.len();
            self.results.pop_front()
        }
    }

    fn partial(text: &str) -> Recognition {
        Recognition {
            is_final: false,
            text: text.to_owned(),
            words: Vec::new(),
        }
    }

    #[test]
    fn trailing_silence_is_fed_after_sixty_ms_up_to_one_and_a_half_seconds() {
        let recognizer = Scripted {
            results: std::iter::repeat_n(partial(""), 200).collect(),
            fed: 0,
        };
        let mut speaker = SpeakerState::new(Box::new(recognizer), PhraseSpotter::new());
        let start = Instant::now();
        let frame = vec![0i16; 1920];
        speaker.accept_discord(&frame, &PhraseSet::default(), start);
        assert!(speaker
            .feed_trailing_silence(start + Duration::from_millis(40), &PhraseSet::default())
            .is_empty());
        speaker.feed_trailing_silence(start + Duration::from_millis(500), &PhraseSet::default());
        assert_eq!(speaker.silence_fed, Duration::from_millis(500));
        speaker.feed_trailing_silence(start + Duration::from_secs(5), &PhraseSet::default());
        assert_eq!(speaker.silence_fed, TRAILING_SILENCE, "capped at 1.5 s");
        assert_eq!(
            speaker.idle_for(start + Duration::from_secs(5)),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn a_final_result_from_silence_confirms_the_phrase_at_the_end_of_a_sentence() {
        let mut results = std::collections::VecDeque::new();
        results.push_back(partial("so long dismiss"));
        for _ in 0..3 {
            results.push_back(partial("so long dismiss"));
        }
        results.push_back(Recognition {
            is_final: true,
            text: "so long dismiss".to_owned(),
            words: Vec::new(),
        });
        let mut speaker =
            SpeakerState::new(Box::new(Scripted { results, fed: 0 }), PhraseSpotter::new());
        let phrases = PhraseSet {
            activation: vec!["luna".to_owned()],
            deactivation: vec!["dismiss".to_owned()],
        };
        let start = Instant::now();
        let hits = speaker.accept_discord(&vec![0i16; 1920], &phrases, start);
        assert!(hits.is_empty(), "one partial is not enough");
        let hits = speaker.feed_trailing_silence(start + Duration::from_millis(200), &phrases);
        assert_eq!(
            hits.len(),
            1,
            "the final result fed by silence confirms it: {hits:?}"
        );
        assert_eq!(hits[0].0, PhraseKind::Deactivate);
        assert_eq!(speaker.take_heard().as_deref(), Some("so long dismiss"));
    }

    struct FakeModel;
    impl LoadedModel for FakeModel {
        fn new_recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, crate::wake::WakeError> {
            Ok(Box::new(Scripted {
                results: Default::default(),
                fed: 0,
            }))
        }
        fn knows_word(&self, word: &str) -> Option<bool> {
            Some(word != "zyx")
        }
    }

    #[test]
    fn usable_phrases_drop_those_with_unknown_words_and_list_the_words() {
        let (usable, unknown) = usable_phrases(
            &FakeModel,
            &[
                "hey luna".to_owned(),
                "zyx now".to_owned(),
                "dismiss".to_owned(),
            ],
        );
        assert_eq!(usable, vec!["hey luna", "dismiss"]);
        assert_eq!(unknown, vec!["zyx"]);
    }
}
