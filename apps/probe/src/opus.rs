//! Opus encode/decode round trip with the crate songbird already links.
//!
//! Pass criterion: a 1 kHz tone at 48 kHz, encoded and decoded in 20 ms mono frames,
//! comes back with a normalized cross-correlation above 0.9 once the codec delay is
//! compensated, and every frame encodes and decodes without error.

use crate::Outcome;
use anyhow::Context;
use opus2::{Application, Channels, Decoder, Encoder};

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SAMPLES: usize = 960; // 20 ms at 48 kHz, mono

/// A mono 1 kHz tone at 48 kHz, amplitude 0.5.
pub fn tone(seconds: f32) -> Vec<i16> {
    let count = (SAMPLE_RATE as f32 * seconds) as usize;
    (0..count)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            (0.5 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * i16::MAX as f32) as i16
        })
        .collect()
}

/// Normalized cross-correlation at the best lag within `max_lag` samples.
pub fn best_correlation(reference: &[i16], decoded: &[i16], max_lag: usize) -> (f32, usize) {
    let mut best = (f32::MIN, 0);
    let len = reference.len().min(decoded.len()).saturating_sub(max_lag);
    if len == 0 {
        return (0.0, 0);
    }
    for lag in 0..max_lag {
        let mut dot = 0f64;
        let mut energy_a = 0f64;
        let mut energy_b = 0f64;
        for index in 0..len {
            let a = reference[index] as f64;
            let b = decoded[index + lag] as f64;
            dot += a * b;
            energy_a += a * a;
            energy_b += b * b;
        }
        let corr = if energy_a > 0.0 && energy_b > 0.0 {
            (dot / (energy_a.sqrt() * energy_b.sqrt())) as f32
        } else {
            0.0
        };
        if corr > best.0 {
            best = (corr, lag);
        }
    }
    best
}

/// Largest absolute sample value.
pub fn best_peak(samples: &[i16]) -> i32 {
    samples
        .iter()
        .map(|sample| (*sample as i32).abs())
        .max()
        .unwrap_or(0)
}

pub fn new_encoder() -> anyhow::Result<Encoder> {
    let mut encoder =
        Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip).context("opus encoder")?;
    encoder.set_inband_fec(true).ok();
    encoder.set_complexity(5).ok();
    Ok(encoder)
}

pub fn roundtrip(_args: &[String]) -> anyhow::Result<Outcome> {
    let input = tone(2.0);
    let mut encoder = new_encoder()?;
    let mut decoder = Decoder::new(SAMPLE_RATE, Channels::Mono).context("opus decoder")?;
    let mut details = vec![format!("libopus {}", opus2::version())];

    let mut decoded = Vec::with_capacity(input.len());
    let mut packet = vec![0u8; 4000];
    let mut frame_out = vec![0i16; FRAME_SAMPLES];
    let mut bytes_total = 0usize;
    let mut frames = 0usize;
    for frame in input.chunks_exact(FRAME_SAMPLES) {
        let written = encoder
            .encode(frame, &mut packet)
            .with_context(|| format!("encode frame {frames}"))?;
        bytes_total += written;
        let samples = decoder
            .decode(&packet[..written], &mut frame_out, false)
            .with_context(|| format!("decode frame {frames}"))?;
        decoded.extend_from_slice(&frame_out[..samples]);
        frames += 1;
    }
    let lookahead = encoder.get_lookahead().unwrap_or(0);
    details.push(format!(
        "{frames} frames of 20 ms, {bytes_total} bytes ({:.1} kbit/s), lookahead {lookahead} samples",
        bytes_total as f32 * 8.0 / (frames as f32 * 0.02) / 1000.0
    ));
    // Skip the first 100 ms on both sides: the encoder ramps in.
    let skip = FRAME_SAMPLES * 5;
    let (corr, lag) = best_correlation(&input[skip..], &decoded[skip..], 400);
    details.push(format!("best correlation {corr:.3} at lag {lag} samples"));
    let pass = corr > 0.9 && decoded.len() == frames * FRAME_SAMPLES;
    details.push(format!(
        "criterion: correlation > 0.9 and {} decoded samples == {} expected",
        decoded.len(),
        frames * FRAME_SAMPLES
    ));
    Ok(Outcome { pass, details })
}
