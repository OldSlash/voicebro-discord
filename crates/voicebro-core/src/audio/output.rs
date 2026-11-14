//! The assistant's audio toward Discord: a small adaptive jitter buffer with a flush epoch,
//! mute, earcons and counters, read by songbird through `PcmSource`. The source also decides
//! whether anything is sent toward Discord at all (change `transmit-only-when-speaking`):
//! only while the assistant's voice or an earcon is audible.

use super::earcon::{self, Earcon};
use super::frame::{Frame, FRAME_SAMPLES};
use super::gain::GainStage;
use crate::latency::{self, SharedLatency};
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use symphonia_core::io::MediaSource;
use tokio::sync::watch;

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
/// Frames between asking for sending and sending surely being on: songbird applies the
/// driver's mute at the start of a later tick, normally the next one. Earcons wait for it.
pub const SEND_LEAD: u32 = 2;
/// Frames in a row with nothing audible before sending stops (500 ms): longer than every
/// pause inside an answer in the recorded calls (at most 300 ms).
pub const SEND_HANGOVER: u32 = 25;
const BYTES_PER_PACKET: usize = FRAME_SAMPLES * std::mem::size_of::<f32>();

/// A source of 20 ms frames that the output pulls on songbird's read clock, ahead of the
/// pushed frames: the call's jitter buffer. It must not lock the output from inside.
pub trait FrameProvider: Send + Sync {
    /// Writes the next frame into `out`; `false` when it has nothing to play right now.
    fn next_frame(&self, out: &mut Frame) -> bool;
    /// Drops everything held, as a flush does for the pushed frames.
    fn reset(&self);
    /// When the audio of the last frame given came from the network, if known. The turn
    /// latency measures from there.
    fn arrival(&self) -> Option<Instant> {
        None
    }
    /// Whether audio already held for the frames after the last one given has a peak of
    /// [`SIGNAL_PEAK`] or more: the output turns sending toward Discord on before that
    /// voice plays. `false` when the provider cannot see ahead.
    fn voice_ahead(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct OutputCounters {
    pub pushed: u64,
    pub dropped: u64,
    pub played: u64,
    pub underruns: u64,
    pub silence_frames: u64,
    pub flushed: u64,
    /// Whether sending toward Discord is on now (asked for or on).
    pub sending: bool,
    /// Times sending turned on.
    pub sends_started: u64,
    /// Frames pulled while nothing was sent toward Discord.
    pub frames_not_sent: u64,
    /// Starts whose first voiced frame played before sending was surely on: the start of
    /// that voice may be missing in Discord.
    pub cut_starts: u64,
}

/// Where the output stands with sending toward Discord (design D2 of change
/// `transmit-only-when-speaking`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sending {
    /// Nothing audible for the hangover: songbird is told to send nothing.
    Silent,
    /// Sending was asked for; `left` more frames before it is surely on. `cut` once a voiced
    /// frame has played meanwhile.
    Opening { left: u32, cut: bool },
    /// Sending; `quiet` frames in a row without anything audible.
    On { quiet: u32 },
}

/// A pushed frame with its epoch and whether it carries a signal.
struct Queued {
    epoch: u64,
    frame: Frame,
    voiced: bool,
}

struct Inner {
    frames: VecDeque<Queued>,
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
    /// The output gain, applied to the assistant's audio and not to earcons.
    gain: Arc<GainStage>,
    /// The guild's turn latency tracker, told when the assistant's audio plays.
    latency: Option<(SharedLatency, u64)>,
    sending: Sending,
    /// Whether sending is wanted, for whoever applies it to the driver (the session).
    sending_tx: watch::Sender<bool>,
}

impl Inner {
    fn set_sending(&mut self, on: bool) {
        self.counters.sending = on;
        self.sending_tx.send_replace(on);
    }

    /// One frame of the sending decision. `voiced`: the assistant's frame played now has a
    /// signal. `ahead`: audio waiting to play has one, or an earcon waits. Returns whether
    /// sending is surely on for this frame, so earcons may be mixed into it.
    fn step_sending(&mut self, voiced: bool, ahead: bool) -> bool {
        if let Sending::Opening { left: 0, .. } = self.sending {
            self.sending = Sending::On { quiet: 0 };
        }
        match self.sending {
            Sending::Silent => {
                self.counters.frames_not_sent += 1;
                if voiced || ahead {
                    self.counters.sends_started += 1;
                    if voiced {
                        self.counters.cut_starts += 1;
                    }
                    self.sending = Sending::Opening {
                        left: SEND_LEAD - 1,
                        cut: voiced,
                    };
                    self.set_sending(true);
                }
                false
            }
            Sending::Opening { left, cut } => {
                if voiced && !cut {
                    self.counters.cut_starts += 1;
                }
                self.sending = Sending::Opening {
                    left: left.saturating_sub(1),
                    cut: cut || voiced,
                };
                false
            }
            Sending::On { quiet } => {
                let quiet = if voiced || ahead { 0 } else { quiet + 1 };
                if quiet >= SEND_HANGOVER {
                    self.sending = Sending::Silent;
                    self.set_sending(false);
                } else {
                    self.sending = Sending::On { quiet };
                }
                true
            }
        }
    }
}

/// Creates the queue and both ends of it: the handle the assistant and the gate hold, and
/// the source songbird reads. The gain starts at 1.0 until [`OutputHandle::use_gain`]
/// shares the app's output gain. Sending starts off: nothing is audible yet.
pub fn output_pair() -> (OutputHandle, PcmSource) {
    let (sending_tx, _) = watch::channel(false);
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
        gain: Arc::new(GainStage::new()),
        latency: None,
        sending: Sending::Silent,
        sending_tx,
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
            latency_lines: Vec::new(),
        },
    )
}

/// Tells the guild's latency tracker that a frame of the assistant's audio with a signal is
/// played now; `received` is when its audio came from the network, if known.
fn note_answer(inner: &Inner, received: Option<Instant>, lines: &mut Vec<String>) {
    let Some((tracker, guild)) = inner.latency.as_ref() else {
        return;
    };
    let now = Instant::now();
    let mut tracker = tracker.lock().expect("latency lock");
    if let Some(event) = tracker.answer_audio(received.unwrap_or(now).min(now)) {
        lines.push(latency::log_line(*guild, &event));
    }
    if tracker.awaiting_playback() {
        let clock = chrono::Local::now().format("%H:%M:%S").to_string();
        if let Some(event) = tracker.played(now, &clock) {
            lines.push(latency::log_line(*guild, &event));
        }
    }
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
        let voiced = frame.peak() >= SIGNAL_PEAK;
        if voiced {
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
        inner.frames.push_back(Queued {
            epoch,
            frame,
            voiced,
        });
    }

    /// Whether sending toward Discord is wanted: `true` while something is audible and for
    /// the hangover after it. The session applies each change to songbird's driver.
    pub fn sending(&self) -> watch::Receiver<bool> {
        self.inner
            .lock()
            .expect("output lock")
            .sending_tx
            .subscribe()
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

    /// Scales everything the assistant says by `stage`'s gain from the next read on.
    pub fn use_gain(&self, stage: Arc<GainStage>) {
        self.inner.lock().expect("output lock").gain = stage;
    }

    /// Reports the playback of the assistant's audio to `guild`'s latency tracker.
    pub fn use_latency(&self, tracker: SharedLatency, guild: u64) {
        self.inner.lock().expect("output lock").latency = Some((tracker, guild));
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
    /// Latency lines found while the output lock was held, logged after it is released.
    latency_lines: Vec<String>,
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
        while matches!(inner.frames.front(), Some(queued) if queued.epoch != epoch) {
            inner.frames.pop_front();
        }
        // Whether the assistant's frame played now has a signal, and whether the provider
        // sees one ahead. Nothing of the assistant counts while muted.
        let mut voiced = false;
        let mut provider_ahead = false;
        let mut from_provider = false;
        // The attached provider (the call's jitter buffer) plays first. While muted its
        // frames are pulled and discarded, so nothing said during a mute plays later.
        if let Some(provider) = inner.provider.clone() {
            if provider.next_frame(&mut self.pulled) && !inner.muted {
                inner
                    .gain
                    .scale_to_f32(self.pulled.as_slice(), &mut self.scratch);
                inner.counters.played += 1;
                if self.pulled.peak() >= SIGNAL_PEAK {
                    inner.last_signal = Some(Instant::now());
                    note_answer(&inner, provider.arrival(), &mut self.latency_lines);
                    voiced = true;
                }
                from_provider = true;
            }
            provider_ahead = !inner.muted && provider.voice_ahead();
        }
        if !from_provider {
            voiced = Self::play_pushed(&mut inner, &mut self.scratch, &mut self.latency_lines);
        }
        let ahead = provider_ahead
            || (!inner.muted && inner.frames.iter().any(|queued| queued.voiced))
            || !inner.earcon.is_empty();
        if inner.step_sending(voiced, ahead) {
            Self::mix_earcon(&mut inner, &mut self.scratch);
        }
        true
    }

    /// Plays the next pushed frame into `scratch` once the queue is primed; returns whether
    /// it has a signal.
    fn play_pushed(
        inner: &mut Inner,
        scratch: &mut [f32],
        latency_lines: &mut Vec<String>,
    ) -> bool {
        let mut voiced = false;
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
            if let Some(queued) = inner.frames.pop_front() {
                inner.gain.scale_to_f32(queued.frame.as_slice(), scratch);
                inner.counters.played += 1;
                if queued.voiced {
                    note_answer(inner, None, latency_lines);
                    voiced = true;
                }
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
        voiced
    }

    /// Earcons are mixed in whatever the mute, one mono sample per stereo pair; only while
    /// sending is surely on, so none is lost to songbird's mute (they wait meanwhile).
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

    /// One 20 ms frame of f32 stereo ([`FRAME_SAMPLES`] values) into `out`: the same frame
    /// `read` hands out as bytes, for the Opus output (change `discord-voice-delivery`).
    /// Returns `false` at the end of the stream.
    pub fn fill_frame(&mut self, out: &mut [f32]) -> bool {
        let filled = self.fill_packet();
        for line in self.latency_lines.drain(..) {
            crate::detail!("latency", "{line}");
        }
        if !filled {
            return false;
        }
        for (slot, sample) in out.iter_mut().zip(self.scratch.iter()) {
            *slot = *sample;
        }
        true
    }
}

impl Read for PcmSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            let filled = self.fill_packet();
            for line in self.latency_lines.drain(..) {
                crate::detail!("latency", "{line}");
            }
            if !filled {
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
        // From silence the earcon waits for sending to be on.
        let audible = (0..=SEND_LEAD)
            .filter_map(|_| read_packet(&mut source))
            .any(|packet| packet.iter().any(|s| s.abs() > 0.01));
        assert!(audible, "earcon audible while muted");
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
    fn the_output_gain_scales_pushed_and_pulled_frames_but_not_earcons() {
        let stage = Arc::new(GainStage::new());
        stage.set_gain(2.5);
        let (handle, mut source) = output_pair();
        handle.use_gain(Arc::clone(&stage));
        for _ in 0..3 {
            handle.push(loud(1000));
        }
        let pushed = read_packet(&mut source).unwrap();
        assert!(
            (pushed[0] - 2_500.0 / 32_768.0).abs() < 1e-6,
            "pushed: {}",
            pushed[0]
        );

        let provider = Arc::new(Countdown {
            left: Mutex::new(2),
            resets: Mutex::new(0),
        });
        handle.attach_provider(provider);
        let pulled = read_packet(&mut source).unwrap();
        assert!(
            (pulled[0] - 10_000.0 / 32_768.0).abs() < 1e-6,
            "pulled: {}",
            pulled[0]
        );
        stage.set_gain(4.0);
        let louder = read_packet(&mut source).unwrap();
        assert!(
            (louder[0] - 16_000.0 / 32_768.0).abs() < 1e-6,
            "a new gain reaches the next read: {}",
            louder[0]
        );
        handle.detach_provider();

        // Earcons keep their own volume whatever the gain.
        let earcon_at = |gain: f32| {
            let stage = Arc::new(GainStage::new());
            stage.set_gain(gain);
            let (handle, mut source) = output_pair();
            handle.use_gain(stage);
            handle.play_earcon(Earcon::Activate, 0.18);
            // The first packet the earcon is in, after the wait for sending.
            (0..=SEND_LEAD)
                .filter_map(|_| read_packet(&mut source))
                .last()
                .unwrap()
        };
        let plain = earcon_at(1.0);
        assert!(plain.iter().any(|s| *s != 0.0), "the earcon plays");
        assert_eq!(earcon_at(4.0), plain);

        // A sample pushed past full scale is clamped and counted.
        handle.flush();
        let clips = stage.clips();
        for _ in 0..3 {
            handle.push(loud(20_000));
        }
        let clamped = read_packet(&mut source).unwrap();
        assert_eq!(clamped[0], 1.0);
        assert_eq!(stage.clips() - clips, FRAME_SAMPLES as u64);
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

    fn audible(packet: &[f32]) -> bool {
        packet.iter().any(|sample| *sample != 0.0)
    }

    /// Reads one packet per tick, pushing one loud frame before each of the first `voiced`
    /// reads as an assistant speaking in real time does, then `quiet` reads with nothing
    /// pushed. Per read: whether the packet was audible, and whether sending was on after it.
    fn speak_then_wait(
        handle: &OutputHandle,
        source: &mut PcmSource,
        voiced: usize,
        quiet: usize,
    ) -> Vec<(bool, bool)> {
        (0..voiced + quiet)
            .map(|tick| {
                if tick < voiced {
                    handle.push(loud(4_000));
                }
                let packet = read_packet(source).unwrap();
                (audible(&packet), handle.counters().sending)
            })
            .collect()
    }

    /// A provider playing frames at the given levels in order, seeing four frames ahead.
    struct Lookahead {
        levels: Mutex<VecDeque<i16>>,
    }

    impl Lookahead {
        fn new(levels: &[i16]) -> Self {
            Self {
                levels: Mutex::new(levels.iter().copied().collect()),
            }
        }
    }

    impl FrameProvider for Lookahead {
        fn next_frame(&self, out: &mut Frame) -> bool {
            let Some(level) = self.levels.lock().unwrap().pop_front() else {
                return false;
            };
            out.as_mut_slice().fill(level);
            true
        }

        fn reset(&self) {
            self.levels.lock().unwrap().clear();
        }

        fn voice_ahead(&self) -> bool {
            self.levels
                .lock()
                .unwrap()
                .iter()
                .take(4)
                .any(|level| i32::from(*level) >= SIGNAL_PEAK)
        }
    }

    #[test]
    fn an_output_with_nothing_to_play_never_sends() {
        let (handle, mut source) = output_pair();
        let sending = handle.sending();
        for _ in 0..100 {
            read_packet(&mut source).unwrap();
        }
        let counters = handle.counters();
        assert!(!counters.sending);
        assert_eq!(counters.sends_started, 0);
        assert_eq!(counters.frames_not_sent, 100);
        assert!(!sending.has_changed().unwrap(), "no change announced");
    }

    #[test]
    fn pushed_voice_turns_sending_on_before_it_plays() {
        let (handle, mut source) = output_pair();
        let ticks = speak_then_wait(&handle, &mut source, 10, 0);
        let first_audible = ticks.iter().position(|(audible, _)| *audible).unwrap();
        let first_sending = ticks.iter().position(|(_, sending)| *sending).unwrap();
        assert!(
            first_sending + SEND_LEAD as usize <= first_audible,
            "sending on after read {first_sending}, voice at read {first_audible}"
        );
        let counters = handle.counters();
        assert_eq!(counters.sends_started, 1);
        assert_eq!(counters.cut_starts, 0);
    }

    #[test]
    fn sending_stops_after_the_hangover_and_holds_through_a_shorter_pause() {
        let (handle, mut source) = output_pair();
        let ticks = speak_then_wait(&handle, &mut source, 10, 40);
        let last_audible = ticks.iter().rposition(|(audible, _)| *audible).unwrap();
        let hangover = SEND_HANGOVER as usize;
        assert!(
            ticks[last_audible + hangover - 1].1,
            "still sending after {} quiet frames",
            hangover - 1
        );
        assert!(
            !ticks[last_audible + hangover].1,
            "sending stopped after {hangover} quiet frames"
        );
        assert!(!handle.counters().sending);

        let (handle, mut source) = output_pair();
        speak_then_wait(&handle, &mut source, 10, 20);
        let resumed = speak_then_wait(&handle, &mut source, 10, 0);
        assert!(resumed.iter().all(|(_, sending)| *sending));
        assert_eq!(
            handle.counters().sends_started,
            1,
            "a pause of 20 frames keeps sending on"
        );
    }

    #[test]
    fn an_earcon_from_silence_waits_for_sending_then_plays_whole() {
        let (handle, mut source) = output_pair();
        handle.play_earcon(Earcon::Activate, 0.18);
        let expected = earcon::samples(Earcon::Activate, 0.18);
        let lead = SEND_LEAD as usize;
        let reads = expected.len().div_ceil(FRAME_SAMPLES / 2) + lead;
        let mut heard = Vec::new();
        for tick in 0..reads {
            let packet = read_packet(&mut source).unwrap();
            if tick < lead {
                assert!(!audible(&packet), "read {tick}: the earcon waits");
                assert!(handle.counters().sending, "sending is asked for at once");
            } else {
                heard.extend(packet.iter().step_by(2).copied());
            }
        }
        heard.truncate(expected.len());
        assert_eq!(heard, expected, "every sample of the earcon plays");
        assert_eq!(handle.counters().cut_starts, 0);
    }

    #[test]
    fn a_muted_output_sends_its_earcons_but_not_the_assistant() {
        let (handle, mut source) = output_pair();
        handle.set_muted(true);
        handle.attach_provider(Arc::new(Lookahead::new(&[4_000; 20])));
        for _ in 0..3 {
            handle.push(loud(4_000));
        }
        for _ in 0..10 {
            read_packet(&mut source).unwrap();
        }
        assert_eq!(
            handle.counters().sends_started,
            0,
            "the muted assistant never turns sending on"
        );
        handle.play_earcon(Earcon::Mute, 0.18);
        read_packet(&mut source).unwrap();
        assert!(handle.counters().sending, "the mute tone turns sending on");
        let tone = (0..SEND_LEAD)
            .filter_map(|_| read_packet(&mut source))
            .any(|packet| audible(&packet));
        assert!(tone, "the mute tone plays");
    }

    #[test]
    fn a_provider_seeing_voice_ahead_turns_sending_on_before_it_plays() {
        let (handle, mut source) = output_pair();
        let mut levels = vec![0i16; 10];
        levels.extend([4_000i16; 10]);
        handle.attach_provider(Arc::new(Lookahead::new(&levels)));
        let ticks: Vec<(bool, bool)> = (0..levels.len())
            .map(|_| {
                let packet = read_packet(&mut source).unwrap();
                (audible(&packet), handle.counters().sending)
            })
            .collect();
        let first_audible = ticks.iter().position(|(audible, _)| *audible).unwrap();
        let first_sending = ticks.iter().position(|(_, sending)| *sending).unwrap();
        assert_eq!(first_audible, 10);
        assert!(
            first_sending + SEND_LEAD as usize <= first_audible,
            "sending on after read {first_sending}"
        );
        let counters = handle.counters();
        assert_eq!(counters.sends_started, 1);
        assert_eq!(counters.cut_starts, 0);
    }

    #[test]
    fn a_voiced_frame_with_nothing_ahead_counts_one_cut_start() {
        let (handle, mut source) = output_pair();
        handle.attach_provider(Arc::new(Countdown {
            left: Mutex::new(5),
            resets: Mutex::new(0),
        }));
        for _ in 0..5 {
            read_packet(&mut source).unwrap();
        }
        let counters = handle.counters();
        assert_eq!(counters.sends_started, 1);
        assert_eq!(counters.cut_starts, 1, "one start, cut once");
    }

    #[test]
    fn the_sending_receiver_sees_each_change_once() {
        let (handle, mut source) = output_pair();
        let mut sending = handle.sending();
        assert!(!*sending.borrow_and_update());
        handle.push(loud(4_000));
        read_packet(&mut source).unwrap();
        assert!(sending.has_changed().unwrap());
        assert!(*sending.borrow_and_update(), "on");
        speak_then_wait(&handle, &mut source, 9, SEND_HANGOVER as usize - 1);
        assert!(
            !sending.has_changed().unwrap(),
            "no change while it stays on"
        );
        for _ in 0..10 {
            read_packet(&mut source).unwrap();
        }
        assert!(sending.has_changed().unwrap());
        assert!(!*sending.borrow_and_update(), "off");
        for _ in 0..10 {
            read_packet(&mut source).unwrap();
        }
        assert!(
            !sending.has_changed().unwrap(),
            "no change while it stays off"
        );
    }
}
