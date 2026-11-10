//! What the send thread puts into the call on each 20 ms tick: the oldest waiting frame, or
//! silence. A pure decision over the queue depth, so the timing rules are tested without a
//! clock or a call.

/// Frames waiting before real audio is sent, at the start and after the queue ran dry, so a
/// late gate tick does not leave the next send tick empty (40 ms).
pub const PREFILL_FRAMES: usize = 2;

/// What one tick sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Pop the oldest waiting frame and send it.
    Frame,
    /// Send silence. `underrun` is true when the stream had started and nothing was waiting.
    Silence { underrun: bool },
}

#[derive(Debug, Default)]
pub struct SendPacer {
    primed: bool,
    started: bool,
}

impl SendPacer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decides one tick from the number of frames waiting.
    pub fn pick(&mut self, queued: usize) -> Pick {
        if queued == 0 {
            self.primed = false;
            return Pick::Silence {
                underrun: self.started,
            };
        }
        if !self.primed {
            if queued < PREFILL_FRAMES {
                return Pick::Silence { underrun: false };
            }
            self.primed = true;
        }
        self.started = true;
        Pick::Frame
    }

    /// Back to the initial state, for a new connection.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{frame_channel, Frame, INPUT_CAPACITY};

    /// A deterministic jitter in -7..=7 ms (intervals between 6 and 34 ms around 20 ms).
    fn jitter(seed: &mut u32) -> i64 {
        *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((*seed >> 16) % 15) as i64 - 7
    }

    #[test]
    fn a_jittery_producer_loses_no_frame_and_keeps_order() {
        let (sender, receiver) = frame_channel(INPUT_CAPACITY);
        let mut pacer = SendPacer::new();
        let total = 500usize;
        let mut seed = 7;
        // Producer: frame k at 20 k + jitter ms; consumer ticks at 20 m + 10 ms.
        let arrivals: Vec<i64> = (0..total as i64)
            .map(|k| 20 * k + jitter(&mut seed))
            .collect();
        let mut sent = Vec::new();
        let mut underruns = 0;
        let mut next_arrival = 0;
        let mut previous = i64::MIN;
        for tick in 0..(total as i64 + 10) {
            let now = 20 * tick + 10;
            while next_arrival < total && arrivals[next_arrival] <= now {
                assert!(
                    arrivals[next_arrival] >= previous,
                    "the producer never goes back"
                );
                previous = arrivals[next_arrival];
                sender.send(Frame::from_slice(&[next_arrival as i16]));
                next_arrival += 1;
            }
            match pacer.pick(receiver.len()) {
                Pick::Frame => sent.push(receiver.try_recv().unwrap().as_slice()[0] as usize),
                Pick::Silence { underrun } => {
                    if underrun && next_arrival < total {
                        underruns += 1;
                    }
                }
            }
        }
        assert_eq!(sender.dropped(), 0, "no frame dropped");
        assert_eq!(
            sent,
            (0..total).collect::<Vec<_>>(),
            "every frame, in order"
        );
        assert_eq!(underruns, 0, "no underrun while the producer runs");
    }

    #[test]
    fn an_empty_queue_sends_silence_and_counts_underruns_once_started() {
        let mut pacer = SendPacer::new();
        assert_eq!(
            pacer.pick(0),
            Pick::Silence { underrun: false },
            "nothing sent yet"
        );
        assert_eq!(pacer.pick(1), Pick::Silence { underrun: false }, "prefill");
        assert_eq!(pacer.pick(2), Pick::Frame);
        assert_eq!(pacer.pick(1), Pick::Frame);
        for _ in 0..5 {
            assert_eq!(pacer.pick(0), Pick::Silence { underrun: true });
        }
        assert_eq!(
            pacer.pick(1),
            Pick::Silence { underrun: false },
            "prefill again"
        );
        assert_eq!(pacer.pick(2), Pick::Frame);
        pacer.reset();
        assert_eq!(pacer.pick(0), Pick::Silence { underrun: false });
    }

    #[test]
    fn a_backlog_over_five_frames_drops_the_oldest() {
        let (sender, receiver) = frame_channel(INPUT_CAPACITY);
        for k in 0..8 {
            sender.send(Frame::from_slice(&[k]));
        }
        assert_eq!(INPUT_CAPACITY, 5);
        assert_eq!(receiver.len(), 5);
        assert_eq!(sender.dropped(), 3);
        assert_eq!(receiver.dropped(), 3);
        assert_eq!(
            receiver.try_recv().unwrap().as_slice()[0],
            3,
            "oldest kept is frame 3"
        );
    }
}
