//! A bounded frame queue that drops the oldest frame when full and counts what it dropped.
//! Deeper buffering would be voice latency, so the capacities stay tiny.

use super::frame::Frame;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub struct FrameQueue {
    capacity: usize,
    frames: Mutex<VecDeque<Frame>>,
    pushed: AtomicU64,
    dropped: AtomicU64,
}

impl FrameQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            frames: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
            pushed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Returns `true` when an older frame had to be dropped to make room.
    pub fn push(&self, frame: Frame) -> bool {
        let mut frames = self.frames.lock().expect("frame queue lock");
        self.pushed.fetch_add(1, Ordering::Relaxed);
        let dropped = if frames.len() >= self.capacity {
            frames.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        };
        frames.push_back(frame);
        dropped
    }

    pub fn pop(&self) -> Option<Frame> {
        self.frames.lock().expect("frame queue lock").pop_front()
    }

    pub fn len(&self) -> usize {
        self.frames.lock().expect("frame queue lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.frames.lock().expect("frame queue lock").clear();
    }

    pub fn pushed(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: i16) -> Frame {
        Frame::from_slice(&[value])
    }

    #[test]
    fn overflow_drops_the_oldest_and_counts() {
        let queue = FrameQueue::new(2);
        assert!(!queue.push(frame(1)));
        assert!(!queue.push(frame(2)));
        assert!(queue.push(frame(3)));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.dropped(), 1);
        assert_eq!(queue.pushed(), 3);
        assert_eq!(queue.pop().unwrap().as_slice()[0], 2);
        assert_eq!(queue.pop().unwrap().as_slice()[0], 3);
        assert!(queue.pop().is_none());
    }
}
