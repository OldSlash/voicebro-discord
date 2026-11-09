//! Small fixed-ratio resamplers for the 48 kHz stereo PCM.
//!
//! The wake-word recognizer wants 16 kHz mono, and a 24 kHz mono path is kept for a later
//! assistant transport. Both are integer ratios of 48 kHz, so a windowed-sinc FIR low-pass
//! followed by decimation (or zero-stuffing for upsampling) is enough and keeps the
//! filters deterministic and testable.

use std::collections::VecDeque;

fn lowpass_taps(cutoff: f32, count: usize) -> Vec<f32> {
    // `cutoff` is a fraction of the sample rate (0..0.5). Blackman window.
    let middle = (count - 1) as f32 / 2.0;
    let mut taps = (0..count)
        .map(|index| {
            let x = index as f32 - middle;
            let sinc = if x == 0.0 {
                2.0 * cutoff
            } else {
                (std::f32::consts::TAU * cutoff * x).sin() / (std::f32::consts::PI * x)
            };
            let phase = std::f32::consts::TAU * index as f32 / (count - 1) as f32;
            let window = 0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos();
            sinc * window
        })
        .collect::<Vec<_>>();
    let sum = taps.iter().sum::<f32>();
    for tap in &mut taps {
        *tap /= sum;
    }
    taps
}

struct Fir {
    taps: Vec<f32>,
    history: VecDeque<f32>,
}

impl Fir {
    fn new(taps: Vec<f32>) -> Self {
        let history = VecDeque::from(vec![0.0; taps.len()]);
        Self { taps, history }
    }

    fn push(&mut self, sample: f32) {
        self.history.pop_front();
        self.history.push_back(sample);
    }

    fn output(&self) -> f32 {
        self.history
            .iter()
            .zip(&self.taps)
            .map(|(sample, tap)| sample * tap)
            .sum()
    }
}

fn to_i16(sample: f32) -> i16 {
    sample.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

/// Downmixes interleaved 48 kHz stereo to mono and decimates by an integer factor.
pub struct StereoDownsampler {
    factor: usize,
    phase: usize,
    filter: Fir,
}

impl StereoDownsampler {
    pub fn new(factor: usize) -> Self {
        let factor = factor.max(1);
        let taps = 16 * factor + 1;
        Self {
            factor,
            phase: 0,
            filter: Fir::new(lowpass_taps(0.45 / factor as f32, taps)),
        }
    }

    pub fn process(&mut self, interleaved: &[i16]) -> Vec<i16> {
        let mut output = Vec::with_capacity(interleaved.len() / 2 / self.factor + 1);
        for frame in interleaved.chunks(2) {
            let left = frame[0] as f32;
            let right = frame.get(1).copied().unwrap_or(frame[0]) as f32;
            self.filter.push((left + right) * 0.5);
            self.phase += 1;
            if self.phase == self.factor {
                self.phase = 0;
                output.push(to_i16(self.filter.output()));
            }
        }
        output
    }
}

/// Upsamples mono PCM by an integer factor and duplicates it to interleaved stereo.
pub struct MonoToStereoUpsampler {
    factor: usize,
    filter: Fir,
}

impl MonoToStereoUpsampler {
    pub fn new(factor: usize) -> Self {
        let factor = factor.max(1);
        let taps = 16 * factor + 1;
        Self {
            factor,
            filter: Fir::new(lowpass_taps(0.45 / factor as f32, taps)),
        }
    }

    pub fn process(&mut self, mono: &[i16]) -> Vec<i16> {
        let mut output = Vec::with_capacity(mono.len() * self.factor * 2);
        let gain = self.factor as f32;
        for sample in mono {
            for step in 0..self.factor {
                self.filter.push(if step == 0 {
                    *sample as f32 * gain
                } else {
                    0.0
                });
                let value = to_i16(self.filter.output());
                output.push(value);
                output.push(value);
            }
        }
        output
    }
}

/// Converts arbitrary-rate PCM chunks to 48 kHz interleaved stereo. Integer ratios use the
/// FIR upsampler; anything else falls back to linear interpolation.
pub struct ToRelayFormat {
    source_rate: u32,
    source_channels: u16,
    upsampler: Option<MonoToStereoUpsampler>,
    linear_position: f64,
    linear_previous: f32,
}

impl ToRelayFormat {
    pub fn new(source_rate: u32, source_channels: u16) -> Self {
        let upsampler = (source_rate > 0 && 48_000 % source_rate == 0)
            .then(|| MonoToStereoUpsampler::new((48_000 / source_rate) as usize));
        Self {
            source_rate,
            source_channels: source_channels.max(1),
            upsampler,
            linear_position: 0.0,
            linear_previous: 0.0,
        }
    }

    pub fn matches(&self, source_rate: u32, source_channels: u16) -> bool {
        self.source_rate == source_rate && self.source_channels == source_channels.max(1)
    }

    pub fn process(&mut self, samples: &[i16]) -> Vec<i16> {
        let channels = self.source_channels as usize;
        let mono = samples
            .chunks(channels)
            .map(|frame| {
                (frame.iter().map(|sample| *sample as i32).sum::<i32>() / frame.len() as i32) as i16
            })
            .collect::<Vec<_>>();
        if let Some(upsampler) = self.upsampler.as_mut() {
            return upsampler.process(&mono);
        }
        if self.source_rate == 0 {
            return Vec::new();
        }
        let step = self.source_rate as f64 / 48_000.0;
        let mut output = Vec::new();
        for sample in mono {
            let current = sample as f32;
            while self.linear_position < 1.0 {
                let t = self.linear_position as f32;
                let value = to_i16(self.linear_previous + (current - self.linear_previous) * t);
                output.push(value);
                output.push(value);
                self.linear_position += step;
            }
            self.linear_position -= 1.0;
            self.linear_previous = current;
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_sine(frequency: f32, rate: f32, frames: usize, amplitude: f32) -> Vec<i16> {
        let mut samples = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let value = (std::f32::consts::TAU * frequency * frame as f32 / rate).sin() * amplitude;
            samples.push(value as i16);
            samples.push(value as i16);
        }
        samples
    }

    fn rms(samples: &[i16]) -> f32 {
        (samples.iter().map(|s| (*s as f32).powi(2)).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn downsampler_keeps_speech_band_and_rejects_aliases() {
        let mut down = StereoDownsampler::new(3);
        let speech = down.process(&stereo_sine(440.0, 48_000.0, 48_000, 10_000.0));
        assert_eq!(speech.len(), 16_000);
        let settled = &speech[1_000..];
        assert!(
            (rms(settled) - 7_071.0).abs() < 300.0,
            "rms {}",
            rms(settled)
        );

        let mut down = StereoDownsampler::new(3);
        let alias = down.process(&stereo_sine(14_000.0, 48_000.0, 48_000, 10_000.0));
        assert!(
            rms(&alias[1_000..]) < 200.0,
            "alias rms {}",
            rms(&alias[1_000..])
        );
    }

    #[test]
    fn upsampler_restores_rate_and_level() {
        let mono = stereo_sine(500.0, 24_000.0, 24_000, 8_000.0)
            .chunks(2)
            .map(|frame| frame[0])
            .collect::<Vec<_>>();
        let mut up = MonoToStereoUpsampler::new(2);
        let stereo = up.process(&mono);
        assert_eq!(stereo.len(), 48_000 * 2);
        let left = stereo.chunks(2).map(|frame| frame[0]).collect::<Vec<_>>();
        assert!(
            (rms(&left[2_000..]) - 5_657.0).abs() < 300.0,
            "rms {}",
            rms(&left[2_000..])
        );
        assert!(stereo.chunks(2).all(|frame| frame[0] == frame[1]));
    }

    #[test]
    fn relay_format_handles_integer_and_fractional_rates() {
        let mut integer = ToRelayFormat::new(24_000, 1);
        assert_eq!(integer.process(&vec![1_000; 480]).len(), 960 * 2);
        let mut fractional = ToRelayFormat::new(44_100, 2);
        let output = fractional.process(&vec![1_000; 441 * 2]);
        let frames = output.len() / 2;
        assert!((479..=481).contains(&frames), "frames {frames}");
        assert!(integer.matches(24_000, 1));
        assert!(!integer.matches(16_000, 1));
    }
}
