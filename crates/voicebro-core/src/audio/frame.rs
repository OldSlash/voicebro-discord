//! The fixed packet everything between Discord and the assistant speaks: 48 kHz, two
//! channels, 20 ms.

use std::time::Duration;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const FRAME_DURATION: Duration = Duration::from_millis(20);
/// Interleaved samples per frame: 960 stereo pairs.
pub const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 50 * CHANNELS;

/// One 20 ms packet of interleaved 16-bit stereo PCM.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    samples: Box<[i16; FRAME_SAMPLES]>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::silence()
    }
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame(peak {})", peak(self.as_slice()))
    }
}

impl Frame {
    pub fn silence() -> Self {
        Self {
            samples: Box::new([0; FRAME_SAMPLES]),
        }
    }

    /// Copies `samples` into a frame; a short slice is padded with silence, a long one is
    /// cut. Callers that care about the length check it first (the receive path does).
    pub fn from_slice(samples: &[i16]) -> Self {
        let mut frame = Self::silence();
        let count = samples.len().min(FRAME_SAMPLES);
        frame.samples[..count].copy_from_slice(&samples[..count]);
        frame
    }

    pub fn as_slice(&self) -> &[i16] {
        &self.samples[..]
    }

    pub fn as_mut_slice(&mut self) -> &mut [i16] {
        &mut self.samples[..]
    }

    pub fn peak(&self) -> i32 {
        peak(self.as_slice())
    }

    pub fn fill_silence(&mut self) {
        self.samples.fill(0);
    }
}

/// Peak absolute value of a PCM block, used for simple voice-activity decisions.
pub fn peak(samples: &[i16]) -> i32 {
    samples
        .iter()
        .map(|sample| (*sample as i32).abs())
        .max()
        .unwrap_or(0)
}

/// Whether a block stays below the given peak.
pub fn is_silent(samples: &[i16], threshold: i32) -> bool {
    peak(samples) < threshold
}

/// Mixes every speaker's block into `out`: the average of the speakers (one speaker passes
/// unchanged), as the first version did. Sums in `i32` and divides before the single clamp,
/// so nothing saturates before averaging.
pub fn mix_into(speakers: &[&[i16]], scratch: &mut Vec<i32>, out: &mut Frame) {
    scratch.clear();
    scratch.resize(FRAME_SAMPLES, 0);
    for block in speakers {
        for (sum, sample) in scratch.iter_mut().zip(block.iter()) {
            *sum += *sample as i32;
        }
    }
    let count = speakers.len().max(1) as i32;
    for (sample, sum) in out.as_mut_slice().iter_mut().zip(scratch.iter()) {
        *sample = (*sum / count).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_20_ms_of_stereo() {
        assert_eq!(FRAME_SAMPLES, 1920);
        assert_eq!(Frame::silence().as_slice().len(), FRAME_SAMPLES);
        assert_eq!(FRAME_DURATION.as_millis(), 20);
    }

    #[test]
    fn overlapping_speakers_are_averaged_and_one_passes_unchanged() {
        let a = vec![30_000i16; FRAME_SAMPLES];
        let b = vec![30_000i16; FRAME_SAMPLES];
        let mut scratch = Vec::new();
        let mut out = Frame::silence();
        mix_into(&[&a, &b], &mut scratch, &mut out);
        assert_eq!(out.as_slice()[0], 30_000, "two loud speakers do not clip");
        let c = vec![-30_000i16; FRAME_SAMPLES];
        mix_into(&[&a, &c], &mut scratch, &mut out);
        assert_eq!(out.as_slice()[0], 0, "opposite signals cancel exactly");
        let single = vec![-1_234i16; FRAME_SAMPLES];
        mix_into(&[&single], &mut scratch, &mut out);
        assert_eq!(out.as_slice()[0], -1_234);
        let three = [
            vec![300i16; FRAME_SAMPLES],
            vec![600; FRAME_SAMPLES],
            vec![900; FRAME_SAMPLES],
        ];
        mix_into(&[&three[0], &three[1], &three[2]], &mut scratch, &mut out);
        assert_eq!(out.as_slice()[0], 600);
        mix_into(&[], &mut scratch, &mut out);
        assert_eq!(out.peak(), 0, "nobody speaking is silence");
    }

    #[test]
    fn two_people_talking_still_cross_the_speech_threshold() {
        // Two voices at a quiet speaking level (peak 4 000), different pitches, one of them
        // starting half a frame later: the average still reads as speech to the gate.
        let voice = |hz: f32, offset: usize| -> Vec<i16> {
            (0..FRAME_SAMPLES / 2)
                .flat_map(|n| {
                    let value = if n < offset {
                        0
                    } else {
                        let t = n as f32 / SAMPLE_RATE as f32;
                        ((std::f32::consts::TAU * hz * t).sin() * 4_000.0) as i16
                    };
                    [value, value]
                })
                .collect()
        };
        let (a, b) = (voice(180.0, 0), voice(240.0, 480));
        let mut scratch = Vec::new();
        let mut out = Frame::silence();
        mix_into(&[&a, &b], &mut scratch, &mut out);
        assert!(out.peak() >= 700, "peak {}", out.peak());
    }

    #[test]
    fn from_slice_pads_and_cuts() {
        let short = Frame::from_slice(&[5, 6]);
        assert_eq!(&short.as_slice()[..3], &[5, 6, 0]);
        let long = Frame::from_slice(&vec![7; FRAME_SAMPLES + 10]);
        assert_eq!(long.as_slice().len(), FRAME_SAMPLES);
        assert_eq!(peak(long.as_slice()), 7);
        assert!(is_silent(short.as_slice(), 10));
    }
}
