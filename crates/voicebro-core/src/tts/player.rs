//! The local voice's audio on the guild's output (change `whisper-mode`, design D7): a
//! [`FrameProvider`] the songbird mixer pulls one 20 ms frame from per read, as it pulls the
//! call's jitter buffer in realtime mode.
//!
//! The player holds the synthesized 48 kHz audio and plays it in order. Every flush
//! (deactivation, barge-in, mute, a mode change) raises its epoch: what it holds is dropped
//! at once, and audio of an older epoch is refused, so a synthesis that was running when the
//! flush came never plays.

use crate::audio::{Frame, FrameProvider, FRAME_SAMPLES, SIGNAL_PEAK};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Seconds of audio a 48 kHz stereo frame holds.
const FRAME_SECONDS: f64 = 0.02;

/// Told when the first frame of a turn's voice plays: the latency line's last step.
pub type FirstPlayed = Arc<dyn Fn(u64, Instant) + Send + Sync>;

struct Held {
    frame: Frame,
    voiced: bool,
    /// The first frame of this turn's voice.
    turn: Option<u64>,
}

#[derive(Default)]
struct Inner {
    frames: VecDeque<Held>,
    /// Mono 48 kHz samples not yet a whole frame.
    partial: Vec<i16>,
    /// The turn whose first frame the next push starts, until it is pushed.
    turn_start: Option<u64>,
    voiced_held: usize,
}

/// See the module documentation.
#[derive(Default)]
pub struct SpeechPlayer {
    inner: Mutex<Inner>,
    epoch: AtomicU64,
    first_played: Mutex<Option<FirstPlayed>>,
}

impl SpeechPlayer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Calls `hook` when the first frame of each turn plays.
    pub fn on_first_played(&self, hook: FirstPlayed) {
        *self.first_played.lock().expect("player hook lock") = Some(hook);
    }

    /// Drops everything held and refuses audio of the epoch before. Returns the new epoch.
    pub fn flush(&self) -> u64 {
        let epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        *self.inner.lock().expect("player lock") = Inner::default();
        epoch
    }

    /// Marks the next audio pushed as the start of `turn`'s voice.
    pub fn start_turn(&self, epoch: u64, turn: u64) {
        let mut inner = self.inner.lock().expect("player lock");
        if epoch == self.epoch() {
            inner.turn_start = Some(turn);
        }
    }

    /// Adds 48 kHz mono audio in -1..1 of `epoch`, scaled to `i16` and duplicated to both
    /// channels. Returns `false`, holding nothing, when the epoch is gone.
    pub fn push(&self, epoch: u64, samples: &[f32]) -> bool {
        let mut inner = self.inner.lock().expect("player lock");
        if epoch != self.epoch() {
            return false;
        }
        inner.partial.extend(
            samples
                .iter()
                .map(|&sample| (sample.clamp(-1.0, 1.0) * 32_767.0) as i16),
        );
        let mono = FRAME_SAMPLES / 2;
        while inner.partial.len() >= mono {
            let chunk: Vec<i16> = inner.partial.drain(..mono).collect();
            Self::hold(&mut inner, &chunk);
        }
        true
    }

    /// The end of a piece: the rest becomes a frame, padded with silence.
    pub fn end_piece(&self, epoch: u64) {
        let mut inner = self.inner.lock().expect("player lock");
        if epoch != self.epoch() || inner.partial.is_empty() {
            return;
        }
        let mut chunk = std::mem::take(&mut inner.partial);
        chunk.resize(FRAME_SAMPLES / 2, 0);
        Self::hold(&mut inner, &chunk);
    }

    fn hold(inner: &mut Inner, mono: &[i16]) {
        let stereo: Vec<i16> = mono.iter().flat_map(|&sample| [sample, sample]).collect();
        let frame = Frame::from_slice(&stereo);
        let voiced = frame.peak() >= SIGNAL_PEAK;
        if voiced {
            inner.voiced_held += 1;
        }
        let turn = inner.turn_start.take();
        inner.frames.push_back(Held {
            frame,
            voiced,
            turn,
        });
    }

    /// Seconds of audio held, for the synthesis's back-pressure.
    pub fn held_seconds(&self) -> f64 {
        let inner = self.inner.lock().expect("player lock");
        inner.frames.len() as f64 * FRAME_SECONDS
            + inner.partial.len() as f64 / (FRAME_SAMPLES / 2) as f64 * FRAME_SECONDS
    }

    pub fn is_empty(&self) -> bool {
        let inner = self.inner.lock().expect("player lock");
        inner.frames.is_empty() && inner.partial.is_empty()
    }
}

impl FrameProvider for SpeechPlayer {
    fn next_frame(&self, out: &mut Frame) -> bool {
        let (held, hook) = {
            let mut inner = self.inner.lock().expect("player lock");
            let Some(held) = inner.frames.pop_front() else {
                return false;
            };
            if held.voiced {
                inner.voiced_held -= 1;
            }
            let hook = held
                .turn
                .and_then(|_| self.first_played.lock().expect("player hook lock").clone());
            (held, hook)
        };
        *out = held.frame;
        if let (Some(turn), Some(hook)) = (held.turn, hook) {
            hook(turn, Instant::now());
        }
        true
    }

    /// The output's flush and mute call this: what is held is never played.
    fn reset(&self) {
        let mut inner = self.inner.lock().expect("player lock");
        inner.frames.clear();
        inner.partial.clear();
        inner.voiced_held = 0;
        inner.turn_start = None;
    }

    fn voice_ahead(&self) -> bool {
        self.inner.lock().expect("player lock").voiced_held > 0
    }

    fn own_latency(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f64) -> Vec<f32> {
        (0..(seconds * 48_000.0) as usize)
            .map(|index| (index as f32 * 0.05).sin() * 0.5)
            .collect()
    }

    #[test]
    fn audio_plays_in_order_and_voice_ahead_comes_before_it() {
        let player = SpeechPlayer::new();
        let epoch = player.epoch();
        assert!(!player.voice_ahead());
        assert!(player.push(epoch, &vec![0.0; 960 * 2]));
        assert!(!player.voice_ahead(), "silence is not voice");
        assert!(player.push(epoch, &tone(0.05)));
        player.end_piece(epoch);
        assert!(player.voice_ahead(), "voiced audio is held before it plays");
        let mut frame = Frame::silence();
        let mut played = 0;
        while player.next_frame(&mut frame) {
            played += 1;
        }
        assert_eq!(
            played,
            2 + 3,
            "two silent frames, 50 ms padded to three frames"
        );
        assert!(!player.voice_ahead());
    }

    #[test]
    fn a_flush_drops_what_is_held_and_refuses_the_old_epoch() {
        let player = SpeechPlayer::new();
        let old = player.epoch();
        player.push(old, &tone(1.0));
        let new = player.flush();
        assert!(player.is_empty());
        assert!(
            !player.push(old, &tone(0.1)),
            "a synthesis of the old epoch"
        );
        assert!(player.is_empty());
        assert!(player.push(new, &tone(0.1)));
        assert!((player.held_seconds() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn the_first_frame_of_a_turn_reports_when_it_plays() {
        let player = SpeechPlayer::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        player.on_first_played(Arc::new(move |turn, _| record.lock().unwrap().push(turn)));
        let epoch = player.epoch();
        player.start_turn(epoch, 4);
        player.push(epoch, &tone(0.1));
        player.push(epoch, &tone(0.1));
        let mut frame = Frame::silence();
        while player.next_frame(&mut frame) {}
        assert_eq!(*seen.lock().unwrap(), vec![4]);
    }
}
