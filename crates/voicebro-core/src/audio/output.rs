//! The assistant's audio toward Discord: a small adaptive jitter buffer with a flush epoch,
//! mute, earcons and counters, read by songbird through `PcmSource`.

use super::earcon::{self, Earcon};
use super::frame::{Frame, FRAME_SAMPLES};
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use symphonia_core::io::MediaSource;

/// Frames kept at most: about 160 ms.
pub const CAPACITY: usize = 8;
const JITTER_MIN: usize = 3;
const JITTER_MAX: usize = CAPACITY;
/// Underruns before the jitter target grows by one frame.
const UNDERRUNS_BEFORE_GROW: u32 = 3;
/// Frames played without an underrun before the target relaxes by one.
const STABLE_FRAMES_BEFORE_RELAX: u32 = 750;
/// Peak above which a frame counts as the assistant speaking.
pub const SIGNAL_PEAK: i32 = 256;
const BYTES_PER_PACKET: usize = FRAME_SAMPLES * std::mem::size_of::<f32>();

/// A source of 20 ms frames that the output pulls on songbird's read clock, ahead of the
/// pushed frames: the call's jitter buffer. It must not lock the output from inside.
pub trait FrameProvider: Send + Sync {
    /// Writes the next frame into `out`; `false` when it has nothing to play right now.
    fn next_frame(&self, out: &mut Frame) -> bool;
    /// Drops everything held, as a flush does for the pushed frames.
    fn reset(&self);
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct OutputCounters {
    pub pushed: u64,
    pub dropped: u64,
    pub played: u64,
    pub underruns: u64,
    pub silence_frames: u64,
    pub flushed: u64,
}

struct Inner {
    frames: VecDeque<(u64, Frame)>,
    epoch: u64,
    jitter_target: usize,
    priming: bool,
    underrun_streak: u32,
    stable_frames: u32,
    muted: bool,
    closed: bool,
    earcon: VecDeque<f32>,
    last_signal: Option<Instant>,
    counters: OutputCounters,
    provider: Option<Arc<dyn FrameProvider>>,
}

/// Creates the queue and both ends of it: the handle the assistant and the gate hold, and
/// the source songbird reads.
pub fn output_pair() -> (OutputHandle, PcmSource) {
    let inner = Arc::new(Mutex::new(Inner {
        frames: VecDeque::with_capacity(CAPACITY),
        epoch: 0,
        jitter_target: JITTER_MIN,
        priming: true,
        underrun_streak: 0,
        stable_frames: 0,
        muted: false,
        closed: false,
        earcon: VecDeque::new(),
        last_signal: None,
        counters: OutputCounters::default(),
        provider: None,
    }));
    (
        OutputHandle {
            inner: Arc::clone(&inner),
        },
        PcmSource {
            inner,
            pending: Vec::new(),
            scratch: vec![0f32; FRAME_SAMPLES],
            pulled: Frame::silence(),
        },
    )
}

#[derive(Clone)]
pub struct OutputHandle {
    inner: Arc<Mutex<Inner>>,
}

impl OutputHandle {
    /// Queues a frame for playback; the oldest frame is dropped when the queue is full.
    /// While muted the frame is discarded, so nothing said during a mute plays later.
    pub fn push(&self, frame: Frame) {
        let mut inner = self.inner.lock().expect("output lock");
        inner.counters.pushed += 1;
        if frame.peak() >= SIGNAL_PEAK {
            inner.last_signal = Some(Instant::now());
        }
        if inner.muted || inner.closed {
            inner.counters.dropped += 1;
            return;
        }
        if inner.frames.len() >= CAPACITY {
            inner.frames.pop_front();
            inner.counters.dropped += 1;
        }
        let epoch = inner.epoch;
        inner.frames.push_back((epoch, frame));
    }

    /// Discards everything queued but not yet played, the attached provider's too.
    pub fn flush(&self) {
        let mut inner = self.inner.lock().expect("output lock");
        let flushed = inner.frames.len() as u64;
        inner.frames.clear();
        inner.epoch += 1;
        inner.counters.flushed += flushed;
        inner.priming = true;
        if let Some(provider) = inner.provider.as_ref() {
            provider.reset();
        }
    }

    pub fn set_muted(&self, muted: bool) {
        let mut inner = self.inner.lock().expect("output lock");
        if muted != inner.muted {
            let flushed = inner.frames.len() as u64;
            if muted {
                inner.frames.clear();
                inner.epoch += 1;
                inner.counters.flushed += flushed;
                inner.priming = true;
            }
            // Whatever the provider holds was said around the change: never play it.
            if let Some(provider) = inner.provider.as_ref() {
                provider.reset();
            }
        }
        inner.muted = muted;
    }

    /// Lets `provider` supply frames ahead of the pushed ones, until detached.
    pub fn attach_provider(&self, provider: Arc<dyn FrameProvider>) {
        self.inner.lock().expect("output lock").provider = Some(provider);
    }

    pub fn detach_provider(&self) {
        self.inner.lock().expect("output lock").provider = None;
    }

    pub fn is_muted(&self) -> bool {
        self.inner.lock().expect("output lock").muted
    }

    /// When the assistant last produced audible output; the gate's idle timeout counts it.
    pub fn last_signal(&self) -> Option<Instant> {
        self.inner.lock().expect("output lock").last_signal
    }

    /// Plays a tone, also while muted.
    pub fn play_earcon(&self, kind: Earcon, volume: f32) {
        let mut inner = self.inner.lock().expect("output lock");
        inner.earcon.extend(earcon::samples(kind, volume));
    }

    /// Ends the track: the next read reports end of stream. Only leaving a channel calls it.
    pub fn close(&self) {
        self.inner.lock().expect("output lock").closed = true;
    }

    pub fn is_closed(&self) -> bool {
        self.inner.lock().expect("output lock").closed
    }

    pub fn queued(&self) -> usize {
        self.inner.lock().expect("output lock").frames.len()
    }

    pub fn counters(&self) -> OutputCounters {
        self.inner.lock().expect("output lock").counters
    }
}

/// What songbird reads: f32 interleaved stereo at 48 kHz through `RawAdapter`.
pub struct PcmSource {
    inner: Arc<Mutex<Inner>>,
    pending: Vec<u8>,
    scratch: Vec<f32>,
    pulled: Frame,
}

impl PcmSource {
    /// Produces exactly one 20 ms packet into `self.scratch`. Returns `false` at end of
    /// stream.
    fn fill_packet(&mut self) -> bool {
        let mut inner = self.inner.lock().expect("output lock");
        if inner.closed {
            return false;
        }
        self.scratch.fill(0.0);
        let epoch = inner.epoch;
        // Skip frames queued before the last flush.
        while matches!(inner.frames.front(), Some((e, _)) if *e != epoch) {
            inner.frames.pop_front();
        }
        // The attached provider (the call's jitter buffer) plays first. While muted its
        // frames are pulled and discarded, so nothing said during a mute plays later.
        if let Some(provider) = inner.provider.clone() {
            if provider.next_frame(&mut self.pulled) && !inner.muted {
                for (out, sample) in self.scratch.iter_mut().zip(self.pulled.as_slice()) {
                    *out = *sample as f32 / 32_768.0;
                }
                inner.counters.played += 1;
                if self.pulled.peak() >= SIGNAL_PEAK {
                    inner.last_signal = Some(Instant::now());
                }
                Self::mix_earcon(&mut inner, &mut self.scratch);
                return true;
            }
        }
        let ready = if inner.priming {
            if inner.frames.len() >= inner.jitter_target {
                inner.priming = false;
                true
            } else {
                false
            }
        } else {
            !inner.frames.is_empty()
        };
        if ready && !inner.muted {
            if let Some((_, frame)) = inner.frames.pop_front() {
                for (out, sample) in self.scratch.iter_mut().zip(frame.as_slice()) {
                    *out = *sample as f32 / 32_768.0;
                }
                inner.counters.played += 1;
                inner.stable_frames += 1;
                if inner.stable_frames >= STABLE_FRAMES_BEFORE_RELAX {
                    inner.stable_frames = 0;
                    inner.underrun_streak = 0;
                    if inner.jitter_target > JITTER_MIN {
                        inner.jitter_target -= 1;
                    }
                }
            }
        } else {
            inner.counters.silence_frames += 1;
            if !inner.muted && !inner.priming {
                // The queue ran dry mid-stream.
                inner.counters.underruns += 1;
                inner.priming = true;
                inner.stable_frames = 0;
                inner.underrun_streak += 1;
                if inner.underrun_streak >= UNDERRUNS_BEFORE_GROW {
                    inner.underrun_streak = 0;
                    if inner.jitter_target < JITTER_MAX {
                        inner.jitter_target += 1;
                    }
                }
            }
        }
        Self::mix_earcon(&mut inner, &mut self.scratch);
        true
    }

    /// Earcons are mixed in whatever the state, one mono sample per stereo pair.
    fn mix_earcon(inner: &mut Inner, scratch: &mut [f32]) {
        if inner.earcon.is_empty() {
            return;
        }
        for pair in scratch.chunks_mut(2) {
            let Some(sample) = inner.earcon.pop_front() else {
                break;
            };
            for channel in pair {
                *channel = (*channel + sample).clamp(-1.0, 1.0);
            }
        }
    }

    pub fn jitter_target(&self) -> usize {
        self.inner.lock().expect("output lock").jitter_target
    }
}

impl Read for PcmSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            if !self.fill_packet() {
                return Ok(0);
            }
            self.pending.reserve(BYTES_PER_PACKET);
            for sample in &self.scratch {
                self.pending.extend_from_slice(&sample.to_le_bytes());
            }
        }
        let count = buf.len().min(self.pending.len());
        buf[..count].copy_from_slice(&self.pending[..count]);
        self.pending.drain(..count);
        Ok(count)
    }
}

impl Seek for PcmSource {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other("live assistant audio cannot seek"))
    }
}

impl MediaSource for PcmSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud(value: i16) -> Frame {
        Frame::from_slice(&vec![value; FRAME_SAMPLES])
    }

    fn read_packet(source: &mut PcmSource) -> Option<Vec<f32>> {
        let mut bytes = vec![0u8; BYTES_PER_PACKET];
        let mut filled = 0;
        while filled < BYTES_PER_PACKET {
            let n = source.read(&mut bytes[filled..]).unwrap();
            if n == 0 {
                return None;
            }
            filled += n;
        }
        Some(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
        )
    }

    #[test]
    fn primes_to_the_jitter_target_then_plays_in_order() {
        let (handle, mut source) = output_pair();
        handle.push(loud(1000));
        let first = read_packet(&mut source).unwrap();
        assert!(
            first.iter().all(|s| *s == 0.0),
            "one frame is below the target: silence"
        );
        handle.push(loud(2000));
        handle.push(loud(3000));
        let played = read_packet(&mut source).unwrap();
        assert!(
            (played[0] - 1000.0 / 32768.0).abs() < 1e-6,
            "oldest frame first"
        );
        assert_eq!(handle.counters().played, 1);
    }

    #[test]
    fn overflow_drops_oldest_and_flush_discards_everything() {
        let (handle, mut source) = output_pair();
        for value in 1..=20 {
            handle.push(loud(value * 100));
        }
        assert_eq!(handle.queued(), CAPACITY);
        assert_eq!(handle.counters().dropped, 12);
        let played = read_packet(&mut source).unwrap();
        assert!(
            (played[0] - 1300.0 / 32768.0).abs() < 1e-6,
            "frames 1..12 were dropped"
        );
        handle.flush();
        assert_eq!(handle.queued(), 0);
        let after = read_packet(&mut source).unwrap();
        assert!(after.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn mute_drops_queued_and_incoming_but_earcons_play() {
        let (handle, mut source) = output_pair();
        for _ in 0..4 {
            handle.push(loud(5000));
        }
        handle.set_muted(true);
        handle.push(loud(5000));
        assert_eq!(handle.queued(), 0);
        handle.play_earcon(Earcon::Activate, 0.18);
        let packet = read_packet(&mut source).unwrap();
        assert!(
            packet.iter().any(|s| s.abs() > 0.01),
            "earcon audible while muted"
        );
        handle.set_muted(false);
        let silence = read_packet(&mut source);
        assert!(silence.is_some());
    }

    #[test]
    fn empty_queue_is_silence_and_close_is_eof() {
        let (handle, mut source) = output_pair();
        assert!(read_packet(&mut source).is_some());
        assert!(handle.counters().silence_frames >= 1);
        handle.close();
        let mut buf = [0u8; 16];
        assert_eq!(source.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn jitter_target_grows_after_repeated_underruns() {
        let (handle, mut source) = output_pair();
        assert_eq!(source.jitter_target(), JITTER_MIN);
        for _ in 0..UNDERRUNS_BEFORE_GROW {
            for _ in 0..JITTER_MIN {
                handle.push(loud(1000));
            }
            for _ in 0..JITTER_MIN {
                read_packet(&mut source);
            }
            read_packet(&mut source); // dry: an underrun
        }
        assert_eq!(source.jitter_target(), JITTER_MIN + 1);
        assert_eq!(handle.counters().underruns, UNDERRUNS_BEFORE_GROW as u64);
    }

    #[test]
    fn last_signal_tracks_audible_frames_only() {
        let (handle, _source) = output_pair();
        handle.push(Frame::silence());
        assert!(handle.last_signal().is_none());
        handle.push(loud(1000));
        assert!(handle.last_signal().is_some());
    }

    /// A provider that plays a fixed number of loud frames and records resets.
    struct Countdown {
        left: Mutex<u32>,
        resets: Mutex<u32>,
    }

    impl FrameProvider for Countdown {
        fn next_frame(&self, out: &mut Frame) -> bool {
            let mut left = self.left.lock().unwrap();
            if *left == 0 {
                return false;
            }
            *left -= 1;
            out.as_mut_slice().fill(4_000);
            true
        }

        fn reset(&self) {
            *self.left.lock().unwrap() = 0;
            *self.resets.lock().unwrap() += 1;
        }
    }

    #[test]
    fn an_attached_provider_plays_first_and_a_flush_empties_it() {
        let (handle, mut source) = output_pair();
        let provider = Arc::new(Countdown {
            left: Mutex::new(5),
            resets: Mutex::new(0),
        });
        handle.attach_provider(provider.clone());
        handle.push(loud(1000));
        let packet = read_packet(&mut source).unwrap();
        assert!(
            (packet[0] - 4000.0 / 32768.0).abs() < 1e-6,
            "the provider's frame plays"
        );
        assert!(handle.last_signal().is_some());
        handle.flush();
        assert_eq!(*provider.resets.lock().unwrap(), 1);
        assert_eq!(
            *provider.left.lock().unwrap(),
            0,
            "the flush emptied the provider"
        );
        let after = read_packet(&mut source).unwrap();
        assert!(after.iter().all(|s| *s == 0.0), "nothing left to play");
    }

    #[test]
    fn a_muted_output_discards_the_providers_frames() {
        let (handle, mut source) = output_pair();
        let provider = Arc::new(Countdown {
            left: Mutex::new(0),
            resets: Mutex::new(0),
        });
        handle.attach_provider(provider.clone());
        handle.set_muted(true);
        *provider.left.lock().unwrap() = 3;
        for _ in 0..3 {
            let packet = read_packet(&mut source).unwrap();
            assert!(packet.iter().all(|s| *s == 0.0), "muted: silence");
        }
        assert_eq!(*provider.left.lock().unwrap(), 0, "pulled and discarded");
        handle.detach_provider();
        handle.set_muted(false);
        assert!(read_packet(&mut source).is_some());
    }

    #[test]
    fn partial_reads_are_served_from_the_pending_packet() {
        let (handle, mut source) = output_pair();
        for _ in 0..3 {
            handle.push(loud(1000));
        }
        let mut small = [0u8; 100];
        let mut total = 0;
        for _ in 0..(BYTES_PER_PACKET / 100 + 1) {
            total += source.read(&mut small).unwrap();
        }
        assert_eq!(total, BYTES_PER_PACKET);
    }
}
