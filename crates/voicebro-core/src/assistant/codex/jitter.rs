//! The receive jitter buffer for the assistant's voice: RTP packets by sequence number,
//! pulled one 20 ms frame at a time by the output on songbird's mixer clock. It reorders,
//! drops late packets, recovers a lost packet from the next one's in-band FEC or conceals it,
//! and tells a pause (contiguous sequence numbers, the server stopped sending) from loss.

use crate::audio::{Frame, FrameProvider, FRAME_SAMPLES};
use bytes::Bytes;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

/// Mono samples per 20 ms at 48 kHz.
const MONO_FRAME: usize = FRAME_SAMPLES / 2;
const TARGET_MIN: usize = 3;
const TARGET_MAX: usize = 8;
/// Frames played without a late packet before the target relaxes by one.
const STABLE_FRAMES_BEFORE_RELAX: u32 = 750;
/// Concealed frames in a row before the buffer skips to the next packet it holds.
const MAX_CONCEALED_RUN: usize = 5;
/// Packets held at most (500 ms); beyond that the oldest is dropped.
const MAX_PACKETS: usize = 25;
/// The longest Opus packet: 120 ms at 48 kHz.
const MAX_PACKET_SAMPLES: usize = 5_760;

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
}

struct Held {
    timestamp: u32,
    payload: Bytes,
}

struct State {
    decoder: Option<opus2::Decoder>,
    packets: BTreeMap<u64, Held>,
    highest: Option<u64>,
    next_seq: Option<u64>,
    playing: bool,
    target: usize,
    stable: u32,
    concealed_run: usize,
    pending: VecDeque<i16>,
    /// Mono samples of the last decoded packet: the size of a concealed frame.
    last_duration: usize,
    /// Timestamp the next packet should carry when the stream is continuous.
    expected_ts: Option<u32>,
    /// `expected_ts` at the moment the buffer ran dry.
    dry_at: Option<u32>,
    scratch: Vec<i16>,
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
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                decoder: opus2::Decoder::new(48_000, opus2::Channels::Mono).ok(),
                packets: BTreeMap::new(),
                highest: None,
                next_seq: None,
                playing: false,
                target: TARGET_MIN,
                stable: 0,
                concealed_run: 0,
                pending: VecDeque::with_capacity(MAX_PACKET_SAMPLES),
                last_duration: MONO_FRAME,
                expected_ts: None,
                dry_at: None,
                scratch: vec![0; MAX_PACKET_SAMPLES],
                counters: JitterCounters::default(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("jitter buffer lock")
    }

    /// Stores one RTP packet. Packets older than the one due next are late and dropped.
    pub fn insert(&self, sequence: u16, timestamp: u32, payload: Bytes) {
        let mut state = self.lock();
        state.counters.packets_received += 1;
        let seq = extend(state.highest, sequence);
        if state.next_seq.is_some_and(|next| seq < next) {
            state.counters.late += 1;
            state.target = (state.target + 1).min(TARGET_MAX);
            state.stable = 0;
            return;
        }
        if state.packets.contains_key(&seq) {
            return;
        }
        state.highest = Some(state.highest.map_or(seq, |high| high.max(seq)));
        state.packets.insert(seq, Held { timestamp, payload });
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
        self.lock().counters
    }

    /// Frames' worth of packets the buffer waits for before it starts playing.
    pub fn target(&self) -> usize {
        self.lock().target
    }
}

impl State {
    fn decode_into_pending(&mut self, payload: &[u8], samples: usize, fec: bool) {
        let samples = samples.clamp(1, MAX_PACKET_SAMPLES);
        let Some(decoder) = self.decoder.as_mut() else {
            self.counters.decode_errors += 1;
            self.pending.extend(std::iter::repeat_n(0, samples));
            return;
        };
        let output = &mut self.scratch[..if payload.is_empty() || fec {
            samples
        } else {
            MAX_PACKET_SAMPLES
        }];
        match decoder.decode(payload, output, fec) {
            Ok(count) => {
                let count = count.min(output.len());
                self.pending.extend(output[..count].iter().copied());
                if !fec && !payload.is_empty() {
                    self.last_duration = count.max(1);
                }
            }
            Err(_) => {
                self.counters.decode_errors += 1;
                self.pending.extend(std::iter::repeat_n(0, samples));
            }
        }
    }

    /// Advances by one packet or one concealed frame. Returns `false` when nothing can be
    /// produced now (not primed, or the buffer ran dry).
    fn step(&mut self) -> bool {
        if !self.playing {
            if self.packets.len() < self.target {
                return false;
            }
            let first = *self.packets.keys().next().expect("packets not empty");
            if let Some(dry_ts) = self.dry_at.take() {
                let continuous = self.next_seq == Some(first)
                    && self.packets.get(&first).map(|held| held.timestamp) == Some(dry_ts);
                if continuous {
                    // The network was late, not the speaker silent: hold more next time.
                    self.counters.playout_underruns += 1;
                    self.target = (self.target + 1).min(TARGET_MAX);
                    self.stable = 0;
                }
            }
            // Late packets are refused on insert, so the first one held is never older than
            // the one due next; start there.
            self.next_seq = Some(first);
            self.playing = true;
        }
        let seq = self.next_seq.expect("next sequence while playing");
        if let Some(held) = self.packets.remove(&seq) {
            let payload = held.payload;
            self.decode_into_pending(&payload, MAX_PACKET_SAMPLES, false);
            self.expected_ts = Some(held.timestamp.wrapping_add(self.last_duration as u32));
            self.next_seq = Some(seq + 1);
            self.concealed_run = 0;
            return true;
        }
        if self.packets.is_empty() {
            // Nothing later is held: a pause or a late packet, not a loss.
            self.playing = false;
            self.dry_at = self.expected_ts;
            return false;
        }
        if self.concealed_run >= MAX_CONCEALED_RUN {
            let first = *self.packets.keys().next().expect("packets not empty");
            self.next_seq = Some(first);
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
                self.decode_into_pending(&next, duration, true);
                self.counters.concealed_fec += 1;
            }
            None => self.decode_into_pending(&[], duration, false),
        }
        self.counters.concealed += 1;
        self.concealed_run += 1;
        self.expected_ts = self.expected_ts.map(|ts| ts.wrapping_add(duration as u32));
        self.next_seq = Some(seq + 1);
        true
    }
}

impl FrameProvider for JitterBuffer {
    fn next_frame(&self, out: &mut Frame) -> bool {
        let mut state = self.lock();
        while state.pending.len() < MONO_FRAME {
            if !state.step() {
                break;
            }
        }
        if state.pending.is_empty() {
            return false;
        }
        let (pairs, _) = out.as_mut_slice().as_chunks_mut::<2>();
        for pair in pairs {
            let value = state.pending.pop_front().unwrap_or(0);
            *pair = [value, value];
        }
        if state.playing {
            state.stable += 1;
            if state.stable >= STABLE_FRAMES_BEFORE_RELAX {
                state.stable = 0;
                if state.target > TARGET_MIN {
                    state.target -= 1;
                }
            }
        }
        true
    }

    fn reset(&self) {
        let mut state = self.lock();
        state.packets.clear();
        state.pending.clear();
        state.next_seq = None;
        state.highest = None;
        state.playing = false;
        state.concealed_run = 0;
        state.expected_ts = None;
        state.dry_at = None;
        state.target = TARGET_MIN;
        state.stable = 0;
        state.decoder = opus2::Decoder::new(48_000, opus2::Channels::Mono).ok();
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

#[cfg(test)]
mod tests {
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

    #[test]
    fn out_of_order_packets_play_in_order() {
        let payloads = packets(14, MONO_FRAME);
        let buffer = JitterBuffer::new();
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
        let buffer = JitterBuffer::new();
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
        let buffer = JitterBuffer::new();
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
        let buffer = JitterBuffer::new();
        for seq in [0, 2, 3, 4] {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        let mut frame = Frame::silence();
        for _ in 0..3 {
            assert!(buffer.next_frame(&mut frame));
        }
        insert(&buffer, &payloads, 1, MONO_FRAME);
        assert_eq!(buffer.counters().late, 1);
        assert_eq!(
            buffer.target(),
            TARGET_MIN + 1,
            "a late packet grows the target"
        );
    }

    #[test]
    fn ten_and_forty_ms_packets_are_sliced_into_twenty_ms_frames() {
        let short = packets(6, MONO_FRAME / 2);
        let buffer = JitterBuffer::new();
        for seq in 0..6u16 {
            insert(&buffer, &short, seq, MONO_FRAME / 2);
        }
        assert_eq!(
            pull_all(&buffer).len(),
            3 * MONO_FRAME,
            "six 10 ms packets, three frames"
        );

        let long = packets(3, MONO_FRAME * 2);
        let buffer = JitterBuffer::new();
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
        let buffer = JitterBuffer::new();
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
        assert_eq!(
            buffer.target(),
            TARGET_MIN + 1,
            "an underrun grows the target"
        );
        // Three packets no longer start playback: the target is now four.
        for seq in 6..9u16 {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        assert!(
            !buffer.next_frame(&mut frame),
            "three of four held: still waiting"
        );
        insert(&buffer, &payloads, 9, MONO_FRAME);
        assert_eq!(pull_all(&buffer).len(), 4 * MONO_FRAME);
        assert_eq!(buffer.counters().playout_underruns, 2);
        assert_eq!(buffer.target(), TARGET_MIN + 2);
        // Then a real pause: the timestamps jump ahead (five packets for the target of five).
        let base = 1_000_000;
        let more = packets(15, MONO_FRAME);
        for seq in 10..15u16 {
            buffer.insert(
                seq,
                base + seq as u32 * MONO_FRAME as u32,
                more[seq as usize].clone(),
            );
        }
        assert_eq!(pull_all(&buffer).len(), 5 * MONO_FRAME);
        assert_eq!(
            buffer.counters().playout_underruns,
            2,
            "a pause is not an underrun"
        );
        assert_eq!(
            buffer.target(),
            TARGET_MIN + 2,
            "a pause does not grow the target"
        );
    }

    #[test]
    fn reset_empties_the_buffer() {
        let payloads = packets(4, MONO_FRAME);
        let buffer = JitterBuffer::new();
        for seq in 0..4u16 {
            insert(&buffer, &payloads, seq, MONO_FRAME);
        }
        buffer.reset();
        let mut frame = Frame::silence();
        assert!(!buffer.next_frame(&mut frame));
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
