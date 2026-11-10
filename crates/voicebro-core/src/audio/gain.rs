//! The two user gains: how loud the assistant hears the channel (input, applied by the gate
//! right before a frame is sent to the assistant) and how loud it speaks (output, applied by
//! `PcmSource` before earcons are mixed in). Each direction keeps its gain in an atomic, so a
//! saved change reaches the next frame without rebuilding anything, and counts the samples
//! it had to clamp. See `docs/ARCHITECTURE.md` 4.1.

use super::frame::Frame;
use crate::settings::AudioSettings;
use serde::Serialize;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Length of one peak window.
const WINDOW_MS: u64 = 1_000;
/// Full scale of a 16-bit sample, the unit of the stored peaks.
const FULL_SCALE: f32 = 32_768.0;

/// One direction: its gain, its clip counter and the peak of the last second.
pub struct GainStage {
    gain_bits: AtomicU32,
    clips: AtomicU64,
    started: Instant,
    /// Milliseconds since `started` at which the current window began.
    window_start: AtomicU64,
    /// Peak of the current window and of the last completed one, in 16-bit units.
    current_peak: AtomicU32,
    last_peak: AtomicU32,
}

impl Default for GainStage {
    fn default() -> Self {
        Self::new()
    }
}

/// One direction as the status shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GainReading {
    pub gain: f32,
    /// Peak of the last second as a share of full scale, 0.0 to 1.0, after the gain.
    pub peak: f32,
    /// Samples clamped to the 16-bit range since start.
    pub clips: u64,
}

impl GainStage {
    pub fn new() -> Self {
        Self {
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            clips: AtomicU64::new(0),
            started: Instant::now(),
            window_start: AtomicU64::new(0),
            current_peak: AtomicU32::new(0),
            last_peak: AtomicU32::new(0),
        }
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain_bits.store(gain.to_bits(), Ordering::Release);
    }

    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Acquire))
    }

    pub fn clips(&self) -> u64 {
        self.clips.load(Ordering::Acquire)
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Scales a frame in place (the input direction), clamping to the 16-bit range.
    pub fn scale_frame(&self, frame: &mut Frame) {
        let gain = self.gain();
        let mut clipped = 0u64;
        let mut peak = 0u32;
        for sample in frame.as_mut_slice() {
            if gain != 1.0 {
                let scaled = (*sample as f32 * gain).round();
                *sample = if scaled > i16::MAX as f32 {
                    clipped += 1;
                    i16::MAX
                } else if scaled < i16::MIN as f32 {
                    clipped += 1;
                    i16::MIN
                } else {
                    scaled as i16
                };
            }
            peak = peak.max(sample.unsigned_abs() as u32);
        }
        self.finish(clipped, peak, self.now_ms());
    }

    /// Writes `samples` times the gain into `out` as floats in -1.0..=1.0 (the output
    /// direction), clamping at full scale.
    pub fn scale_to_f32(&self, samples: &[i16], out: &mut [f32]) {
        let gain = self.gain() / FULL_SCALE;
        let mut clipped = 0u64;
        let mut peak = 0f32;
        for (out, sample) in out.iter_mut().zip(samples) {
            let scaled = *sample as f32 * gain;
            *out = if scaled > 1.0 {
                clipped += 1;
                1.0
            } else if scaled < -1.0 {
                clipped += 1;
                -1.0
            } else {
                scaled
            };
            peak = peak.max(out.abs());
        }
        self.finish(clipped, (peak * FULL_SCALE) as u32, self.now_ms());
    }

    fn finish(&self, clipped: u64, peak: u32, now_ms: u64) {
        if clipped > 0 {
            self.clips.fetch_add(clipped, Ordering::AcqRel);
        }
        self.record_peak(peak, now_ms);
    }

    fn record_peak(&self, peak: u32, now_ms: u64) {
        let start = self.window_start.load(Ordering::Acquire);
        let age = now_ms.saturating_sub(start);
        if age >= WINDOW_MS
            && self
                .window_start
                .compare_exchange(start, now_ms, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let finished = self.current_peak.swap(0, Ordering::AcqRel);
            // A window older than one more second means nothing played in the last second.
            let last = if age < 2 * WINDOW_MS { finished } else { 0 };
            self.last_peak.store(last, Ordering::Release);
        }
        self.current_peak.fetch_max(peak, Ordering::AcqRel);
    }

    /// The peak of the last completed second, 0.0 when nothing passed in that second.
    fn peak_at(&self, now_ms: u64) -> f32 {
        let age = now_ms.saturating_sub(self.window_start.load(Ordering::Acquire));
        let raw = if age < WINDOW_MS {
            self.last_peak.load(Ordering::Acquire)
        } else if age < 2 * WINDOW_MS {
            // The current window is complete but nothing has rolled it over yet.
            self.current_peak.load(Ordering::Acquire)
        } else {
            0
        };
        (raw as f32 / FULL_SCALE).min(1.0)
    }

    pub fn reading(&self) -> GainReading {
        GainReading {
            gain: self.gain(),
            peak: self.peak_at(self.now_ms()),
            clips: self.clips(),
        }
    }
}

/// Both directions, shared by every gate (input) and every output (output).
#[derive(Clone, Default)]
pub struct AudioGains {
    pub input: Arc<GainStage>,
    pub output: Arc<GainStage>,
}

/// The levels part of the status.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AudioLevels {
    /// Toward the assistant.
    pub input: GainReading,
    /// Toward Discord.
    pub output: GainReading,
}

impl AudioGains {
    pub fn apply(&self, settings: &AudioSettings) {
        self.input.set_gain(settings.input_gain);
        self.output.set_gain(settings.output_gain);
    }

    pub fn levels(&self) -> AudioLevels {
        AudioLevels {
            input: self.input.reading(),
            output: self.output.reading(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::FRAME_SAMPLES;

    #[test]
    fn input_gain_scales_clamps_and_counts_clips() {
        let stage = GainStage::new();
        let mut frame = Frame::from_slice(&[1_000, -1_000, 0]);
        let original = frame.clone();
        stage.scale_frame(&mut frame);
        assert_eq!(frame, original, "1.0 leaves the frame unchanged");

        stage.set_gain(2.0);
        stage.scale_frame(&mut frame);
        assert_eq!(&frame.as_slice()[..3], &[2_000, -2_000, 0]);

        stage.set_gain(4.0);
        let mut loud = Frame::from_slice(&[10_000]);
        stage.scale_frame(&mut loud);
        assert_eq!(loud.as_slice()[0], 32_767);
        assert_eq!(stage.clips(), 1, "one clamped sample");

        stage.set_gain(0.0);
        let mut any = Frame::from_slice(&vec![12_345; FRAME_SAMPLES]);
        stage.scale_frame(&mut any);
        assert_eq!(any.peak(), 0, "0.0 is silence");
    }

    #[test]
    fn output_gain_scales_into_floats_and_counts_clips() {
        let stage = GainStage::new();
        stage.set_gain(2.5);
        let mut out = [0f32; 3];
        stage.scale_to_f32(&[1_000, -1_000, 20_000], &mut out);
        assert!((out[0] - 2_500.0 / FULL_SCALE).abs() < 1e-6);
        assert!((out[1] + 2_500.0 / FULL_SCALE).abs() < 1e-6);
        assert_eq!(out[2], 1.0, "20 000 times 2.5 is past full scale");
        assert_eq!(stage.clips(), 1);
    }

    #[test]
    fn the_peak_is_the_last_completed_second_and_goes_to_zero_without_audio() {
        let stage = GainStage::new();
        stage.record_peak(8_192, 100);
        stage.record_peak(4_096, 900);
        assert_eq!(
            stage.peak_at(950),
            0.0,
            "the first second is not complete yet"
        );
        assert_eq!(stage.peak_at(1_200), 0.25, "complete, not rolled over");
        stage.record_peak(1_000, 1_300);
        assert_eq!(stage.peak_at(1_400), 0.25, "rolled over: the last second");
        assert_eq!(stage.peak_at(3_400), 0.0, "nothing played for two seconds");
        stage.record_peak(2_048, 5_000);
        assert_eq!(stage.peak_at(5_100), 0.0, "the stale window does not count");
    }

    #[test]
    fn gains_apply_from_settings_to_both_directions() {
        let gains = AudioGains::default();
        gains.apply(&AudioSettings {
            input_gain: 3.39,
            output_gain: 2.52,
        });
        let levels = gains.levels();
        assert_eq!(levels.input.gain, 3.39);
        assert_eq!(levels.output.gain, 2.52);
        assert_eq!(levels.input.clips + levels.output.clips, 0);
    }
}
