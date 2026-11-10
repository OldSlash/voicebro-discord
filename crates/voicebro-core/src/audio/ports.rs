//! The audio connection between the gate and an assistant: frames in through a tiny
//! bounded queue, frames out through the shared output queue.

use super::frame::Frame;
use super::output::OutputHandle;
use super::queue::FrameQueue;
use std::sync::Arc;

/// Frames kept toward the assistant: 100 ms. The call's send thread takes them in order on
/// its own 20 ms clock; more than this waiting means the consumer fell behind, and the
/// oldest is dropped.
pub const INPUT_CAPACITY: usize = 5;

#[derive(Clone)]
pub struct FrameSender {
    queue: Arc<FrameQueue>,
}

impl FrameSender {
    /// Returns `true` when an older frame was dropped to make room.
    pub fn send(&self, frame: Frame) -> bool {
        self.queue.push(frame)
    }

    pub fn dropped(&self) -> u64 {
        self.queue.dropped()
    }

    pub fn clear(&self) {
        self.queue.clear();
    }
}

#[derive(Clone)]
pub struct FrameReceiver {
    queue: Arc<FrameQueue>,
}

impl FrameReceiver {
    pub fn try_recv(&self) -> Option<Frame> {
        self.queue.pop()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Frames dropped at the sending side because this receiver fell behind.
    pub fn dropped(&self) -> u64 {
        self.queue.dropped()
    }
}

pub fn frame_channel(capacity: usize) -> (FrameSender, FrameReceiver) {
    let queue = Arc::new(FrameQueue::new(capacity));
    (
        FrameSender {
            queue: Arc::clone(&queue),
        },
        FrameReceiver { queue },
    )
}

/// What an assistant session gets: where to read the channel and where to speak.
#[derive(Clone)]
pub struct AudioPorts {
    pub input: FrameReceiver,
    pub output: OutputHandle,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::output::output_pair;

    #[test]
    fn frames_flow_from_sender_to_receiver_and_out_to_the_source() {
        let (sender, receiver) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        let ports = AudioPorts {
            input: receiver,
            output: output.clone(),
        };
        assert!(!sender.send(Frame::from_slice(&[1])));
        let frame = ports.input.try_recv().expect("frame arrives");
        assert_eq!(frame.as_slice()[0], 1);
        ports.output.push(frame);
        assert_eq!(output.queued(), 1);
        for _ in 0..INPUT_CAPACITY + 1 {
            sender.send(Frame::silence());
        }
        assert_eq!(sender.dropped(), 1);
    }
}
