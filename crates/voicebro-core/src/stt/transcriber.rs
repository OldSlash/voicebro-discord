//! The transcriber thread of the Whisper mode (change `whisper-mode`, designs D2, D3 and
//! D5): a bounded queue of finished utterances, one model, and the guards against text
//! Whisper invents on noise. The model sits in a slot the manager swaps during a background
//! reload; the old one keeps serving until the new one is in.

use super::cutter::Utterance;
use super::{DecodeOptions, RawTranscript, SpeechToText};
use crate::settings::phrase_words;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

/// Utterances waiting for the transcriber; the oldest waiting is dropped beyond it.
pub const QUEUE_CAPACITY: usize = 8;
/// A word repeated this many times in a row is a hallucination.
pub const REPEATED_WORDS: usize = 4;

/// The model the transcriber uses, swapped by the manager.
pub type ModelSlot = Arc<Mutex<Option<Box<dyn SpeechToText>>>>;

/// What one utterance gave once transcribed.
#[derive(Debug, Clone)]
pub struct Transcript {
    pub guild: u64,
    pub ssrc: u32,
    pub text: String,
    /// The language Whisper used or detected (ISO 639-1).
    pub language: Option<String>,
    /// When the gate handed over the utterance's last frame.
    pub speech_end: Instant,
    /// When the text was ready.
    pub text_at: Instant,
    pub audio_seconds: f64,
    pub transcribe_ms: u64,
    /// The model and the device in use, for the latency line (set by the manager).
    pub model: String,
    pub device: String,
}

/// Receives the transcripts: the gate, through the registry.
pub trait TranscriptSink: Send + Sync {
    fn on_transcript(&self, transcript: Transcript);
}

/// The transcriber's counters, as `Status.whisper` shows them (design D15).
#[derive(Debug, Default)]
pub struct Counters {
    pub queued: AtomicU64,
    pub transcribed: AtomicU64,
    pub dropped_late: AtomicU64,
    pub dropped_noise: AtomicU64,
    pub dropped_hallucination: AtomicU64,
    pub last_transcribe_ms: AtomicU64,
    /// The last utterance's speech, in milliseconds.
    pub last_audio_ms: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct CountersSnapshot {
    /// Waiting now.
    pub queued: u64,
    pub transcribed: u64,
    pub dropped_late: u64,
    pub dropped_noise: u64,
    pub dropped_hallucination: u64,
    pub last_transcribe_ms: Option<u64>,
    pub last_audio_seconds: Option<f64>,
}

impl Counters {
    pub fn snapshot(&self) -> CountersSnapshot {
        let last_transcribe_ms = self.last_transcribe_ms.load(Ordering::Relaxed);
        let last_audio_ms = self.last_audio_ms.load(Ordering::Relaxed);
        CountersSnapshot {
            queued: self.queued.load(Ordering::Relaxed),
            transcribed: self.transcribed.load(Ordering::Relaxed),
            dropped_late: self.dropped_late.load(Ordering::Relaxed),
            dropped_noise: self.dropped_noise.load(Ordering::Relaxed),
            dropped_hallucination: self.dropped_hallucination.load(Ordering::Relaxed),
            last_transcribe_ms: (self.transcribed.load(Ordering::Relaxed) > 0)
                .then_some(last_transcribe_ms),
            last_audio_seconds: (last_audio_ms > 0).then(|| last_audio_ms as f64 / 1000.0),
        }
    }
}

/// The bounded queue between the cutter and the transcriber.
#[derive(Default)]
pub struct UtteranceQueue {
    state: Mutex<(VecDeque<Utterance>, bool)>,
    ready: Condvar,
}

impl UtteranceQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Adds an utterance; returns the oldest waiting one when the queue was full.
    pub fn push(&self, utterance: Utterance) -> Option<Utterance> {
        let mut state = self.state.lock().expect("utterance queue lock");
        let dropped = (state.0.len() >= QUEUE_CAPACITY)
            .then(|| state.0.pop_front())
            .flatten();
        state.0.push_back(utterance);
        self.ready.notify_one();
        dropped
    }

    /// The next utterance; `None` once the queue was closed.
    pub fn pop(&self) -> Option<Utterance> {
        let mut state = self.state.lock().expect("utterance queue lock");
        loop {
            if state.1 {
                return None;
            }
            if let Some(utterance) = state.0.pop_front() {
                return Some(utterance);
            }
            state = self.ready.wait(state).expect("utterance queue wait");
        }
    }

    pub fn len(&self) -> usize {
        self.state.lock().expect("utterance queue lock").0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops what waits for `guild` (the bot left, or the session was muted).
    pub fn clear_guild(&self, guild: u64) {
        self.state
            .lock()
            .expect("utterance queue lock")
            .0
            .retain(|utterance| utterance.guild != guild);
    }

    /// Wakes the thread and makes it end.
    pub fn close(&self) {
        let mut state = self.state.lock().expect("utterance queue lock");
        state.1 = true;
        state.0.clear();
        self.ready.notify_all();
    }
}

/// What the guards made of a transcription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cleaned {
    Text(String),
    /// Nothing left once the no-speech segments were dropped.
    NoSpeech,
    /// One word four or more times in a row.
    Hallucination(String),
}

/// The guards of design D5 on a raw transcription: segments above the no-speech threshold
/// and segments that are only a bracketed note ("[BLANK_AUDIO]", "(music)") are dropped,
/// and text that repeats one word [`REPEATED_WORDS`] times in a row is a hallucination.
pub fn clean(raw: &RawTranscript, no_speech_threshold: f32) -> Cleaned {
    let text = raw
        .segments
        .iter()
        .filter(|segment| segment.no_speech_prob <= no_speech_threshold)
        .map(|segment| segment.text.trim())
        .filter(|text| {
            !text.is_empty()
                && !((text.starts_with('[') && text.ends_with(']'))
                    || (text.starts_with('(') && text.ends_with(')'))
                    || (text.starts_with('*') && text.ends_with('*')))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let words = phrase_words(&text);
    if words.is_empty() {
        return Cleaned::NoSpeech;
    }
    let mut run = 1;
    for pair in words.windows(2) {
        run = if pair[0] == pair[1] { run + 1 } else { 1 };
        if run >= REPEATED_WORDS {
            return Cleaned::Hallucination(text);
        }
    }
    Cleaned::Text(text)
}

/// The transcriber thread: takes utterances until the queue closes.
pub fn run(
    queue: Arc<UtteranceQueue>,
    model: ModelSlot,
    options: Arc<Mutex<DecodeOptions>>,
    counters: Arc<Counters>,
    sink: Arc<dyn TranscriptSink>,
    no_speech_threshold: f32,
) {
    while let Some(utterance) = queue.pop() {
        counters.queued.store(queue.len() as u64, Ordering::Relaxed);
        let options = options.lock().expect("decode options lock").clone();
        let started = Instant::now();
        let result = {
            let mut slot = model.lock().expect("whisper model lock");
            match slot.as_mut() {
                Some(model) => model.transcribe(&utterance.audio, &options),
                None => continue,
            }
        };
        let transcribe_ms = started.elapsed().as_millis() as u64;
        counters
            .last_transcribe_ms
            .store(transcribe_ms, Ordering::Relaxed);
        counters.last_audio_ms.store(
            (utterance.speech_seconds * 1000.0).round() as u64,
            Ordering::Relaxed,
        );
        counters.transcribed.fetch_add(1, Ordering::Relaxed);
        let raw = match result {
            Ok(raw) => raw,
            Err(error) => {
                log::warn!(
                    "[whisper:{}] transcription failed: {error}",
                    utterance.guild
                );
                continue;
            }
        };
        let language = raw.language.clone();
        match clean(&raw, no_speech_threshold) {
            Cleaned::Text(text) => {
                crate::detail!(
                    "whisper",
                    "[whisper:{}] ssrc {} heard in {transcribe_ms} ms ({:.1} s, {}): {text}",
                    utterance.guild,
                    utterance.ssrc,
                    utterance.speech_seconds,
                    language.as_deref().unwrap_or("?")
                );
                sink.on_transcript(Transcript {
                    guild: utterance.guild,
                    ssrc: utterance.ssrc,
                    text,
                    language,
                    speech_end: utterance.speech_end,
                    text_at: Instant::now(),
                    audio_seconds: utterance.speech_seconds,
                    transcribe_ms,
                    model: String::new(),
                    device: String::new(),
                });
            }
            Cleaned::NoSpeech => {
                counters.dropped_noise.fetch_add(1, Ordering::Relaxed);
                crate::detail!(
                    "whisper",
                    "[whisper:{}] ssrc {}: no speech in {:.1} s ({transcribe_ms} ms)",
                    utterance.guild,
                    utterance.ssrc,
                    utterance.speech_seconds
                );
            }
            Cleaned::Hallucination(text) => {
                let count = counters
                    .dropped_hallucination
                    .fetch_add(1, Ordering::Relaxed)
                    + 1;
                log::info!(
                    "[whisper:{}] dropped a repeated-word transcription as a hallucination ({count} so far)",
                    utterance.guild
                );
                crate::detail!("whisper", "[whisper:{}] dropped: {text}", utterance.guild);
            }
        }
    }
}

/// Hands a finished utterance to the queue, counting and logging the one dropped when the
/// transcriber is [`QUEUE_CAPACITY`] behind.
pub fn enqueue(queue: &UtteranceQueue, counters: &Counters, utterance: Utterance) {
    if let Some(dropped) = queue.push(utterance) {
        let count = counters.dropped_late.fetch_add(1, Ordering::Relaxed) + 1;
        log::warn!(
            "[whisper:{}] the transcriber is {QUEUE_CAPACITY} utterances behind; dropped the oldest ({:.1} s, {count} so far)",
            dropped.guild,
            dropped.speech_seconds
        );
    }
    counters.queued.store(queue.len() as u64, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::{RawSegment, SttError};
    use std::time::Duration;

    fn utterance(guild: u64, seconds: f64) -> Utterance {
        Utterance {
            guild,
            ssrc: 7,
            audio: vec![0.0; (seconds.max(1.0) * 16_000.0) as usize],
            speech_seconds: seconds,
            speech_end: Instant::now(),
            cut: false,
        }
    }

    fn raw(segments: &[(&str, f32)]) -> RawTranscript {
        RawTranscript {
            segments: segments
                .iter()
                .map(|(text, no_speech_prob)| RawSegment {
                    text: (*text).to_owned(),
                    no_speech_prob: *no_speech_prob,
                })
                .collect(),
            language: Some("en".to_owned()),
        }
    }

    #[test]
    fn a_full_queue_drops_the_oldest_and_counts_it() {
        let queue = UtteranceQueue::new();
        let counters = Counters::default();
        for index in 0..10 {
            enqueue(&queue, &counters, utterance(1, f64::from(index)));
        }
        assert_eq!(queue.len(), QUEUE_CAPACITY);
        assert_eq!(counters.dropped_late.load(Ordering::Relaxed), 2);
        assert_eq!(
            queue.pop().unwrap().speech_seconds,
            2.0,
            "0 and 1 were dropped"
        );
        queue.clear_guild(1);
        assert!(queue.is_empty());
    }

    #[test]
    fn the_guards_drop_no_speech_notes_and_repetitions() {
        assert_eq!(
            clean(
                &raw(&[(" Hello there.", 0.1), (" Thanks for watching!", 0.9)]),
                0.6
            ),
            Cleaned::Text("Hello there.".to_owned())
        );
        assert_eq!(
            clean(&raw(&[(" [BLANK_AUDIO]", 0.1)]), 0.6),
            Cleaned::NoSpeech
        );
        assert_eq!(clean(&raw(&[(" (music)", 0.2)]), 0.6), Cleaned::NoSpeech);
        assert_eq!(clean(&raw(&[]), 0.6), Cleaned::NoSpeech);
        assert!(matches!(
            clean(&raw(&[(" Bro, bro, bro, bro.", 0.1)]), 0.6),
            Cleaned::Hallucination(_)
        ));
        assert_eq!(
            clean(&raw(&[(" Bro, bro, bro, what?", 0.1)]), 0.6),
            Cleaned::Text("Bro, bro, bro, what?".to_owned())
        );
    }

    /// Records the clip lengths and options it was given and answers from a list.
    struct FakeModel {
        answers: Mutex<VecDeque<RawTranscript>>,
        seen: Arc<Mutex<Vec<(usize, DecodeOptions)>>>,
    }

    impl SpeechToText for FakeModel {
        fn transcribe(
            &mut self,
            audio: &[f32],
            options: &DecodeOptions,
        ) -> Result<RawTranscript, SttError> {
            self.seen
                .lock()
                .unwrap()
                .push((audio.len(), options.clone()));
            Ok(self.answers.lock().unwrap().pop_front().unwrap_or_default())
        }
    }

    #[derive(Default)]
    struct Collect(Mutex<Vec<Transcript>>);

    impl TranscriptSink for Collect {
        fn on_transcript(&self, transcript: Transcript) {
            self.0.lock().unwrap().push(transcript);
        }
    }

    #[test]
    fn the_thread_transcribes_padded_clips_with_the_options_and_filters() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let model: ModelSlot = Arc::new(Mutex::new(Some(Box::new(FakeModel {
            answers: Mutex::new(VecDeque::from([
                raw(&[(" What time is it in London?", 0.05)]),
                raw(&[(" Subtitles by the community.", 0.95)]),
                raw(&[(" no no no no no", 0.1)]),
            ])),
            seen: Arc::clone(&seen),
        }))));
        let options = Arc::new(Mutex::new(DecodeOptions {
            language: "en".to_owned(),
            threads: 4,
            prompt: Some("hey bro, goodbye".to_owned()),
        }));
        let queue = UtteranceQueue::new();
        let counters = Arc::new(Counters::default());
        let sink = Arc::new(Collect::default());
        for seconds in [0.3, 2.0, 1.5] {
            enqueue(&queue, &counters, utterance(1, seconds));
        }
        let thread = {
            let (queue, counters, sink) =
                (Arc::clone(&queue), Arc::clone(&counters), Arc::clone(&sink));
            std::thread::spawn(move || run(queue, model, options, counters, sink, 0.6))
        };
        for _ in 0..100 {
            if counters.transcribed.load(Ordering::Relaxed) == 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        queue.close();
        thread.join().unwrap();
        let heard = sink.0.lock().unwrap();
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].text, "What time is it in London?");
        assert_eq!(heard[0].language.as_deref(), Some("en"));
        let snapshot = counters.snapshot();
        assert_eq!(
            (
                snapshot.transcribed,
                snapshot.dropped_noise,
                snapshot.dropped_hallucination
            ),
            (3, 1, 1)
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].0, 16_000, "a short clip reaches Whisper as 1 s");
        assert_eq!(seen[0].1.prompt.as_deref(), Some("hey bro, goodbye"));
    }
}
