//! The audio core: the formats, queues and conversions shared by Discord, the gate, the
//! wake engine and the assistant. See `docs/ARCHITECTURE.md` 4.1.

pub mod dsp;
pub mod earcon;
pub mod frame;
pub mod output;
pub mod ports;
pub mod queue;

pub use earcon::Earcon;
pub use frame::{peak, Frame, CHANNELS, FRAME_DURATION, FRAME_SAMPLES, SAMPLE_RATE};
pub use output::{output_pair, FrameProvider, OutputCounters, OutputHandle, PcmSource};
pub use ports::{frame_channel, AudioPorts, FrameReceiver, FrameSender, INPUT_CAPACITY};
pub use queue::FrameQueue;
