//! Short tones that tell the channel a session started or ended. Generated here, mixed into
//! the output even while muted.

use super::frame::SAMPLE_RATE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Earcon {
    /// Two rising notes: a session started.
    Activate,
    /// Two falling notes: a session ended.
    Deactivate,
}

const NOTE_MS: usize = 110;
const GAP_MS: usize = 30;
const LOW_HZ: f32 = 523.25; // C5
const HIGH_HZ: f32 = 783.99; // G5

/// Mono 48 kHz samples in -1.0..1.0 scaled by `volume`.
pub fn samples(kind: Earcon, volume: f32) -> Vec<f32> {
    let (first, second) = match kind {
        Earcon::Activate => (LOW_HZ, HIGH_HZ),
        Earcon::Deactivate => (HIGH_HZ, LOW_HZ),
    };
    let note = SAMPLE_RATE as usize * NOTE_MS / 1000;
    let gap = SAMPLE_RATE as usize * GAP_MS / 1000;
    let volume = volume.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(note * 2 + gap);
    for (index, frequency) in [first, second].into_iter().enumerate() {
        if index == 1 {
            out.extend(std::iter::repeat_n(0.0, gap));
        }
        for n in 0..note {
            let t = n as f32 / SAMPLE_RATE as f32;
            // Short linear fade in and out so the tone does not click.
            let fade_len = (note / 10) as f32;
            let fade = (n as f32 / fade_len).min(1.0) * ((note - n) as f32 / fade_len).min(1.0);
            out.push((std::f32::consts::TAU * frequency * t).sin() * volume * fade);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earcons_have_two_notes_and_respect_volume() {
        let activate = samples(Earcon::Activate, 0.18);
        let deactivate = samples(Earcon::Deactivate, 0.18);
        assert_eq!(activate.len(), deactivate.len());
        assert_eq!(
            activate.len(),
            SAMPLE_RATE as usize * (2 * NOTE_MS + GAP_MS) / 1000
        );
        let peak = activate.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.18 + 1e-6 && peak > 0.15, "peak {peak}");
        assert_eq!(
            samples(Earcon::Activate, 0.0)
                .iter()
                .filter(|s| **s != 0.0)
                .count(),
            0
        );
    }
}
