//! Short tones that tell the channel what changed: a session started or ended, the
//! assistant was muted or unmuted. Generated here, mixed into the output even while muted.

use super::frame::SAMPLE_RATE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Earcon {
    /// Three rising notes: a session started.
    Activate,
    /// Two falling notes: a session ended.
    Deactivate,
    /// Two falling notes, shorter: the assistant was muted.
    Mute,
    /// Two rising notes, shorter: the assistant was unmuted.
    Unmute,
}

/// Fade in and out at each note's edges so the tone does not click.
const FADE_MS: f32 = 6.0;

/// Each note as (frequency in Hz, duration in ms).
pub fn notes(kind: Earcon) -> &'static [(f32, u32)] {
    match kind {
        Earcon::Mute => &[(660.0, 90), (440.0, 110)],
        Earcon::Unmute => &[(440.0, 90), (660.0, 110)],
        Earcon::Activate => &[(523.25, 70), (659.25, 70), (783.99, 110)],
        Earcon::Deactivate => &[(659.25, 80), (392.0, 130)],
    }
}

/// Mono 48 kHz samples in -1.0..1.0 scaled by `volume`.
pub fn samples(kind: Earcon, volume: f32) -> Vec<f32> {
    let volume = volume.clamp(0.0, 1.0);
    let rate = SAMPLE_RATE as f32;
    let fade = ((rate * FADE_MS / 1_000.0) as usize).max(1);
    let mut out = Vec::new();
    for (frequency, milliseconds) in notes(kind) {
        let count = (rate * *milliseconds as f32 / 1_000.0) as usize;
        for n in 0..count {
            let envelope = (n.min(count - 1 - n) as f32 / fade as f32).min(1.0);
            out.push(
                (std::f32::consts::TAU * frequency * n as f32 / rate).sin() * volume * envelope,
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dominant frequency of a block, by counting rising zero crossings.
    fn frequency(block: &[f32]) -> f32 {
        let crossings = block
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        crossings as f32 * SAMPLE_RATE as f32 / block.len() as f32
    }

    #[test]
    fn the_four_tones_have_their_lengths_and_first_notes() {
        let ms = |n: usize| n * SAMPLE_RATE as usize / 1_000;
        for (kind, total_ms, first_hz) in [
            (Earcon::Mute, 200, 660.0),
            (Earcon::Unmute, 200, 440.0),
            (Earcon::Activate, 250, 523.25),
            (Earcon::Deactivate, 210, 659.25),
        ] {
            let tone = samples(kind, 0.18);
            assert_eq!(tone.len(), ms(total_ms), "{kind:?}");
            let first_note = &tone[..ms(notes(kind)[0].1 as usize)];
            let measured = frequency(first_note);
            assert!(
                (measured - first_hz).abs() < first_hz * 0.05,
                "{kind:?}: {measured} Hz"
            );
        }
    }

    #[test]
    fn volume_scales_the_tone() {
        let tone = samples(Earcon::Activate, 0.18);
        let peak = tone.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.18 + 1e-6 && peak > 0.15, "peak {peak}");
        assert!(samples(Earcon::Mute, 0.0).iter().all(|s| *s == 0.0));
    }
}
