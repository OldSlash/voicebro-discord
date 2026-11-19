//! One speaker at a time toward the call (change `speaker-turns`, design D1 to D6): the
//! floor, the waiting people's buffers and the per-speaker pre-roll. Pure logic on the
//! receive ticks; the gate owns one `Floor` per session that started with
//! `assistant.one_speaker_at_a_time` on.
//!
//! - The first voiced speaker takes the floor; their frames go to the call as they come.
//! - Whoever speaks meanwhile waits: their frames go to a buffer of their own (at most
//!   [`MAX_WAIT_FRAMES`]; the oldest are dropped and counted).
//! - The holder releases the floor after [`FLOOR_PAUSE_TICKS`] ticks without a voiced frame
//!   once their own backlog is played. The floor then passes to the person who started
//!   waiting first, unless the assistant's voice plays (D4); their buffer drains one frame
//!   per tick, silent frames at its front skipped while more wait, followed by their live
//!   frames.
//! - Silence is counted in receive ticks, never on a clock.

use crate::audio::{peak, Frame};
use std::collections::{HashMap, VecDeque};

/// A frame is voiced from this peak (the gate's own threshold).
pub use super::VOICE_PEAK_THRESHOLD;

/// Ticks without a voiced frame after which the holder releases the floor (300 ms). Set
/// from probe 2.1 of the change; it must stay below the pause after which the realtime model
/// ends a turn on its own.
pub const FLOOR_PAUSE_TICKS: u32 = 15;
/// At most 15 s of a waiting person's speech is kept.
pub const MAX_WAIT_FRAMES: usize = 750;
/// A waiting person's silent frames stop being kept after this many in a row (they have
/// finished talking); a voiced frame resumes it.
const WAIT_TRAILING_SILENCE: u32 = 15;
/// The ticks a passed buffer is held for its note to reach Codex first (D5).
pub const NOTE_HOLD_TICKS: u32 = 12;
/// A speaker's pre-roll ring is given back after this many ticks without a frame (5 s).
const RING_IDLE_TICKS: u64 = 250;

struct Waiter {
    ssrc: u32,
    buffer: VecDeque<Frame>,
    /// Silent ticks in a row at the end of the buffer.
    trailing_silence: u32,
}

impl Waiter {
    fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            buffer: VecDeque::with_capacity(MAX_WAIT_FRAMES),
            trailing_silence: 0,
        }
    }

    /// Keeps a frame: voiced always, silent only within a pause. Returns the frames dropped
    /// from the full buffer.
    fn keep(&mut self, frame: Frame, voiced: bool) -> u64 {
        if voiced {
            self.trailing_silence = 0;
        } else {
            self.trailing_silence += 1;
            if self.trailing_silence > WAIT_TRAILING_SILENCE {
                return 0;
            }
        }
        let mut dropped = 0;
        if self.buffer.len() >= MAX_WAIT_FRAMES {
            self.buffer.pop_front();
            dropped = 1;
        }
        self.buffer.push_back(frame);
        dropped
    }
}

struct Ring {
    frames: VecDeque<Frame>,
    last_tick: u64,
}

/// Who the floor went to in a tick, for the note (D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloorChange {
    pub ssrc: u32,
    /// The person had waited (their buffer follows).
    pub waited: bool,
}

/// What one tick gives the call.
#[derive(Debug, Default)]
pub struct FloorTick {
    /// The holder's frame (or the next of their backlog); silence when nobody holds it.
    pub frame: Option<Frame>,
    pub change: Option<FloorChange>,
}

#[derive(Default)]
pub struct Floor {
    holder: Option<u32>,
    holder_silent: u32,
    /// The passed holder's buffer, played before their live frames.
    backlog: VecDeque<Frame>,
    /// Ticks the backlog still waits for its note.
    hold: u32,
    waiting: VecDeque<Waiter>,
    rings: HashMap<u32, Ring>,
    pub passes: u64,
    pub dropped: u64,
}

fn is_voiced(samples: &[i16]) -> bool {
    peak(samples) >= VOICE_PEAK_THRESHOLD
}

impl Floor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    #[cfg(test)]
    pub fn holder(&self) -> Option<u32> {
        self.holder
    }

    /// Keeps each speaker's last [`super::PREROLL_FRAMES`] (D6), every tick, whoever holds
    /// the floor; gives back the rings of speakers absent for 5 s.
    pub fn remember(&mut self, tick: u64, speakers: &[(u32, &[i16])]) {
        for (ssrc, samples) in speakers {
            let ring = self.rings.entry(*ssrc).or_insert_with(|| Ring {
                frames: VecDeque::with_capacity(super::PREROLL_FRAMES),
                last_tick: tick,
            });
            if ring.frames.len() >= super::PREROLL_FRAMES {
                ring.frames.pop_front();
            }
            ring.frames.push_back(Frame::from_slice(samples));
            ring.last_tick = tick;
        }
        self.rings
            .retain(|_, ring| tick.saturating_sub(ring.last_tick) <= RING_IDLE_TICKS);
    }

    /// An activation by `caller` (D6): the caller holds the floor, and the replay is their
    /// ring's last `frames`; every other ring's last `frames` with speech in them go to
    /// that person's buffer, in the order they started talking.
    pub fn activate(&mut self, caller: u32, frames: usize) -> VecDeque<Frame> {
        self.holder = Some(caller);
        self.holder_silent = 0;
        self.backlog.clear();
        self.hold = 0;
        let tail = |ring: &Ring| -> Vec<Frame> {
            let skip = ring.frames.len().saturating_sub(frames);
            ring.frames.iter().skip(skip).cloned().collect()
        };
        let replay = self
            .rings
            .get(&caller)
            .map(|ring| tail(ring).into_iter().collect())
            .unwrap_or_default();
        let mut others: Vec<(usize, u32, Vec<Frame>)> = self
            .rings
            .iter()
            .filter(|(ssrc, _)| **ssrc != caller)
            .filter_map(|(ssrc, ring)| {
                let frames = tail(ring);
                let first = frames
                    .iter()
                    .position(|frame| frame.peak() >= VOICE_PEAK_THRESHOLD)?;
                Some((first, *ssrc, frames))
            })
            .collect();
        others.sort_by_key(|(first, ssrc, _)| (*first, *ssrc));
        self.waiting.clear();
        for (first, ssrc, frames) in others {
            let mut waiter = Waiter::new(ssrc);
            for frame in frames.into_iter().skip(first) {
                let voiced = frame.peak() >= VOICE_PEAK_THRESHOLD;
                self.dropped += waiter.keep(frame, voiced);
            }
            self.waiting.push_back(waiter);
        }
        replay
    }

    /// One receive tick. `answer_playing`: the assistant's voice played within the last
    /// 200 ms (D4). `note_hold`: a passed buffer waits this many ticks for its note.
    pub fn tick(
        &mut self,
        speakers: &[(u32, &[i16])],
        answer_playing: bool,
        note_hold: u32,
    ) -> FloorTick {
        let mut out = FloorTick::default();
        // Free floor: the loudest voiced speaker of the tick takes it, unless people wait
        // (then a new speaker waits behind them).
        if self.holder.is_none() && self.waiting.is_empty() {
            if let Some((ssrc, _)) = speakers
                .iter()
                .filter(|(_, samples)| is_voiced(samples))
                .max_by_key(|(ssrc, samples)| (peak(samples), std::cmp::Reverse(*ssrc)))
            {
                self.holder = Some(*ssrc);
                self.holder_silent = 0;
                out.change = Some(FloorChange {
                    ssrc: *ssrc,
                    waited: false,
                });
            }
        }
        // Everyone else: waiting speech is kept in order.
        let mut holder_frame = None;
        for (ssrc, samples) in speakers {
            if Some(*ssrc) == self.holder {
                holder_frame = Some(Frame::from_slice(samples));
                continue;
            }
            let voiced = is_voiced(samples);
            let index = self.waiting.iter().position(|waiter| waiter.ssrc == *ssrc);
            match index {
                Some(index) => {
                    self.dropped += self.waiting[index].keep(Frame::from_slice(samples), voiced);
                }
                None if voiced => {
                    let mut waiter = Waiter::new(*ssrc);
                    self.dropped += waiter.keep(Frame::from_slice(samples), true);
                    self.waiting.push_back(waiter);
                }
                None => {}
            }
        }
        // The holder: their backlog first, then their live frames.
        if self.holder.is_some() {
            let voiced = holder_frame
                .as_ref()
                .is_some_and(|frame| frame.peak() >= VOICE_PEAK_THRESHOLD);
            if voiced {
                self.holder_silent = 0;
            } else {
                self.holder_silent += 1;
            }
            if self.hold > 0 {
                self.hold -= 1;
                self.follow_backlog(holder_frame, voiced);
            } else if !self.backlog.is_empty() {
                self.follow_backlog(holder_frame, voiced);
                out.frame = self.backlog.pop_front();
                while self.backlog.len() > 1
                    && self
                        .backlog
                        .front()
                        .is_some_and(|frame| frame.peak() < VOICE_PEAK_THRESHOLD)
                {
                    self.backlog.pop_front();
                }
            } else {
                out.frame = holder_frame;
            }
            if self.holder_silent >= FLOOR_PAUSE_TICKS && self.backlog.is_empty() && self.hold == 0
            {
                self.holder = None;
            }
        }
        // A free floor passes to the first who waited, unless the answer plays.
        if self.holder.is_none() && !answer_playing {
            if let Some(waiter) = self.waiting.pop_front() {
                self.holder = Some(waiter.ssrc);
                self.holder_silent = 0;
                self.backlog = waiter.buffer;
                self.hold = note_hold;
                self.passes += 1;
                out.change = Some(FloorChange {
                    ssrc: waiter.ssrc,
                    waited: true,
                });
            }
        }
        out
    }

    /// The holder's live frame while their buffer still plays: it follows the buffer, except
    /// silence after silence, so a finished speaker's buffer empties and they are live again.
    fn follow_backlog(&mut self, frame: Option<Frame>, voiced: bool) {
        let Some(frame) = frame else {
            return;
        };
        let after_speech = self
            .backlog
            .back()
            .is_some_and(|last| last.peak() >= VOICE_PEAK_THRESHOLD);
        if voiced || after_speech {
            self.backlog.push_back(frame);
        }
    }

    /// The end of a session, mute or a deactivation: nobody holds or waits.
    pub fn clear(&mut self) {
        self.holder = None;
        self.holder_silent = 0;
        self.backlog.clear();
        self.hold = 0;
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::FRAME_SAMPLES;

    fn voice(level: i16) -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|i| if i % 2 == 0 { level } else { -level })
            .collect()
    }
    fn silence() -> Vec<i16> {
        vec![0; FRAME_SAMPLES]
    }

    /// A frame tagged by its level, so the passed frames say whose they are.
    fn level(frame: &Option<Frame>) -> i32 {
        frame.as_ref().map_or(0, Frame::peak)
    }

    const A: u32 = 1;
    const B: u32 = 2;
    const C: u32 = 3;

    #[test]
    fn one_speaker_passes_unchanged_with_no_delay() {
        let mut floor = Floor::new();
        let a = voice(1000);
        for _ in 0..10 {
            let out = floor.tick(&[(A, &a)], false, 0);
            assert_eq!(level(&out.frame), 1000);
        }
        assert_eq!(floor.waiting(), 0);
        assert_eq!(floor.passes, 0);
    }

    #[test]
    fn a_second_speaker_waits_and_follows_after_the_pause() {
        let mut floor = Floor::new();
        let (a, b, quiet) = (voice(1000), voice(2000), silence());
        // A talks 10 ticks, B starts on tick 3 and talks 10 ticks.
        let mut passed = Vec::new();
        for tick in 0..60 {
            let mut speakers: Vec<(u32, &[i16])> = Vec::new();
            speakers.push((A, if tick < 10 { &a } else { &quiet }));
            speakers.push((B, if (3..13).contains(&tick) { &b } else { &quiet }));
            let out = floor.tick(&speakers, false, 0);
            passed.push(level(&out.frame));
        }
        // A's ten frames first, never mixed with B's.
        assert!(
            passed[..10].iter().all(|level| *level == 1000),
            "{passed:?}"
        );
        // The floor passes after A's 15 silent ticks, then B's ten voiced frames follow.
        let b_frames = passed.iter().filter(|level| **level == 2000).count();
        assert_eq!(b_frames, 10, "{passed:?}");
        let first_b = passed.iter().position(|level| *level == 2000).unwrap();
        assert!(first_b >= 10 + FLOOR_PAUSE_TICKS as usize - 1, "{first_b}");
        assert_eq!(floor.passes, 1);
    }

    #[test]
    fn a_short_pause_keeps_the_floor() {
        let mut floor = Floor::new();
        let (a, b, quiet) = (voice(1000), voice(2000), silence());
        for tick in 0..30 {
            // A pauses 10 ticks (200 ms) in the middle; B talks from the second tick on.
            let a_frame: &[i16] = if (5..15).contains(&tick) { &quiet } else { &a };
            let b_frame: &[i16] = if tick > 0 { &b } else { &quiet };
            floor.tick(&[(A, a_frame), (B, b_frame)], false, 0);
        }
        assert_eq!(floor.holder(), Some(A));
        assert_eq!(floor.waiting(), 1);
    }

    #[test]
    fn the_earlier_waiter_goes_first() {
        let mut floor = Floor::new();
        let (a, b, c, quiet) = (voice(1000), voice(2000), voice(3000), silence());
        let mut order = Vec::new();
        for tick in 0..120 {
            let a_frame: &[i16] = if tick < 10 { &a } else { &quiet };
            let b_frame: &[i16] = if (2..6).contains(&tick) { &b } else { &quiet };
            let c_frame: &[i16] = if (4..8).contains(&tick) { &c } else { &quiet };
            let out = floor.tick(&[(A, a_frame), (B, b_frame), (C, c_frame)], false, 0);
            if let Some(change) = out.change {
                order.push(change);
            }
        }
        assert_eq!(
            order,
            [
                FloorChange {
                    ssrc: A,
                    waited: false
                },
                FloorChange {
                    ssrc: B,
                    waited: true
                },
                FloorChange {
                    ssrc: C,
                    waited: true
                }
            ]
        );
    }

    #[test]
    fn twenty_seconds_of_waiting_keep_the_last_fifteen() {
        let mut floor = Floor::new();
        let (a, b) = (voice(1000), voice(2000));
        for _ in 0..1000 {
            floor.tick(&[(A, &a), (B, &b)], false, 0);
        }
        assert_eq!(floor.waiting(), 1);
        assert_eq!(floor.dropped, 1000 - MAX_WAIT_FRAMES as u64);
    }

    #[test]
    fn pauses_in_a_buffer_are_skipped_and_live_frames_follow() {
        let mut floor = Floor::new();
        let (a, b, quiet) = (voice(1000), voice(2000), silence());
        let b_late = voice(2500);
        let mut passed = Vec::new();
        for tick in 0..150 {
            let a_frame: &[i16] = if tick < 70 { &a } else { &quiet };
            // B: 10 voiced, 10 silent, 10 voiced while A talks, then live speech later.
            let b_frame: &[i16] = match tick {
                2..12 => &b,
                12..22 => &quiet,
                22..32 => &b,
                85..95 => &b_late,
                _ => &quiet,
            };
            let out = floor.tick(&[(A, a_frame), (B, b_frame)], false, 0);
            passed.push(level(&out.frame));
        }
        assert_eq!(
            passed.iter().filter(|level| **level == 2000).count(),
            20,
            "both speech parts"
        );
        assert_eq!(
            passed.iter().filter(|level| **level == 2500).count(),
            10,
            "the live frames, none lost or repeated"
        );
        let first = passed.iter().position(|level| *level == 2000).unwrap();
        let last = passed.iter().rposition(|level| *level == 2000).unwrap();
        assert!(
            last - first < 20 + 10,
            "less than the 10 silent ticks between: {first}..{last}"
        );
    }

    #[test]
    fn a_waiting_buffer_does_not_cut_the_answer() {
        let mut floor = Floor::new();
        let (a, b, quiet) = (voice(1000), voice(2000), silence());
        for tick in 0..10 {
            floor.tick(
                &[(A, &a), (B, if tick > 2 { &b } else { &quiet })],
                false,
                0,
            );
        }
        // The answer plays: A's pause passes nothing.
        for _ in 0..40 {
            let out = floor.tick(&[(A, &quiet), (B, &quiet)], true, 0);
            assert_ne!(level(&out.frame), 2000);
        }
        assert_eq!(floor.waiting(), 1);
        // The answer stops: B follows.
        let out = floor.tick(&[(A, &quiet), (B, &quiet)], false, 0);
        assert_eq!(
            out.change,
            Some(FloorChange {
                ssrc: B,
                waited: true
            })
        );
    }

    #[test]
    fn a_new_speaker_during_the_answer_goes_live_when_nobody_waits() {
        let mut floor = Floor::new();
        let c = voice(3000);
        let out = floor.tick(&[(C, &c)], true, 0);
        assert_eq!(level(&out.frame), 3000);
    }

    #[test]
    fn the_note_hold_delays_the_buffer() {
        let mut floor = Floor::new();
        let (a, b, quiet) = (voice(1000), voice(2000), silence());
        for tick in 0..10 {
            floor.tick(
                &[(A, &a), (B, if tick > 2 { &b } else { &quiet })],
                false,
                5,
            );
        }
        let mut ticks = 0;
        loop {
            ticks += 1;
            let out = floor.tick(&[(A, &quiet), (B, &quiet)], false, 5);
            if level(&out.frame) == 2000 {
                break;
            }
            assert!(ticks < 40, "the buffer never came");
        }
        assert!(ticks >= FLOOR_PAUSE_TICKS + 5, "{ticks}");
    }

    #[test]
    fn an_activation_replays_the_caller_and_queues_the_others() {
        let mut floor = Floor::new();
        let (a, b, quiet) = (voice(1000), voice(2000), silence());
        for tick in 0..50u64 {
            let b_frame: &[i16] = if tick >= 30 { &b } else { &quiet };
            floor.remember(tick, &[(A, &a), (B, b_frame)]);
        }
        let replay = floor.activate(A, 25);
        assert_eq!(replay.len(), 25);
        assert!(
            replay.iter().all(|frame| frame.peak() == 1000),
            "only the caller's speech"
        );
        assert_eq!(floor.holder(), Some(A));
        assert_eq!(floor.waiting(), 1, "B's speech after the phrase waits");
    }

    #[test]
    fn a_ring_is_given_back_after_five_seconds_of_absence() {
        let mut floor = Floor::new();
        let a = voice(1000);
        floor.remember(0, &[(A, &a), (B, &a)]);
        for tick in 1..=RING_IDLE_TICKS + 1 {
            floor.remember(tick, &[(A, &a)]);
        }
        assert!(floor.rings.contains_key(&A));
        assert!(!floor.rings.contains_key(&B));
    }

    #[test]
    fn clear_empties_everything() {
        let mut floor = Floor::new();
        let (a, b) = (voice(1000), voice(2000));
        floor.tick(&[(A, &a), (B, &b)], false, 0);
        floor.tick(&[(A, &a), (B, &b)], false, 0);
        floor.clear();
        assert_eq!((floor.holder(), floor.waiting()), (None, 0));
    }
}
