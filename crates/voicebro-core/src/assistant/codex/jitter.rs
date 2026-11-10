//! The receive jitter buffer for the assistant's voice: RTP packets by sequence number,
//! pulled one 20 ms frame at a time by the output on songbird's mixer clock. It reorders,
//! drops late packets, recovers a lost packet from the next one's in-band FEC or conceals it,
//! and tells a pause (contiguous sequence numbers, the server stopped sending) from loss.
//!
//! Decoded samples go to the [`Playout`], which keeps the voice continuous when the server
//! is late: its depth follows the measured lateness, it stretches and shrinks pauses first
//! and voiced speech by one pitch period, and a dry buffer in the middle of speech is bridged
//! with Opus concealment for up to 60 ms (change `codex-adaptive-playout`).

use super::playout::{DepthEstimator, Playout, FRAME as PLAYOUT_FRAME};
use crate::audio::{Frame, FrameProvider, FRAME_SAMPLES};
use bytes::Bytes;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

/// Mono samples per 20 ms at 48 kHz.
const MONO_FRAME: usize = FRAME_SAMPLES / 2;
const _: () = assert!(MONO_FRAME == PLAYOUT_FRAME);
/// Concealed frames in a row before the buffer skips to the next packet it holds.
const MAX_CONCEALED_RUN: usize = 5;
/// Packets held at most (1 s, twice the deepest playout); beyond that the oldest is dropped.
const MAX_PACKETS: usize = 50;
/// The longest Opus packet: 120 ms at 48 kHz.
const MAX_PACKET_SAMPLES: usize = 5_760;
/// Frames bridged with concealment when the buffer runs dry in speech (60 ms).
const MAX_BRIDGE_FRAMES: u32 = 3;
/// Packets decoded ahead of time in one pull at most.
const DECODE_AHEAD_PER_PULL: usize = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct JitterCounters {
    pub packets_received: u64,
    pub late: u64,
    /// Frames reconstructed for lost packets (FEC or concealment).
    pub concealed: u64,
    /// Of those, frames recovered from the next packet's in-band FEC.
    pub concealed_fec: u64,
    /// The buffer ran dry in the middle of a stream (the next packet was continuous).
    pub playout_underruns: u64,
    /// Packets dropped because the buffer was full.
    pub overflow_drops: u64,
    pub decode_errors: u64,
    /// Time added by lengthening pauses, in ms.
    pub stretched_pause_ms: u64,
    /// Time added by lengthening voiced speech, in ms.
    pub stretched_speech_ms: u64,
    /// Time removed from pauses and voiced speech, in ms.
    pub removed_ms: u64,
    /// Frames bridged with concealment while the buffer was dry in speech.
    pub bridged_frames: u64,
    /// Dry spells in speech that bridging could not cover: heard as a gap.
    pub speech_gaps: u64,
    /// The playout depth now, and the highest it reached, in ms.
    pub depth_ms: u64,
    pub depth_max_ms: u64,
}

struct Held {
    timestamp: u32,
    payload: Bytes,
    /// When the packet came from the network, for the turn latency.
    arrived: Instant,
}

struct State {
    decoder: Option<opus2::Decoder>,
    packets: BTreeMap<u64, Held>,
    highest: Option<u64>,
    next_seq: Option<u64>,
    /// Waiting for the depth before (re)starting: at the start, and after a dry spell
    /// outside speech.
    priming: bool,
    concealed_run: usize,
    /// Mono samples of the last decoded packet: the size of a concealed frame.
    last_duration: usize,
    /// Timestamp the next packet should carry when the stream is continuous.
    expected_ts: Option<u32>,
    /// `expected_ts` at the moment the buffer ran dry.
    dry_at: Option<u32>,
    /// Frames bridged in the current dry spell; whether silence had to follow them (a gap
    /// if the stream goes on, the end of the answer if it does not).
    bridged_run: u32,
    gap_pending: bool,
    scratch: Vec<i16>,
    mono: [i16; MONO_FRAME],
    playout: Playout,
    counters: JitterCounters,
}

pub struct JitterBuffer {
    state: Mutex<State>,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl JitterBuffer {
    /// The adaptive playout of a call with nothing learnt yet: it starts at 200 ms.
    pub fn new() -> Self {
        Self::with_estimator(true, DepthEstimator::with_prior())
    }

    /// `false` plays every frame as decoded, waits for the depth again after any dry spell
    /// and never bridges: the replay stand's baseline. The depth starts empty, at 60 ms.
    pub fn with_adaptive_playout(adaptive: bool) -> Self {
        Self::with_estimator(adaptive, DepthEstimator::new())
    }

    /// A buffer whose depth starts from `estimator`: the prior, or what the previous call
    /// learnt ([`Self::learned`]).
    pub fn with_estimator(adaptive: bool, estimator: DepthEstimator) -> Self {
        Self {
            state: Mutex::new(State {
                decoder: opus2::Decoder::new(48_000, opus2::Channels::Mono).ok(),
                packets: BTreeMap::new(),
                highest: None,
                next_seq: None,
                priming: true,
                concealed_run: 0,
                last_duration: MONO_FRAME,
                expected_ts: None,
                dry_at: None,
                bridged_run: 0,
                gap_pending: false,
                scratch: vec![0; MAX_PACKET_SAMPLES],
                mono: [0; MONO_FRAME],
                playout: Playout::with_estimator(adaptive, estimator),
                counters: JitterCounters::default(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("jitter buffer lock")
    }

    /// Stores one RTP packet. Packets older than the one due next are late and dropped.
    pub fn insert(&self, sequence: u16, timestamp: u32, payload: Bytes) {
        self.insert_at(sequence, timestamp, payload, Instant::now());
    }

    /// [`Self::insert`] with the packet's arrival time given, so a recorded call can be
    /// replayed on a simulated clock.
    pub fn insert_at(&self, sequence: u16, timestamp: u32, payload: Bytes, arrived: Instant) {
        #[cfg(test)]
        recording::record(sequence, timestamp, &payload, arrived);
        let mut state = self.lock();
        state.counters.packets_received += 1;
        state.playout.on_packet(arrived, timestamp);
        let seq = extend(state.highest, sequence);
        if state.next_seq.is_some_and(|next| seq < next) {
            state.counters.late += 1;
            return;
        }
        if state.packets.contains_key(&seq) {
            return;
        }
        state.highest = Some(state.highest.map_or(seq, |high| high.max(seq)));
        state.packets.insert(
            seq,
            Held {
                timestamp,
                payload,
                arrived,
            },
        );
        while state.packets.len() > MAX_PACKETS {
            let Some((oldest, _)) = state.packets.pop_first() else {
                break;
            };
            state.counters.overflow_drops += 1;
            if state.next_seq.is_some_and(|next| next <= oldest) {
                state.next_seq = Some(oldest + 1);
            }
        }
    }

    pub fn counters(&self) -> JitterCounters {
        let state = self.lock();
        let playout = state.playout.counters();
        JitterCounters {
            stretched_pause_ms: playout.stretched_pause_ms,
            stretched_speech_ms: playout.stretched_speech_ms,
            removed_ms: playout.removed_ms,
            bridged_frames: playout.bridged_frames,
            speech_gaps: playout.speech_gaps,
            depth_ms: u64::from(state.playout.depth_ms()),
            depth_max_ms: playout.depth_max_ms,
            ..state.counters
        }
    }

    /// The playout depth now, in ms.
    pub fn depth_ms(&self) -> u32 {
        self.lock().playout.depth_ms()
    }

    /// What the depth estimator learnt in this call, to start the next one from.
    pub fn learned(&self) -> DepthEstimator {
        self.lock().playout.learned()
    }
}

impl State {
    /// Decodes `payload` into the playout. An empty payload asks for concealment and `fec`
    /// for the redundancy of the next packet; both are `samples` long. A packet that cannot
    /// be decoded is counted and replaced by concealment. `arrived` is the packet's arrival
    /// (none for concealment).
    fn decode(&mut self, payload: &[u8], samples: usize, fec: bool, arrived: Option<Instant>) {
        let concealed_len = samples.clamp(1, MAX_PACKET_SAMPLES);
        let wanted = if payload.is_empty() || fec {
            concealed_len
        } else {
            MAX_PACKET_SAMPLES
        };
        let Some(decoder) = self.decoder.as_mut() else {
            self.counters.decode_errors += 1;
            return;
        };
        let Some(output) = self.scratch.get_mut(..wanted) else {
            return;
        };
        let count = match decoder.decode(payload, output, fec) {
            Ok(count) => {
                if !fec && !payload.is_empty() {
                    self.last_duration = count.clamp(1, MAX_PACKET_SAMPLES);
                }
                count.min(wanted)
            }
            Err(_) => {
                self.counters.decode_errors += 1;
                let len = self.last_duration.clamp(1, MAX_PACKET_SAMPLES);
                match self
                    .scratch
                    .get_mut(..len)
                    .map(|out| decoder.decode(&[], out, false))
                {
                    Some(Ok(count)) => count.min(len),
                    _ => {
                        if let Some(out) = self.scratch.get_mut(..len) {
                            out.fill(0);
                        }
                        len
                    }
                }
            }
        };
        if let Some(decoded) = self.scratch.get(..count) {
            self.playout.push(decoded, arrived);
        }
    }

    /// Decodes the packet due next into the playout. Without `conceal` it stops at a missing
    /// packet (ahead of time, a missing packet may still arrive); with it, a missing packet
    /// whose successors are held is recovered from FEC or concealed (its turn has come).
    fn decode_next(&mut self, conceal: bool) -> bool {
        let Some(seq) = self.next_seq else {
            return false;
        };
        if !self.playout.has_room(MAX_PACKET_SAMPLES) {
            return false;
        }
        if let Some(held) = self.packets.remove(&seq) {
            if let Some(dry_ts) = self.dry_at.take() {
                if held.timestamp == dry_ts {
                    // The network was late, not the speaker silent.
                    self.counters.playout_underruns += 1;
                }
            }
            if held.payload.is_empty() {
                // Nothing to decode: count it and conceal one packet's length instead.
                self.counters.decode_errors += 1;
                self.decode(&[], self.last_duration, false, Some(held.arrived));
            } else {
                self.decode(&held.payload, MAX_PACKET_SAMPLES, false, Some(held.arrived));
            }
            self.expected_ts = Some(held.timestamp.wrapping_add(self.last_duration as u32));
            self.next_seq = Some(seq + 1);
            self.concealed_run = 0;
            self.bridged_run = 0;
            if self.gap_pending {
                // The stream went on after the silence: it was a gap in the speech.
                self.gap_pending = false;
                self.playout.note_speech_gap();
            }
            return true;
        }
        if !conceal || self.packets.is_empty() {
            return false;
        }
        if self.concealed_run >= MAX_CONCEALED_RUN {
            if let Some(first) = self.packets.keys().next().copied() {
                self.next_seq = Some(first);
            }
            self.concealed_run = 0;
            return true;
        }
        let duration = self.last_duration;
        let following = self
            .packets
            .get(&(seq + 1))
            .map(|held| held.payload.clone());
        match following {
            Some(next) => {
                self.decode(&next, duration, true, None);
                self.counters.concealed_fec += 1;
            }
            None => self.decode(&[], duration, false, None),
        }
        self.counters.concealed += 1;
        self.concealed_run += 1;
        self.expected_ts = self.expected_ts.map(|ts| ts.wrapping_add(duration as u32));
        self.next_seq = Some(seq + 1);
        true
    }

    /// One 20 ms frame of concealment while the buffer is dry in speech; the packet due
    /// next stays due (design D5).
    fn bridge(&mut self) {
        self.decode(&[], MONO_FRAME, false, None);
        self.bridged_run += 1;
        self.playout.note_bridged_frame();
    }

    fn held_samples(&self) -> usize {
        self.packets.len().saturating_mul(self.last_duration)
    }

    /// Plays one frame into `out`; false when there is nothing to play.
    fn pull(&mut self, out: &mut Frame) -> bool {
        if self.priming {
            let depth = self.playout.depth_ms() as usize * (MONO_FRAME / 20);
            if self.held_samples() + self.playout.future_len() < depth {
                return false;
            }
            if self.playout.future_len() == 0 {
                // Late packets are refused on insert, so the first one held is never older
                // than the one due next; start there.
                match self.packets.keys().next().copied() {
                    Some(first) => self.next_seq = Some(first),
                    None => return false,
                }
            }
            self.priming = false;
        }
        // Ahead of time, at most two packets per pull: a burst after a stall is spread over
        // the next pulls instead of costing one read five decodes.
        let mut ahead = 0;
        while ahead < DECODE_AHEAD_PER_PULL && self.playout.wants_more() && self.decode_next(false)
        {
            ahead += 1;
        }
        while self.playout.future_len() < MONO_FRAME && self.decode_next(true) {}
        if self.playout.future_len() < MONO_FRAME {
            if self.dry_at.is_none() {
                self.dry_at = self.expected_ts;
            }
            let in_speech = self.bridged_run > 0 || self.playout.in_speech();
            let adaptive = self.playout.is_adaptive();
            if adaptive && in_speech && self.bridged_run < MAX_BRIDGE_FRAMES {
                self.bridge();
            }
            if self.playout.future_len() < MONO_FRAME {
                if adaptive && in_speech {
                    // Stay started: the late packet plays as soon as it arrives, and the
                    // silence counts as a gap once it does.
                    self.gap_pending = true;
                } else {
                    // A pause or the end of an answer: build the depth up again first.
                    self.priming = true;
                }
                return false;
            }
        }
        let held = self.held_samples();
        let State { playout, mono, .. } = self;
        if playout.pull(held, mono).is_none() {
            return false;
        }
        let (pairs, _) = out.as_mut_slice().as_chunks_mut::<2>();
        for (pair, value) in pairs.iter_mut().zip(mono.iter()) {
            *pair = [*value, *value];
        }
        true
    }
}

impl FrameProvider for JitterBuffer {
    fn next_frame(&self, out: &mut Frame) -> bool {
        self.lock().pull(out)
    }

    fn reset(&self) {
        let mut state = self.lock();
        state.packets.clear();
        state.next_seq = None;
        state.highest = None;
        state.priming = true;
        state.concealed_run = 0;
        state.expected_ts = None;
        state.dry_at = None;
        state.bridged_run = 0;
        state.gap_pending = false;
        state.playout.reset();
        state.decoder = opus2::Decoder::new(48_000, opus2::Channels::Mono).ok();
    }

    fn arrival(&self) -> Option<Instant> {
        self.lock().playout.arrival()
    }
}

/// Extends a 16-bit RTP sequence number to 64 bits around the highest one seen.
fn extend(highest: Option<u64>, sequence: u16) -> u64 {
    let Some(high) = highest else {
        return (1 << 16) + sequence as u64;
    };
    let mut ext = (high & !0xFFFF) | sequence as u64;
    if ext + 0x8000 < high {
        ext += 1 << 16;
    } else if ext > high + 0x8000 && ext >= 1 << 16 {
        ext -= 1 << 16;
    }
    ext
}

/// Records every packet a call receives (`VOICEBRO_CODEX_RECORD` in the live call test), one
/// line each: arrival in µs after the first packet, sequence, timestamp, payload in hex.
#[cfg(test)]
pub(crate) mod recording {
    use std::io::Write as _;
    use std::sync::Mutex;
    use std::time::Instant;

    struct Recorder {
        file: std::io::BufWriter<std::fs::File>,
        first: Option<Instant>,
        lines: u64,
    }

    static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

    pub fn start(path: &str) -> std::io::Result<()> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        *RECORDER.lock().unwrap() = Some(Recorder {
            file,
            first: None,
            lines: 0,
        });
        Ok(())
    }

    pub fn record(sequence: u16, timestamp: u32, payload: &[u8], arrived: Instant) {
        let mut guard = RECORDER.lock().unwrap();
        let Some(recorder) = guard.as_mut() else {
            return;
        };
        let first = *recorder.first.get_or_insert(arrived);
        let micros = arrived.saturating_duration_since(first).as_micros();
        let hex: String = payload.iter().map(|byte| format!("{byte:02x}")).collect();
        let _ = writeln!(recorder.file, "{micros} {sequence} {timestamp} {hex}");
        recorder.lines += 1;
    }

    /// Stops recording; returns the number of packets written.
    pub fn finish() -> u64 {
        let mut guard = RECORDER.lock().unwrap();
        let Some(mut recorder) = guard.take() else {
            return 0;
        };
        let _ = recorder.file.flush();
        recorder.lines
    }
}

/// Replays recorded or synthetic packet streams through a [`JitterBuffer`] on a simulated
/// clock: packets are inserted when their arrival time has passed, one 20 ms frame is pulled
/// per simulated tick. Shared by the unit tests and the gated replay of real recordings.
#[cfg(test)]
pub(crate) mod replay {
    use super::*;

    /// One packet: arrival after the start of the stream, sequence, timestamp, payload.
    #[derive(Clone)]
    pub struct Packet {
        pub at: std::time::Duration,
        pub sequence: u16,
        pub timestamp: u32,
        pub payload: Bytes,
    }

    pub fn parse(text: &str) -> Vec<Packet> {
        text.lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let micros: u64 = parts.next()?.parse().ok()?;
                let sequence: u16 = parts.next()?.parse().ok()?;
                let timestamp: u32 = parts.next()?.parse().ok()?;
                let hex = parts.next().unwrap_or("");
                let payload: Vec<u8> = (0..hex.len() / 2)
                    .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
                    .collect();
                Some(Packet {
                    at: std::time::Duration::from_micros(micros),
                    sequence,
                    timestamp,
                    payload: Bytes::from(payload),
                })
            })
            .collect()
    }

    pub struct Outcome {
        pub counters: JitterCounters,
        /// Mono samples played, one 20 ms frame per tick (silence when nothing played).
        pub played: Vec<i16>,
        /// How long each packet waited between arrival and playback, in ms.
        pub waits_ms: Vec<f64>,
        /// Holes heard inside speech: the buffer had nothing to play, with speech in the
        /// 60 ms before and in the 60 ms after.
        pub holes: u32,
        /// The silent time of those holes, in ms.
        pub hole_ms: u32,
    }

    /// RMS of a frame above which it counts as speech for the hole count.
    const VOICED_RMS: f64 = 150.0;
    /// Frames of context on each side of a dry spell (60 ms).
    const CONTEXT: usize = 3;

    pub fn run(packets: &[Packet], buffer: &JitterBuffer) -> Outcome {
        let start = Instant::now();
        let end = packets.last().map_or(std::time::Duration::ZERO, |p| p.at)
            + std::time::Duration::from_secs(2);
        let mut next = 0;
        let mut tick = std::time::Duration::ZERO;
        let mut frame = Frame::silence();
        let mut played = Vec::new();
        let mut waits_ms = Vec::new();
        let mut last_arrival = None;
        // The loudest of the last frames played, the dry spell in progress (loudest frame
        // before it, its length), and the frames after it (the spell, frames seen, loudest).
        let mut recent = [0.0f64; CONTEXT];
        let mut recent_next = 0;
        let mut dry: Option<(f64, u32)> = None;
        let mut after: Option<((f64, u32), usize, f64)> = None;
        let mut holes = 0;
        let mut hole_ms = 0;
        while tick <= end {
            while next < packets.len() && packets[next].at <= tick {
                let packet = &packets[next];
                buffer.insert_at(
                    packet.sequence,
                    packet.timestamp,
                    packet.payload.clone(),
                    start + packet.at,
                );
                next += 1;
            }
            let produced = buffer.next_frame(&mut frame);
            let mono: Vec<i16> = if produced {
                frame.as_slice().iter().step_by(2).copied().collect()
            } else {
                vec![0; MONO_FRAME]
            };
            if produced {
                if let Some(arrival) = buffer.arrival() {
                    if last_arrival != Some(arrival) {
                        last_arrival = Some(arrival);
                        let now = start + tick;
                        waits_ms.push(now.saturating_duration_since(arrival).as_secs_f64() * 1e3);
                    }
                }
            }
            let rms =
                (mono.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / MONO_FRAME as f64).sqrt();
            if !produced {
                // A new dry spell, or the previous one resumed before its context was seen.
                let spell = match (dry.take(), after.take()) {
                    (Some((before, frames)), _) => (before, frames + 1),
                    (None, Some(((before, frames), _, _))) => (before, frames + 1),
                    (None, None) => (recent.iter().copied().fold(0.0, f64::max), 1),
                };
                dry = Some(spell);
                recent = [0.0; CONTEXT];
            } else {
                if let Some(spell) = dry.take() {
                    after = Some((spell, 0, 0.0));
                }
                if let Some(((before, frames), seen, loudest)) = after.as_mut() {
                    *seen += 1;
                    *loudest = loudest.max(rms);
                    if *seen == CONTEXT {
                        if *before >= VOICED_RMS && *loudest >= VOICED_RMS {
                            holes += 1;
                            hole_ms += *frames * 20;
                        }
                        after = None;
                    }
                }
                recent[recent_next % CONTEXT] = rms;
                recent_next += 1;
            }
            played.extend_from_slice(&mono);
            tick += std::time::Duration::from_millis(20);
        }
        Outcome {
            counters: buffer.counters(),
            played,
            waits_ms,
            holes,
            hole_ms,
        }
    }

    /// p-th quantile (0..=1) of `values`.
    pub fn quantile(values: &[f64], p: f64) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }

    pub fn write_wav(path: &str, mono: &[i16]) -> std::io::Result<()> {
        let data_len = (mono.len() * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + mono.len() * 2);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in mono {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, bytes)
    }

    /// A synthetic answer: a voiced signal (a 140 Hz harmonic tone with a slow vibrato) in
    /// phrases of `phrase_ms` separated by pauses of `pause_ms`, Opus-encoded in 20 ms
    /// packets, arriving `slower` times slower than real time, with each stall in `stalls`
    /// (at that packet index, that many ms) delaying every packet after it.
    pub fn synthetic(
        seconds: u32,
        phrase_ms: u32,
        pause_ms: u32,
        slower: f64,
        stalls: &[(usize, u64)],
    ) -> Vec<Packet> {
        let mut encoder =
            opus2::Encoder::new(48_000, opus2::Channels::Mono, opus2::Application::Voip).unwrap();
        encoder.set_inband_fec(true).unwrap();
        let mut out = vec![0u8; 4000];
        let count = (seconds * 50) as usize;
        let cycle = phrase_ms + pause_ms;
        let mut phase = 0.0f64;
        (0..count)
            .map(|index| {
                let pcm: Vec<i16> = (0..MONO_FRAME)
                    .map(|i| {
                        let t_ms = (index * MONO_FRAME + i) as f64 / 48.0;
                        let voiced = (t_ms as u32 % cycle) < phrase_ms;
                        let f0 = 140.0
                            * (1.0
                                + 0.03 * (2.0 * std::f64::consts::PI * 5.0 * t_ms / 1000.0).sin());
                        phase += f0 / 48_000.0;
                        if !voiced {
                            return 0;
                        }
                        let p = 2.0 * std::f64::consts::PI * phase;
                        (6_000.0 * p.sin() + 3_000.0 * (2.0 * p).sin() + 1_500.0 * (3.0 * p).sin())
                            as i16
                    })
                    .collect();
                let written = encoder.encode(&pcm, &mut out).unwrap();
                let delay: u64 = stalls
                    .iter()
                    .filter(|(at, _)| *at <= index)
                    .map(|(_, ms)| *ms)
                    .sum();
                let nominal = index as f64 * 20.0 * slower;
                Packet {
                    at: std::time::Duration::from_micros((nominal * 1000.0) as u64)
                        + std::time::Duration::from_millis(delay),
                    sequence: index as u16,
                    timestamp: (index * MONO_FRAME) as u32,
                    payload: Bytes::copy_from_slice(&out[..written]),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::playout;
    use super::*;

    /// Opus packets of a tone, each `frame` mono samples long, with in-band FEC.
    fn packets(count: usize, frame: usize) -> Vec<Bytes> {
        let mut encoder =
            opus2::Encoder::new(48_000, opus2::Channels::Mono, opus2::Application::Voip).unwrap();
        encoder.set_inband_fec(true).unwrap();
        encoder.set_packet_loss_perc(20).unwrap();
        let mut out = vec![0u8; 4000];
        (0..count)
            .map(|index| {
                let pcm: Vec<i16> = (0..frame)
                    .map(|i| {
                        let t = (index * frame + i) as f32 / 48_000.0;
                        ((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 12_000.0) as i16
                    })
                    .collect();
                let written = encoder.encode(&pcm, &mut out).unwrap();
                Bytes::copy_from_slice(&out[..written])
            })
            .collect()
    }

    /// Decodes `payloads` in order with a fresh decoder, as stereo frames of 20 ms.
    fn reference(payloads: &[Bytes]) -> Vec<i16> {
        let mut decoder = opus2::Decoder::new(48_000, opus2::Channels::Mono).unwrap();
        let mut buffer = vec![0i16; MAX_PACKET_SAMPLES];
        let mut mono = Vec::new();
        for payload in payloads {
            let count = decoder.decode(payload, &mut buffer, false).unwrap();
            mono.extend_from_slice(&buffer[..count]);
        }
        mono
    }

    fn pull_all(buffer: &JitterBuffer) -> Vec<i16> {
        let mut mono = Vec::new();
        let mut frame = Frame::silence();
        while buffer.next_frame(&mut frame) {
            mono.extend(frame.as_slice().iter().step_by(2).copied());
        }
        mono
    }

    fn insert(buffer: &JitterBuffer, payloads: &[Bytes], seq: u16, frame: usize) {
        let ts = (seq as u32).wrapping_mul(frame as u32);
        buffer.insert(seq, ts, payloads[seq as usize].clone());
    }

    /// The packet logic tests play every frame as decoded and never bridge, so their output
    /// can be compared with a reference decode.
    fn fixed() -> JitterBuffer {
        JitterBuffer::with_adaptive_playout(false)
    }

    /// The adaptive playout starting empty (60 ms) instead of from the 200 ms prior, for
    /// tests that start playback with a few packets.
    fn adaptive_empty() -> JitterBuffer {
        JitterBuffer::with_estimator(true, DepthEstimator::new())
    }

    #[test]
    fn out_of_order_packets_play_in_order() {
        let payloads = packets(14, MONO_FRAME);
        let buffer = fixed();
        for seq in [10, 12, 11, 13] {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        let played = pull_all(&buffer);
        assert_eq!(
            played,
            reference(&payloads[10..14]),
            "decoded as 10, 11, 12, 13"
        );
        assert_eq!(buffer.counters().concealed, 0);
    }

    #[test]
    fn a_lost_packet_is_recovered_from_the_next_ones_fec() {
        let payloads = packets(6, MONO_FRAME);
        let buffer = fixed();
        for seq in [0, 1, 3, 4, 5] {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        let played = pull_all(&buffer);
        assert_eq!(played.len(), 6 * MONO_FRAME, "six frames, none missing");
        let counters = buffer.counters();
        assert_eq!(counters.concealed, 1);
        assert_eq!(counters.concealed_fec, 1);
        assert!(
            played[2 * MONO_FRAME..3 * MONO_FRAME]
                .iter()
                .any(|s| s.unsigned_abs() > 1_000),
            "the recovered frame is not silence"
        );
    }

    #[test]
    fn three_lost_packets_give_three_concealed_frames() {
        let payloads = packets(8, MONO_FRAME);
        let buffer = fixed();
        for seq in [0, 1, 5, 6, 7] {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        let played = pull_all(&buffer);
        assert_eq!(played.len(), 8 * MONO_FRAME);
        let counters = buffer.counters();
        assert_eq!(counters.concealed, 3);
        assert_eq!(
            counters.concealed_fec, 1,
            "only the last lost one has its successor"
        );
    }

    #[test]
    fn a_late_packet_is_dropped_and_counted() {
        let payloads = packets(6, MONO_FRAME);
        let buffer = fixed();
        for seq in [0, 2, 3, 4] {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        let mut frame = Frame::silence();
        for _ in 0..3 {
            assert!(buffer.next_frame(&mut frame));
        }
        insert(&buffer, &payloads, 1, MONO_FRAME);
        assert_eq!(buffer.counters().late, 1);
    }

    #[test]
    fn ten_and_forty_ms_packets_are_sliced_into_twenty_ms_frames() {
        let short = packets(6, MONO_FRAME / 2);
        let buffer = fixed();
        for seq in 0..6u16 {
            insert(&buffer, &short, seq, MONO_FRAME / 2);
        }
        assert_eq!(
            pull_all(&buffer).len(),
            3 * MONO_FRAME,
            "six 10 ms packets, three frames"
        );

        let long = packets(3, MONO_FRAME * 2);
        let buffer = fixed();
        for seq in 0..3u16 {
            insert(&buffer, &long, seq, MONO_FRAME * 2);
        }
        assert_eq!(
            pull_all(&buffer).len(),
            6 * MONO_FRAME,
            "three 40 ms packets, six frames"
        );
    }

    #[test]
    fn a_pause_is_silence_and_a_continuous_gap_is_an_underrun() {
        let payloads = packets(12, MONO_FRAME);
        let buffer = fixed();
        for seq in 0..3u16 {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        assert_eq!(pull_all(&buffer).len(), 3 * MONO_FRAME);
        let mut frame = Frame::silence();
        assert!(
            !buffer.next_frame(&mut frame),
            "dry: nothing to play, no concealment"
        );
        assert_eq!(buffer.counters().concealed, 0);
        // The stream resumes with contiguous sequence numbers and continuous timestamps:
        // the packets were late, which counts as an underrun.
        for seq in 3..6u16 {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        assert_eq!(
            buffer.counters().playout_underruns,
            0,
            "counted when playing resumes"
        );
        assert_eq!(pull_all(&buffer).len(), 3 * MONO_FRAME);
        assert_eq!(buffer.counters().playout_underruns, 1);
        // Then a real pause: the timestamps jump ahead.
        let base = 1_000_000;
        let more = packets(10, MONO_FRAME);
        for seq in 6..10u16 {
            buffer.insert(
                seq,
                base + seq as u32 * MONO_FRAME as u32,
                more[seq as usize].clone(),
            );
        }
        assert_eq!(pull_all(&buffer).len(), 4 * MONO_FRAME);
        assert_eq!(
            buffer.counters().playout_underruns,
            1,
            "a pause is not an underrun"
        );
    }

    /// Packets of `stream` inserted with their scheduled arrival, from a common start.
    fn insert_stream(buffer: &JitterBuffer, stream: &[replay::Packet], start: Instant) {
        for packet in stream {
            buffer.insert_at(
                packet.sequence,
                packet.timestamp,
                packet.payload.clone(),
                start + packet.at,
            );
        }
    }

    #[test]
    fn the_depth_follows_lateness_not_underruns() {
        // Steady packets: the depth stays at its minimum.
        let steady = replay::synthetic(10, 1_400, 300, 1.0, &[]);
        let buffer = adaptive_empty();
        insert_stream(&buffer, &steady, Instant::now());
        assert_eq!(buffer.depth_ms(), playout::DEPTH_MIN_MS);
        // Stalls of 200 ms: the depth covers them, within the ceiling.
        let stalls: Vec<(usize, u64)> = (1..6).map(|i| (i * 80, 200)).collect();
        let stalling = replay::synthetic(10, 1_400, 300, 1.0, &stalls);
        let buffer = adaptive_empty();
        insert_stream(&buffer, &stalling, Instant::now());
        let depth = buffer.depth_ms();
        assert!(
            (200..=playout::DEPTH_MAX_MS).contains(&depth),
            "depth {depth} ms after 200 ms stalls"
        );
    }

    #[test]
    fn reset_empties_the_buffer() {
        let payloads = packets(4, MONO_FRAME);
        let buffer = fixed();
        for seq in 0..4u16 {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        buffer.reset();
        let mut frame = Frame::silence();
        assert!(!buffer.next_frame(&mut frame));
    }

    /// The stand's synthetic stream: 20 s of 1.7 s phrases and 300 ms pauses (ending on a
    /// pause), 5 % slower than real time, with three 200 ms stalls in the middle of phrases.
    fn slow_stalling_stream() -> Vec<replay::Packet> {
        replay::synthetic(20, 1_700, 300, 1.05, &[(160, 200), (420, 200), (700, 200)])
    }

    /// Opus packets of silence (the end of an answer).
    fn silent_packets(count: usize) -> Vec<Bytes> {
        let mut encoder =
            opus2::Encoder::new(48_000, opus2::Channels::Mono, opus2::Application::Voip).unwrap();
        let mut out = vec![0u8; 4000];
        let pcm = vec![0i16; MONO_FRAME];
        (0..count)
            .map(|_| {
                let written = encoder.encode(&pcm, &mut out).unwrap();
                Bytes::copy_from_slice(&out[..written])
            })
            .collect()
    }

    /// Plays three frames of a tone so the buffer is in speech and dry.
    fn speaking_then_dry() -> (JitterBuffer, Vec<Bytes>) {
        let payloads = packets(8, MONO_FRAME);
        let buffer = adaptive_empty();
        for seq in 0..3u16 {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        let mut frame = Frame::silence();
        for _ in 0..3 {
            assert!(buffer.next_frame(&mut frame), "the tone plays");
        }
        (buffer, payloads)
    }

    #[test]
    fn a_short_stall_in_speech_is_bridged_and_the_late_packet_plays() {
        let (buffer, payloads) = speaking_then_dry();
        let mut frame = Frame::silence();
        for _ in 0..2 {
            assert!(buffer.next_frame(&mut frame), "bridged");
        }
        insert(&buffer, &payloads, 3, MONO_FRAME);
        assert!(buffer.next_frame(&mut frame), "packet 3 plays next");
        let counters = buffer.counters();
        assert_eq!(counters.bridged_frames, 2);
        assert_eq!(counters.speech_gaps, 0);
        assert_eq!(counters.late, 0, "a bridged packet is not late");
        assert_eq!(counters.concealed, 0, "a stall is not a loss");
    }

    #[test]
    fn a_long_stall_in_speech_is_bridged_then_counted_as_one_gap() {
        let (buffer, payloads) = speaking_then_dry();
        let mut frame = Frame::silence();
        for _ in 0..3 {
            assert!(buffer.next_frame(&mut frame), "bridged");
        }
        for _ in 0..12 {
            assert!(!buffer.next_frame(&mut frame), "silence after 60 ms");
        }
        insert(&buffer, &payloads, 3, MONO_FRAME);
        assert!(
            buffer.next_frame(&mut frame),
            "packet 3 plays as soon as it arrives"
        );
        let counters = buffer.counters();
        assert_eq!(counters.bridged_frames, 3);
        assert_eq!(counters.speech_gaps, 1);
        assert_eq!(counters.late, 0);
        assert_eq!(counters.concealed, 0);
    }

    #[test]
    fn the_end_of_an_answer_is_neither_bridged_nor_a_gap() {
        let tone = packets(3, MONO_FRAME);
        let silence = silent_packets(4);
        let buffer = adaptive_empty();
        for seq in 0..3u16 {
            insert(&buffer, &tone, seq, MONO_FRAME);
        }
        for (index, payload) in silence.iter().enumerate() {
            let seq = 3 + index as u16;
            buffer.insert(seq, seq as u32 * MONO_FRAME as u32, payload.clone());
        }
        let played = pull_all(&buffer);
        assert!(
            played.len() >= 7 * MONO_FRAME,
            "the tone and the silence play"
        );
        let counters = buffer.counters();
        assert_eq!(counters.bridged_frames, 0);
        assert_eq!(counters.speech_gaps, 0);
    }

    #[test]
    fn unusual_and_broken_packets_never_stop_the_playout() {
        let mut encoder =
            opus2::Encoder::new(48_000, opus2::Channels::Mono, opus2::Application::Voip).unwrap();
        let mut out = vec![0u8; 4000];
        let mut payloads: Vec<(Bytes, usize)> = Vec::new();
        for samples in [120usize, 480, 1_920, 2_880, 5_760] {
            let pcm: Vec<i16> = (0..samples)
                .map(|i| ((i as f32 * 0.07).sin() * 9_000.0) as i16)
                .collect();
            if let Ok(written) = encoder.encode(&pcm, &mut out) {
                payloads.push((Bytes::copy_from_slice(&out[..written]), samples));
            }
        }
        let mut seed = 7u32;
        let random: Vec<u8> = (0..60)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (seed >> 16) as u8
            })
            .collect();
        let truncated = payloads[2].0.slice(..1);
        payloads.push((Bytes::new(), MONO_FRAME));
        payloads.push((truncated, MONO_FRAME));
        payloads.push((Bytes::from(random), MONO_FRAME));
        payloads.push((Bytes::from_static(&[0xff; 3]), MONO_FRAME));
        let decodable: usize = payloads.iter().take(5).map(|(_, samples)| samples).sum();
        let buffer = adaptive_empty();
        let mut timestamp = 0u32;
        for (seq, (payload, samples)) in payloads.iter().enumerate() {
            buffer.insert(seq as u16, timestamp, payload.clone());
            timestamp = timestamp.wrapping_add(*samples as u32);
        }
        let played = pull_all(&buffer);
        assert!(
            played.len() >= decodable / MONO_FRAME * MONO_FRAME,
            "the decodable packets play: {} of {decodable} samples",
            played.len()
        );
        assert!(
            buffer.counters().decode_errors >= 1,
            "the empty packet is a decode error"
        );
    }

    fn report(name: &str, outcome: &replay::Outcome) {
        let c = outcome.counters;
        eprintln!(
            "{name}: holes {} ({} ms), gaps {}, bridged {}, stretched pause {} ms, speech {} ms, removed {} ms, depth max {} ms",
            outcome.holes,
            outcome.hole_ms,
            c.speech_gaps,
            c.bridged_frames,
            c.stretched_pause_ms,
            c.stretched_speech_ms,
            c.removed_ms,
            c.depth_max_ms
        );
    }

    #[test]
    fn a_stream_five_percent_slow_plays_without_gaps_mostly_in_pauses() {
        // 30 s of 1.7 s phrases and 300 ms pauses, 5 % slower than real time.
        let stream = replay::synthetic(30, 1_700, 300, 1.05, &[]);
        let outcome = replay::run(&stream, &JitterBuffer::new());
        report("5 % slow", &outcome);
        let c = outcome.counters;
        assert_eq!(outcome.holes, 0, "no hole heard");
        assert_eq!(c.speech_gaps, 0);
        assert!(
            c.stretched_pause_ms > c.stretched_speech_ms,
            "most added time is in pauses"
        );
    }

    #[test]
    fn stalls_are_absorbed_from_the_first_one() {
        // Three 200 ms stalls in phrases, the first 3.2 s in. A call starts at 200 ms
        // (design D2a), so none is heard; without stretching and bridging all are.
        let outcome = replay::run(&slow_stalling_stream(), &JitterBuffer::new());
        report("5 % slow with stalls", &outcome);
        let baseline = replay::run(&slow_stalling_stream(), &fixed());
        report("the same without stretching and bridging", &baseline);
        assert_eq!(outcome.holes, 0, "no stall is heard");
        assert_eq!(outcome.counters.speech_gaps, 0);
        assert!(baseline.holes > 0);
        assert!(
            outcome.counters.stretched_pause_ms > outcome.counters.stretched_speech_ms,
            "most added time is in pauses"
        );
    }

    #[test]
    fn the_first_stall_of_a_call_is_not_heard() {
        // One 200 ms stall 3 s in, in the middle of a phrase.
        let stream = replay::synthetic(10, 1_700, 300, 1.0, &[(150, 200)]);
        let outcome = replay::run(&stream, &JitterBuffer::new());
        report("first stall, starting at 200 ms", &outcome);
        assert_eq!(outcome.holes, 0);
        assert_eq!(outcome.counters.speech_gaps, 0);
        // Starting empty, the same stall is heard: what the prior is for.
        let empty = replay::run(&stream, &adaptive_empty());
        report("first stall, starting empty", &empty);
        assert!(empty.holes > 0);
    }

    #[test]
    fn a_call_starts_from_what_the_previous_one_learnt() {
        let first = adaptive_empty();
        let outcome = replay::run(&slow_stalling_stream(), &first);
        report("first call, starting empty", &outcome);
        let learnt = first.depth_ms();
        assert!(learnt >= 200, "the stalls were learnt: {learnt} ms");
        let next = JitterBuffer::with_estimator(true, first.learned());
        assert_eq!(next.depth_ms(), learnt, "the next call starts there");
        let outcome = replay::run(&slow_stalling_stream(), &next);
        report("next call, from what the first learnt", &outcome);
        assert_eq!(outcome.holes, 0);
        assert_eq!(outcome.counters.speech_gaps, 0);
    }

    #[test]
    fn a_burst_after_a_stall_is_played_down_to_the_depth() {
        // 400 ms of audio held back, then delivered at once.
        let mut stream = replay::synthetic(20, 1_400, 300, 1.0, &[]);
        let burst_at = stream[320].at;
        for packet in &mut stream[300..320] {
            packet.at = burst_at;
        }
        // From the 60 ms start, so most of the burst is over the depth.
        let outcome = replay::run(&stream, &adaptive_empty());
        let c = outcome.counters;
        let tail = &outcome.waits_ms[outcome.waits_ms.len().saturating_sub(100)..];
        let tail_wait = replay::quantile(tail, 0.5);
        eprintln!(
            "burst: removed {} ms, wait at the end {tail_wait:.0} ms, depth {} ms",
            c.removed_ms, c.depth_ms
        );
        assert!(c.removed_ms >= 200, "the extra delay is removed");
        assert!(
            tail_wait <= c.depth_ms as f64 + 100.0,
            "the wait comes back to the depth"
        );
    }

    #[test]
    fn without_stretching_and_bridging_a_slow_stalling_stream_has_holes() {
        let outcome = replay::run(&slow_stalling_stream(), &fixed());
        eprintln!(
            "fixed playout: holes {} ({} ms), playout underruns {}",
            outcome.holes, outcome.hole_ms, outcome.counters.playout_underruns
        );
        assert!(outcome.holes > 0, "the voice is heard breaking up");
    }

    /// The milliseconds of speech in `played` (frames with an RMS of at least 150).
    fn speech_ms(played: &[i16]) -> u64 {
        played
            .as_chunks::<MONO_FRAME>()
            .0
            .iter()
            .filter(|frame| {
                let sum: f64 = frame.iter().map(|s| (*s as f64).powi(2)).sum();
                (sum / MONO_FRAME as f64).sqrt() >= 150.0
            })
            .count() as u64
            * 20
    }

    fn describe(name: &str, packets: &[replay::Packet], outcome: &replay::Outcome) -> String {
        let c = outcome.counters;
        format!(
            "{name}: {} packets over {:.1} s; holes {} ({} ms); wait ms p50 {:.0} p95 {:.0} max {:.0}; speech {} ms; stretched pause {} ms, speech {} ms, removed {} ms; bridged {}, speech gaps {}, depth {} ms (max {}); late {}, concealed {} (FEC {}), playout underruns {}, output drops {}, decode errors {}",
            packets.len(),
            packets.last().map_or(0.0, |p| p.at.as_secs_f64()),
            outcome.holes,
            outcome.hole_ms,
            replay::quantile(&outcome.waits_ms, 0.5),
            replay::quantile(&outcome.waits_ms, 0.95),
            replay::quantile(&outcome.waits_ms, 1.0),
            speech_ms(&outcome.played),
            c.stretched_pause_ms,
            c.stretched_speech_ms,
            c.removed_ms,
            c.bridged_frames,
            c.speech_gaps,
            c.depth_ms,
            c.depth_max_ms,
            c.late,
            c.concealed,
            c.concealed_fec,
            c.playout_underruns,
            c.overflow_drops,
            c.decode_errors
        )
    }

    /// Replays recorded calls (`VOICEBRO_PLAYOUT_REPLAY=<file>[;<file>]`, written by the live
    /// call test with `VOICEBRO_CODEX_RECORD`): prints the counters, the wait per packet and
    /// the holes heard, and writes `<file>.wav`. With `VOICEBRO_PLAYOUT_BASELINE=1` the same
    /// recording is also played without stretching and bridging into `<file>.baseline.wav`.
    #[test]
    fn replay_recordings_when_enabled() {
        let Ok(files) = std::env::var("VOICEBRO_PLAYOUT_REPLAY") else {
            return;
        };
        let baseline = std::env::var("VOICEBRO_PLAYOUT_BASELINE").as_deref() == Ok("1");
        for file in files.split(';').filter(|f| !f.trim().is_empty()) {
            let file = file.trim();
            let packets = replay::parse(&std::fs::read_to_string(file).expect("recording"));
            let outcome = replay::run(&packets, &JitterBuffer::new());
            eprintln!("{}", describe(file, &packets, &outcome));
            replay::write_wav(&format!("{file}.wav"), &outcome.played).expect("wav written");
            if baseline {
                let outcome = replay::run(&packets, &fixed());
                eprintln!(
                    "{}",
                    describe(&format!("{file} (baseline)"), &packets, &outcome)
                );
                replay::write_wav(&format!("{file}.baseline.wav"), &outcome.played)
                    .expect("wav written");
            }
        }
    }

    /// The cost of one pull with the adaptive playout busy (a slow, stalling stream: pauses
    /// and speech stretched, bridging). Gated by `VOICEBRO_BENCH=1`; run in release.
    #[test]
    fn playout_pull_cost_when_enabled() {
        if std::env::var("VOICEBRO_BENCH").ok().as_deref() != Some("1") {
            return;
        }
        let stream = replay::synthetic(
            60,
            1_700,
            300,
            1.05,
            &[(400, 200), (1_300, 300), (2_200, 200)],
        );
        let measure = |buffer: &JitterBuffer| {
            let start = Instant::now();
            let mut next = 0;
            let mut frame = Frame::silence();
            let mut costs = Vec::with_capacity(4_000);
            let mut tick = std::time::Duration::ZERO;
            let end = stream.last().map_or(tick, |p| p.at) + std::time::Duration::from_secs(1);
            while tick <= end {
                while next < stream.len() && stream[next].at <= tick {
                    let p = &stream[next];
                    buffer.insert_at(p.sequence, p.timestamp, p.payload.clone(), start + p.at);
                    next += 1;
                }
                let began = Instant::now();
                let _ = buffer.next_frame(&mut frame);
                costs.push(began.elapsed().as_secs_f64() * 1e6);
                tick += std::time::Duration::from_millis(20);
            }
            costs
        };
        let describe = |name: &str, costs: &[f64], c: JitterCounters| {
            eprintln!(
                "{name}: {} pulls, mean {:.1} us, p99 {:.1} us, max {:.1} us; stretched pause {} ms, speech {} ms, removed {} ms, bridged {}",
                costs.len(),
                costs.iter().sum::<f64>() / costs.len() as f64,
                replay::quantile(costs, 0.99),
                replay::quantile(costs, 1.0),
                c.stretched_pause_ms,
                c.stretched_speech_ms,
                c.removed_ms,
                c.bridged_frames
            );
        };
        let baseline = fixed();
        let baseline_costs = measure(&baseline);
        describe(
            "decode only (no playout work)",
            &baseline_costs,
            baseline.counters(),
        );
        let buffer = adaptive_empty();
        let costs = measure(&buffer);
        describe("adaptive playout", &costs, buffer.counters());
        let mean = costs.iter().sum::<f64>() / costs.len() as f64;
        assert!(mean <= 50.0, "mean {mean:.1} us per pull");
        assert!(
            replay::quantile(&costs, 0.99) <= 200.0,
            "p99 pull over 200 us"
        );
    }

    #[test]
    fn sequence_numbers_wrap_around() {
        let high = extend(None, 65_534);
        let next = extend(Some(high), 1);
        assert_eq!(next, high + 3);
        let back = extend(Some(next), 65_535);
        assert_eq!(back, high + 1);
    }
}
