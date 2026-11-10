//! Adaptive playout for the Codex call's voice (change `codex-adaptive-playout`).
//!
//! The realtime server sends the assistant's voice unevenly and, on average, a few percent
//! slower than real time. This module keeps the voice continuous anyway:
//! - [`DepthEstimator`] measures how late packets arrive against their RTP clock and turns
//!   that into a buffer depth between [`DEPTH_MIN_MS`] and [`DEPTH_MAX_MS`]. No rate of
//!   server lateness is assumed: the depth follows what the packets show.
//! - [`Playout`] holds the decoded samples and makes one 20 ms frame per pull. Below the
//!   depth it lengthens the voice (a pause by 20 ms first; voiced speech by one pitch period
//!   only when no pause has been usable); above it, it shortens the same way.
//!
//! Every buffer is allocated in `new()`. Nothing on the pull path allocates, and no slice is
//! indexed without a checked range: a bad situation plays the frame unchanged instead.

use std::time::{Duration, Instant};

/// Mono samples per ms at 48 kHz.
const PER_MS: usize = 48;
/// Mono samples per 20 ms frame.
pub const FRAME: usize = 960;

/// The smallest buffer depth.
pub const DEPTH_MIN_MS: u32 = 60;
/// The largest buffer depth.
pub const DEPTH_MAX_MS: u32 = 500;
/// Added to the measured lateness.
const DEPTH_MARGIN_MS: u32 = 20;
/// Packets are compared with the earliest one of this window.
const WINDOW: Duration = Duration::from_millis(2_000);
/// Slots of the window (2 s at 50 packets per second, with room for 10 ms packets).
const WINDOW_SLOTS: usize = 256;
const BUCKET_MS: f64 = 20.0;
/// Histogram buckets of 20 ms: 0 to 1000 ms.
const BUCKETS: usize = 50;
/// Per-packet forgetting of the histogram: an old stall fades out over about 15 s.
const FORGET: f64 = 0.9983;
const QUANTILE: f64 = 0.95;

/// Below `depth - LOW_MARGIN` the voice is lengthened.
const LOW_MARGIN: usize = 20 * PER_MS;
/// Above `depth + HIGH_MARGIN` the voice is shortened.
const HIGH_MARGIN: usize = 60 * PER_MS;
/// Cross-fade of a pause splice (5 ms).
const PAUSE_FADE: usize = 5 * PER_MS;
/// A pause is shortened only when this much of it lies ahead, so 60 ms of it remain.
const PAUSE_AHEAD: usize = 5 * FRAME;
/// Voiced speech is stretched only after this many frames without a usable pause (200 ms).
const FRAMES_WITHOUT_PAUSE: u32 = 10;
/// At most one voiced stretch or shrink per this many frames (60 ms).
const FRAMES_BETWEEN_VOICED: u32 = 3;
/// Pause stretches without a new packet at most (the deepest buffer): beyond that the
/// stream has stopped rather than slowed, and the pause plays out so the buffer runs dry.
const MAX_PAUSE_STRETCH_RUN: u32 = DEPTH_MAX_MS / 20;
/// The pitch periods considered: 2.5 to 15 ms.
const PERIOD_MIN: usize = 120;
const PERIOD_MAX: usize = 720;
/// The pitch search runs on the signal decimated by this factor (to 4 kHz).
const DECIMATE: usize = 12;
/// The decimated search window: 20 ms at 4 kHz, plus the longest lag.
const SEARCH_WINDOW: usize = 80;
const SEARCH_SPAN: usize = SEARCH_WINDOW + PERIOD_MAX / DECIMATE;
/// A splice needs at least this normalized correlation.
const MIN_CORRELATION: f32 = 0.5;

/// Already played samples kept before the read position (for splices and the search).
const HISTORY: usize = 2_048;
/// Decoded samples wanted ahead of the read position.
pub const LOOKAHEAD: usize = PAUSE_AHEAD;
/// The longest Opus packet: 120 ms.
pub const MAX_PACKET: usize = 5_760;
const CAPACITY: usize = HISTORY + LOOKAHEAD + MAX_PACKET + FRAME * 2;
const ARRIVAL_SLOTS: usize = 64;

/// The prior of a call with nothing to start from (design D2a): this share of the mass in
/// the 160–180 ms bucket, which the 95th percentile turns into a depth of 200 ms. It fades
/// like anything learnt: on a steady server the depth is back at 60 ms after about 21 s.
const PRIOR_BUCKET: usize = 8;
const PRIOR_MASS: f64 = 0.3;

/// Turns packet lateness into a buffer depth (design D2).
#[derive(Clone)]
pub struct DepthEstimator {
    reference: Option<(Instant, i64)>,
    last_timestamp: u32,
    extended: i64,
    /// (arrival, offset in ms) of the packets of the last 2 s, oldest first.
    window: [Option<(Instant, f64)>; WINDOW_SLOTS],
    window_start: usize,
    window_len: usize,
    histogram: [f64; BUCKETS],
}

impl Default for DepthEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl DepthEstimator {
    /// An estimator that knows nothing: it starts at the minimum depth.
    pub fn new() -> Self {
        let mut histogram = [0.0; BUCKETS];
        if let Some(zero) = histogram.first_mut() {
            *zero = 1.0;
        }
        Self {
            reference: None,
            last_timestamp: 0,
            extended: 0,
            window: [None; WINDOW_SLOTS],
            window_start: 0,
            window_len: 0,
            histogram,
        }
    }

    /// The start of a call with nothing learnt yet: a depth of 200 ms that fades toward the
    /// measured lateness (design D2a).
    pub fn with_prior() -> Self {
        let mut estimator = Self::new();
        if let Some(zero) = estimator.histogram.first_mut() {
            *zero = 1.0 - PRIOR_MASS;
        }
        if let Some(prior) = estimator.histogram.get_mut(PRIOR_BUCKET) {
            *prior = PRIOR_MASS;
        }
        estimator
    }

    /// What this estimator learnt, ready to start the next call: the histogram, without the
    /// reference and the window of this call.
    pub fn learned(&self) -> Self {
        let mut next = self.clone();
        next.restart();
        next
    }

    /// Forgets the reference and the window (after a flush), keeping what the histogram
    /// learned about the server.
    pub fn restart(&mut self) {
        self.reference = None;
        self.window = [None; WINDOW_SLOTS];
        self.window_start = 0;
        self.window_len = 0;
    }

    /// Notes one packet: its arrival and RTP timestamp (48 kHz).
    pub fn on_packet(&mut self, arrived: Instant, timestamp: u32) {
        let extended = match self.reference {
            None => 0,
            Some(_) => {
                self.extended + i64::from(timestamp.wrapping_sub(self.last_timestamp) as i32)
            }
        };
        self.last_timestamp = timestamp;
        self.extended = extended;
        let (reference_at, reference_ts) = *self.reference.get_or_insert((arrived, extended));
        let arrival_ms = if arrived >= reference_at {
            arrived.duration_since(reference_at).as_secs_f64() * 1e3
        } else {
            -(reference_at.duration_since(arrived).as_secs_f64() * 1e3)
        };
        let offset = arrival_ms - (extended - reference_ts) as f64 / PER_MS as f64;
        // Drop packets older than the window, then add this one.
        while self.window_len > 0 {
            let Some(oldest) = self.window.get_mut(self.window_start) else {
                break;
            };
            match *oldest {
                Some((at, _)) if arrived.saturating_duration_since(at) > WINDOW => {
                    *oldest = None;
                    self.window_start = (self.window_start + 1) % WINDOW_SLOTS;
                    self.window_len -= 1;
                }
                _ => break,
            }
        }
        if self.window_len == WINDOW_SLOTS {
            self.window_start = (self.window_start + 1) % WINDOW_SLOTS;
            self.window_len -= 1;
        }
        if let Some(slot) = self
            .window
            .get_mut((self.window_start + self.window_len) % WINDOW_SLOTS)
        {
            *slot = Some((arrived, offset));
            self.window_len += 1;
        }
        let earliest = self
            .window
            .iter()
            .flatten()
            .map(|(_, offset)| *offset)
            .fold(offset, f64::min);
        let delay = (offset - earliest).max(0.0);
        let bucket = ((delay / BUCKET_MS) as usize).min(BUCKETS - 1);
        for value in &mut self.histogram {
            *value *= FORGET;
        }
        if let Some(value) = self.histogram.get_mut(bucket) {
            *value += 1.0 - FORGET;
        }
    }

    /// The depth in ms: the 95th percentile of the lateness plus a margin, clamped.
    pub fn depth_ms(&self) -> u32 {
        let total: f64 = self.histogram.iter().sum();
        let mut cumulative = 0.0;
        let mut upper = BUCKETS as u32 * BUCKET_MS as u32;
        for (index, value) in self.histogram.iter().enumerate() {
            cumulative += value;
            if cumulative >= QUANTILE * total {
                upper = (index as u32 + 1) * BUCKET_MS as u32;
                break;
            }
        }
        (upper + DEPTH_MARGIN_MS).clamp(DEPTH_MIN_MS, DEPTH_MAX_MS)
    }
}

/// What the playout did with one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Normal,
    /// A pause lengthened by 20 ms.
    PauseStretch,
    /// A pause shortened by 20 ms.
    PauseShrink,
    /// Voiced speech lengthened by one pitch period of this many samples.
    VoicedStretch(usize),
    /// Voiced speech shortened by one pitch period of this many samples.
    VoicedShrink(usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayoutCounters {
    pub stretched_pause_ms: u64,
    pub stretched_speech_ms: u64,
    pub removed_ms: u64,
    pub bridged_frames: u64,
    pub speech_gaps: u64,
    pub depth_ms: u64,
    pub depth_max_ms: u64,
}

/// The decoded samples and the per-frame decision (design D3, D4).
pub struct Playout {
    adaptive: bool,
    /// History (`..pos`) and decoded samples not yet played (`pos..`).
    buf: Vec<i16>,
    pos: usize,
    /// Samples taken from the stream so far, and samples pushed into it.
    consumed: u64,
    pushed: u64,
    /// Where each decoded packet starts in the stream, and when it arrived.
    arrivals: [Option<(u64, Instant)>; ARRIVAL_SLOTS],
    arrival_next: usize,
    current_arrival: Option<Instant>,
    estimator: DepthEstimator,
    speech_level: f32,
    last_voiced: bool,
    since_voiced_op: u32,
    since_pause: u32,
    pause_stretch_run: u32,
    decimated: [f32; SEARCH_SPAN],
    counters: PlayoutCounters,
}

impl Playout {
    /// `adaptive: false` plays every frame unchanged (the replay stand's baseline). The
    /// depth starts empty, at the minimum.
    pub fn new(adaptive: bool) -> Self {
        Self::with_estimator(adaptive, DepthEstimator::new())
    }

    /// A playout whose depth starts from `estimator` (a prior, or what an earlier call
    /// learnt).
    pub fn with_estimator(adaptive: bool, estimator: DepthEstimator) -> Self {
        let mut buf = Vec::with_capacity(CAPACITY);
        buf.resize(HISTORY, 0);
        Self {
            adaptive,
            buf,
            pos: HISTORY,
            consumed: 0,
            pushed: 0,
            arrivals: [None; ARRIVAL_SLOTS],
            arrival_next: 0,
            current_arrival: None,
            estimator,
            speech_level: 3_000.0,
            last_voiced: false,
            since_voiced_op: FRAMES_BETWEEN_VOICED,
            since_pause: 0,
            pause_stretch_run: 0,
            decimated: [0.0; SEARCH_SPAN],
            counters: PlayoutCounters::default(),
        }
    }

    pub fn is_adaptive(&self) -> bool {
        self.adaptive
    }

    /// Empties the samples (a flush); the depth estimator keeps what it learned.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.buf.resize(HISTORY, 0);
        self.pos = HISTORY;
        self.arrivals = [None; ARRIVAL_SLOTS];
        self.arrival_next = 0;
        self.current_arrival = None;
        self.last_voiced = false;
        self.since_voiced_op = FRAMES_BETWEEN_VOICED;
        self.since_pause = 0;
        self.pause_stretch_run = 0;
        self.estimator.restart();
    }

    /// Notes a packet from the network: the depth estimate, and that the stream is still
    /// delivering, so pauses may be stretched again.
    pub fn on_packet(&mut self, arrived: Instant, timestamp: u32) {
        self.estimator.on_packet(arrived, timestamp);
        self.pause_stretch_run = 0;
    }

    pub fn depth_ms(&self) -> u32 {
        self.estimator.depth_ms()
    }

    /// What the depth estimator learnt, to start the next call from.
    pub fn learned(&self) -> DepthEstimator {
        self.estimator.learned()
    }

    /// Decoded samples not yet played.
    pub fn future_len(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Whether more decoded samples would help the decision.
    pub fn wants_more(&self) -> bool {
        self.future_len() < LOOKAHEAD
    }

    /// Whether the last frame played was speech (for bridging and gap counting).
    pub fn in_speech(&self) -> bool {
        self.last_voiced
    }

    pub fn arrival(&self) -> Option<Instant> {
        self.current_arrival
    }

    pub fn counters(&self) -> PlayoutCounters {
        self.counters
    }

    pub fn note_bridged_frame(&mut self) {
        self.counters.bridged_frames += 1;
    }

    pub fn note_speech_gap(&mut self) {
        self.counters.speech_gaps += 1;
    }

    /// Makes room for `samples` more, dropping old history; false when there is none.
    pub fn has_room(&mut self, samples: usize) -> bool {
        if self.buf.len() + samples > CAPACITY {
            self.compact();
        }
        self.buf.len() + samples <= CAPACITY
    }

    /// Appends decoded samples. `arrived` is the packet's arrival; concealment passes `None`
    /// and keeps the previous one for the latency line.
    pub fn push(&mut self, samples: &[i16], arrived: Option<Instant>) {
        if !self.has_room(samples.len()) {
            return;
        }
        if let Some(arrived) = arrived {
            if let Some(slot) = self.arrivals.get_mut(self.arrival_next) {
                *slot = Some((self.pushed, arrived));
            }
            self.arrival_next = (self.arrival_next + 1) % ARRIVAL_SLOTS;
        }
        self.buf.extend_from_slice(samples);
        self.pushed += samples.len() as u64;
    }

    /// Plays one 20 ms frame into `out` (960 mono samples). `held` is the audio still in
    /// packets, in samples. Returns `None` when fewer than 20 ms are decoded.
    pub fn pull(&mut self, held: usize, out: &mut [i16]) -> Option<Operation> {
        if out.len() < FRAME || self.future_len() < FRAME {
            return None;
        }
        let depth = self.estimator.depth_ms();
        self.counters.depth_ms = u64::from(depth);
        self.counters.depth_max_ms = self.counters.depth_max_ms.max(u64::from(depth));
        let threshold = self.pause_threshold();
        let head_is_pause = self.rms(0, FRAME).is_some_and(|rms| rms < threshold);
        if head_is_pause {
            self.since_pause = 0;
        } else {
            self.since_pause = self.since_pause.saturating_add(1);
        }
        self.since_voiced_op = self.since_voiced_op.saturating_add(1);
        let operation = if self.adaptive {
            let level = self.future_len() + held;
            let depth = depth as usize * PER_MS;
            // Speech is changed only when waiting for a pause is not enough: the buffer is
            // under half the depth (or over twice it), and no pause has come for a while.
            let voiced_allowed = !head_is_pause
                && self.since_pause >= FRAMES_WITHOUT_PAUSE
                && self.since_voiced_op >= FRAMES_BETWEEN_VOICED;
            if level + LOW_MARGIN < depth {
                if head_is_pause && self.pause_stretch_run < MAX_PAUSE_STRETCH_RUN {
                    Operation::PauseStretch
                } else if voiced_allowed && level * 2 < depth {
                    self.period(false)
                        .map_or(Operation::Normal, Operation::VoicedStretch)
                } else {
                    Operation::Normal
                }
            } else if level > depth + HIGH_MARGIN {
                if head_is_pause && self.pause_ahead(threshold) {
                    Operation::PauseShrink
                } else if voiced_allowed && level > depth * 2 {
                    self.period(true)
                        .map_or(Operation::Normal, Operation::VoicedShrink)
                } else {
                    Operation::Normal
                }
            } else {
                Operation::Normal
            }
        } else {
            Operation::Normal
        };
        let operation = if self.render(operation, out) {
            operation
        } else {
            // A splice that does not fit the buffer plays the frame unchanged.
            self.render(Operation::Normal, out);
            Operation::Normal
        };
        if operation == Operation::PauseStretch {
            self.pause_stretch_run += 1;
        }
        match operation {
            Operation::Normal => {}
            Operation::PauseStretch => self.counters.stretched_pause_ms += 20,
            Operation::PauseShrink => self.counters.removed_ms += 20,
            Operation::VoicedStretch(period) => {
                self.counters.stretched_speech_ms += (period / PER_MS) as u64;
                self.since_voiced_op = 0;
            }
            Operation::VoicedShrink(period) => {
                self.counters.removed_ms += (period / PER_MS) as u64;
                self.since_voiced_op = 0;
            }
        }
        let consumed = match operation {
            Operation::Normal => FRAME,
            Operation::PauseStretch => 0,
            Operation::PauseShrink => 2 * FRAME,
            Operation::VoicedStretch(period) => FRAME - period,
            Operation::VoicedShrink(period) => FRAME + period,
        };
        self.pos = (self.pos + consumed).min(self.buf.len());
        self.consumed += consumed as u64;
        self.update_arrival();
        let played = rms_of(out.get(..FRAME).unwrap_or(&[]));
        self.last_voiced = played >= threshold;
        if self.last_voiced && played > self.speech_level * 0.1 {
            self.speech_level = 0.98 * self.speech_level + 0.02 * played;
        }
        if self.pos > HISTORY * 2 {
            self.compact();
        }
        Some(operation)
    }

    /// Writes the frame for `operation`; false when the samples it needs are not there.
    fn render(&self, operation: Operation, out: &mut [i16]) -> bool {
        let Some(out) = out.get_mut(..FRAME) else {
            return false;
        };
        match operation {
            Operation::Normal => match self.segment(0, FRAME) {
                Some(head) => {
                    out.copy_from_slice(head);
                    true
                }
                None => false,
            },
            // Play the pause again: fade its end into the samples before it, so the next
            // frame (the same pause) continues smoothly.
            Operation::PauseStretch => {
                self.splice(out, FRAME - PAUSE_FADE, -(PAUSE_FADE as isize), PAUSE_FADE)
            }
            // Skip 20 ms of the pause.
            Operation::PauseShrink => self.splice(
                out,
                FRAME - PAUSE_FADE,
                (2 * FRAME - PAUSE_FADE) as isize,
                PAUSE_FADE,
            ),
            // Repeat the last period: fade into the period before it.
            Operation::VoicedStretch(period) => self.splice(
                out,
                FRAME - period,
                FRAME as isize - 2 * period as isize,
                period,
            ),
            // Skip one period: fade into the period after the frame.
            Operation::VoicedShrink(period) => {
                self.splice(out, FRAME - period, FRAME as isize, period)
            }
        }
    }

    /// `out[..at]` is the head as it is; `out[at..at + fade]` fades from the head into the
    /// samples starting at `from` (relative to the read position).
    fn splice(&self, out: &mut [i16], at: usize, from: isize, fade: usize) -> bool {
        if at + fade != FRAME || out.len() != FRAME {
            return false;
        }
        let (Some(head), Some(tail)) = (self.segment(0, FRAME), self.segment(from, fade)) else {
            return false;
        };
        let (Some(head_keep), Some(head_fade)) = (head.get(..at), head.get(at..)) else {
            return false;
        };
        let (keep, fading) = out.split_at_mut(at);
        keep.copy_from_slice(head_keep);
        for (k, (slot, (a, b))) in fading
            .iter_mut()
            .zip(head_fade.iter().zip(tail.iter()))
            .enumerate()
        {
            let weight = (k as f32 + 0.5) / fade as f32;
            let mixed = f32::from(*a) * (1.0 - weight) + f32::from(*b) * weight;
            *slot = mixed.round().clamp(-32_768.0, 32_767.0) as i16;
        }
        true
    }

    /// `len` samples starting `from` samples after the read position (negative: history).
    fn segment(&self, from: isize, len: usize) -> Option<&[i16]> {
        segment_of(&self.buf, self.pos, from, len)
    }

    fn rms(&self, from: isize, len: usize) -> Option<f32> {
        self.segment(from, len).map(rms_of)
    }

    fn pause_threshold(&self) -> f32 {
        (self.speech_level * 0.031_6).max(40.0)
    }

    /// Whether the pause goes on for [`PAUSE_AHEAD`] from the read position.
    fn pause_ahead(&self, threshold: f32) -> bool {
        (0..PAUSE_AHEAD / FRAME).all(|index| {
            self.rms((index * FRAME) as isize, FRAME)
                .is_some_and(|rms| rms < threshold)
        })
    }

    /// The pitch period at the end of the head frame, if the signal is clearly periodic:
    /// a search on the 4 kHz decimation, refined at 48 kHz. `forward` compares the last
    /// period of the frame with the one after it (for shrinking) instead of the one before.
    fn period(&mut self, forward: bool) -> Option<usize> {
        let span = SEARCH_SPAN * DECIMATE;
        let source = segment_of(&self.buf, self.pos, FRAME as isize - span as isize, span)?;
        for (slot, chunk) in self
            .decimated
            .iter_mut()
            .zip(source.as_chunks::<DECIMATE>().0)
        {
            *slot = chunk.iter().map(|s| f32::from(*s)).sum::<f32>() / DECIMATE as f32;
        }
        let window = self.decimated.get(SEARCH_SPAN - SEARCH_WINDOW..)?;
        let first_lag = PERIOD_MIN / DECIMATE;
        let mut correlations = [0.0f32; PERIOD_MAX / DECIMATE + 1];
        let mut best = f32::MIN;
        for lag in first_lag..=PERIOD_MAX / DECIMATE {
            let other = self
                .decimated
                .get(SEARCH_SPAN - SEARCH_WINDOW - lag..SEARCH_SPAN - lag)?;
            let correlation = normalized(window, other);
            if let Some(slot) = correlations.get_mut(lag) {
                *slot = correlation;
            }
            best = best.max(correlation);
        }
        if best < MIN_CORRELATION {
            return None;
        }
        // A periodic signal also matches at two or three periods: take the shortest lag
        // that matches almost as well as the best one.
        let lag = (first_lag..=PERIOD_MAX / DECIMATE)
            .find(|lag| correlations.get(*lag).is_some_and(|c| *c >= best - 0.05))?;
        let center = lag * DECIMATE;
        let mut refined = (0usize, f32::MIN);
        for period in
            center.saturating_sub(DECIMATE).max(PERIOD_MIN)..=(center + DECIMATE).min(PERIOD_MAX)
        {
            let last = self.segment(FRAME as isize - period as isize, period)?;
            let other = if forward {
                self.segment(FRAME as isize, period)?
            } else {
                self.segment(FRAME as isize - 2 * period as isize, period)?
            };
            let correlation = normalized(last, other);
            if correlation > refined.1 {
                refined = (period, correlation);
            }
        }
        (refined.1 >= MIN_CORRELATION).then_some(refined.0)
    }

    fn update_arrival(&mut self) {
        let mut best: Option<(u64, Instant)> = None;
        for (start, arrived) in self.arrivals.iter().flatten() {
            if *start <= self.consumed && best.is_none_or(|(b, _)| *start >= b) {
                best = Some((*start, *arrived));
            }
        }
        if let Some((_, arrived)) = best {
            self.current_arrival = Some(arrived);
        }
    }

    /// Drops history beyond [`HISTORY`] samples before the read position.
    fn compact(&mut self) {
        if self.pos > HISTORY {
            let drop = self.pos - HISTORY;
            self.buf.drain(..drop);
            self.pos = HISTORY;
        }
    }
}

/// `len` samples of `buf` starting `from` samples after `pos` (negative: before it).
fn segment_of(buf: &[i16], pos: usize, from: isize, len: usize) -> Option<&[i16]> {
    let start = isize::try_from(pos).ok()?.checked_add(from)?;
    let start = usize::try_from(start).ok()?;
    buf.get(start..start.checked_add(len)?)
}

fn rms_of<T: Copy + Into<f32>>(samples: &[T]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples
        .iter()
        .map(|s| {
            let v: f32 = (*s).into();
            v * v
        })
        .sum();
    (sum / samples.len() as f32).sqrt()
}

fn normalized<T: Copy + Into<f32>>(a: &[T], b: &[T]) -> f32 {
    let (mut ab, mut aa, mut bb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b.iter()) {
        let (x, y): (f32, f32) = ((*x).into(), (*y).into());
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa <= f32::EPSILON || bb <= f32::EPSILON {
        return 0.0;
    }
    ab / (aa * bb).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `count` packets of 20 ms: arrival `i * 20 ms` plus `extra(i)` ms.
    fn feed(
        estimator: &mut DepthEstimator,
        start: Instant,
        first: usize,
        count: usize,
        extra: impl Fn(usize) -> u64,
    ) {
        for i in first..first + count {
            let at = start + Duration::from_millis(i as u64 * 20 + extra(i));
            estimator.on_packet(at, (i * FRAME) as u32);
        }
    }

    /// A little jitter, 0 to 9 ms.
    fn jitter(i: usize) -> u64 {
        (i as u64).wrapping_mul(2_654_435_761) % 10
    }

    #[test]
    fn a_steady_server_keeps_the_minimum_depth() {
        let mut estimator = DepthEstimator::new();
        feed(&mut estimator, Instant::now(), 0, 1_500, jitter);
        assert_eq!(estimator.depth_ms(), DEPTH_MIN_MS);
    }

    #[test]
    fn repeated_stalls_grow_the_depth_and_a_calm_period_brings_it_back() {
        let mut estimator = DepthEstimator::new();
        let start = Instant::now();
        // A 200 ms stall every 100 packets (2 s), five times; the server never catches up.
        let stalled = |i: usize| (i / 100) as u64 * 200 + jitter(i);
        feed(&mut estimator, start, 0, 500, stalled);
        let after_stalls = estimator.depth_ms();
        assert!(
            (200..=DEPTH_MAX_MS).contains(&after_stalls),
            "depth {after_stalls} ms after 200 ms stalls"
        );
        // 30 s without a stall.
        feed(&mut estimator, start, 500, 1_500, |i| 4 * 200 + jitter(i));
        let calm = estimator.depth_ms();
        assert!(calm < after_stalls, "{calm} ms after the calm period");
        assert!(calm <= 100, "{calm} ms after the calm period");
    }

    #[test]
    fn the_prior_starts_at_200_ms_and_fades_on_a_steady_server() {
        let mut estimator = DepthEstimator::with_prior();
        assert_eq!(estimator.depth_ms(), 200);
        let start = Instant::now();
        feed(&mut estimator, start, 0, 250, jitter);
        assert_eq!(estimator.depth_ms(), 200, "still there after 5 s");
        feed(&mut estimator, start, 250, 1_250, jitter);
        assert_eq!(estimator.depth_ms(), DEPTH_MIN_MS, "gone after 30 s");
    }

    #[test]
    fn what_was_learnt_is_kept_without_the_call_s_clock() {
        let mut estimator = DepthEstimator::new();
        let start = Instant::now();
        feed(&mut estimator, start, 0, 500, |i| (i / 100) as u64 * 200);
        let learnt = estimator.learned();
        assert_eq!(learnt.depth_ms(), estimator.depth_ms());
        assert!(learnt.reference.is_none() && learnt.window_len == 0);
    }

    #[test]
    fn the_depth_never_exceeds_the_ceiling() {
        let mut estimator = DepthEstimator::new();
        feed(&mut estimator, Instant::now(), 0, 600, |i| {
            (i / 50) as u64 * 900
        });
        assert!(estimator.depth_ms() <= DEPTH_MAX_MS);
    }

    /// A sustained vowel: 140 Hz with harmonics, continuing from sample `from`.
    fn vowel(from: usize, len: usize) -> Vec<i16> {
        (from..from + len)
            .map(|n| {
                let p = 2.0 * std::f32::consts::PI * 140.0 * n as f32 / 48_000.0;
                (5_000.0 * p.sin() + 2_500.0 * (2.0 * p).sin() + 1_200.0 * (3.0 * p).sin()) as i16
            })
            .collect()
    }

    /// The fundamental of `samples` from the autocorrelation peak between 2.5 and 15 ms.
    fn fundamental(samples: &[i16]) -> f32 {
        let window = &samples[..samples.len() - PERIOD_MAX];
        let best = (PERIOD_MIN..=PERIOD_MAX)
            .map(|lag| {
                let other = &samples[lag..lag + window.len()];
                (lag, normalized(window, other))
            })
            .fold(
                (0, f32::MIN),
                |best, item| if item.1 > best.1 { item } else { best },
            );
        48_000.0 / best.0 as f32
    }

    /// Pulls frames of the vowel, `ahead` frames decoded first and one more pushed before
    /// each pull, with `held` samples in packets; returns the operations and the frames.
    fn play_vowel(
        playout: &mut Playout,
        ahead: usize,
        held: usize,
        pulls: usize,
    ) -> (Vec<Operation>, Vec<i16>) {
        let mut operations = Vec::new();
        let mut played = Vec::new();
        let mut written = 0;
        let mut out = [0i16; FRAME];
        playout.push(&vowel(written, FRAME * ahead), None);
        written += FRAME * ahead;
        for _ in 0..pulls {
            playout.push(&vowel(written, FRAME), None);
            written += FRAME;
            out.fill(i16::MAX);
            let operation = playout.pull(held, &mut out).expect("a frame");
            assert!(
                !out.contains(&i16::MAX),
                "the whole frame is written ({operation:?})"
            );
            operations.push(operation);
            played.extend_from_slice(&out);
        }
        (operations, played)
    }

    #[test]
    fn a_stretched_vowel_keeps_its_pitch() {
        let mut playout = Playout::new(true);
        let (operations, played) = play_vowel(&mut playout, 0, 0, 40);
        let index = operations
            .iter()
            .position(|op| matches!(op, Operation::VoicedStretch(_)))
            .expect("a voiced stretch below the depth");
        let Operation::VoicedStretch(period) = operations[index] else {
            unreachable!()
        };
        assert!((331..=355).contains(&period), "period {period} for 140 Hz");
        let around = &played[index * FRAME..(index + 3).min(operations.len()) * FRAME];
        let f0 = fundamental(around);
        assert!(
            (f0 - 140.0).abs() <= 140.0 * 0.02,
            "f0 {f0} Hz after the stretch"
        );
        assert!(playout.counters().stretched_speech_ms > 0);
        let gaps = operations
            .windows(2)
            .filter(|pair| {
                matches!(pair[0], Operation::VoicedStretch(_))
                    && matches!(pair[1], Operation::VoicedStretch(_))
            })
            .count();
        assert_eq!(gaps, 0, "at most one voiced stretch per 60 ms");
    }

    #[test]
    fn a_shrunk_vowel_keeps_its_pitch() {
        let mut playout = Playout::new(true);
        let (operations, played) = play_vowel(&mut playout, 2, 40 * FRAME, 40);
        let index = operations
            .iter()
            .position(|op| matches!(op, Operation::VoicedShrink(_)))
            .expect("a voiced shrink above the depth");
        let around = &played[index * FRAME..(index + 3).min(operations.len()) * FRAME];
        let f0 = fundamental(around);
        assert!(
            (f0 - 140.0).abs() <= 140.0 * 0.02,
            "f0 {f0} Hz after the shrink"
        );
        assert!(playout.counters().removed_ms > 0);
    }

    #[test]
    fn noise_gets_no_voiced_stretch() {
        let mut playout = Playout::new(true);
        let mut seed = 11u32;
        let mut noise = |len: usize| -> Vec<i16> {
            (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    ((seed >> 16) as i16 as i32 / 4) as i16
                })
                .collect()
        };
        playout.push(&noise(FRAME * 2), None);
        let mut out = [0i16; FRAME];
        for _ in 0..40 {
            playout.push(&noise(FRAME), None);
            let operation = playout.pull(0, &mut out).expect("a frame");
            assert!(
                !matches!(operation, Operation::VoicedStretch(_)),
                "noise has no pitch"
            );
        }
    }

    #[test]
    fn a_pause_is_lengthened_below_the_depth() {
        let mut playout = Playout::new(true);
        playout.push(&[0; FRAME], None);
        let mut out = [1i16; FRAME];
        assert_eq!(playout.pull(0, &mut out), Some(Operation::PauseStretch));
        assert_eq!(playout.future_len(), FRAME, "the pause is still to play");
        assert_eq!(playout.counters().stretched_pause_ms, 20);
    }

    #[test]
    fn a_long_pause_is_shortened_above_the_depth_but_keeps_60_ms() {
        let mut playout = Playout::new(true);
        playout.push(&[0; FRAME * 10], None);
        let mut out = [1i16; FRAME];
        assert_eq!(
            playout.pull(30 * FRAME, &mut out),
            Some(Operation::PauseShrink)
        );
        assert_eq!(playout.counters().removed_ms, 20);
        assert_eq!(playout.future_len(), FRAME * 8);

        // A pause of 60 ms before speech is kept whole.
        let mut playout = Playout::new(true);
        playout.push(&[0; FRAME * 3], None);
        playout.push(&vowel(0, FRAME * 5), None);
        assert_eq!(playout.pull(30 * FRAME, &mut out), Some(Operation::Normal));
    }

    #[test]
    fn the_baseline_plays_every_frame_unchanged() {
        let mut playout = Playout::new(false);
        let (operations, _) = play_vowel(&mut playout, 0, 0, 20);
        assert!(operations.iter().all(|op| *op == Operation::Normal));
        playout.push(&[0; FRAME], None);
        let mut out = [0i16; FRAME];
        assert!(playout.pull(0, &mut out).is_some());
        assert_eq!(playout.counters().stretched_pause_ms, 0);
    }
}
