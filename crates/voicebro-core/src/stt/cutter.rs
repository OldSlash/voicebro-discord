//! Cuts each speaker's audio into utterances for Whisper (change `whisper-mode`, designs D2
//! and D3). Silence is counted in the guild's receive ticks, as for the wake engine, so a busy
//! transcriber or a late tick never ends an utterance early. Cheap: it only downsamples and
//! appends; the transcriber thread does the heavy work.

use crate::audio::dsp::StereoDownsampler;
use crate::audio::peak;
use std::collections::HashMap;
use std::time::Instant;

/// An utterance ends once its speaker sent nothing for this many receive ticks (700 ms):
/// longer than songbird's refill gaps and the gate's own utterance gap.
pub const END_AFTER_TICKS: u64 = 35;
/// Whisper's window is 30 s; an utterance is cut at 28 s and the rest starts a new one.
pub const MAX_SECONDS: f64 = 28.0;
/// Fewer voiced 20 ms frames than this is a click or a breath, not speech (240 ms).
pub const MIN_VOICED_FRAMES: u32 = 12;
/// A frame with this peak or more is voiced (the gate's speech threshold).
pub const VOICED_PEAK: i32 = 700;
/// Silence added after the speech, and the shortest clip handed to Whisper, which returns
/// nothing for clips under 1 s.
pub const TRAILING_SILENCE_SECONDS: f64 = 0.3;
pub const MIN_CLIP_SECONDS: f64 = 1.0;

const RATE: usize = super::SAMPLE_RATE as usize;

/// A finished utterance, ready for Whisper.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub guild: u64,
    pub ssrc: u32,
    /// 16 kHz mono in -1..1, with the trailing silence and the padding.
    pub audio: Vec<f32>,
    /// The speech itself, before the padding.
    pub speech_seconds: f64,
    /// When the gate handed over the utterance's last frame: the end of speech for the
    /// latency line.
    pub speech_end: Instant,
    /// The utterance was cut at [`MAX_SECONDS`] and goes on in the next one.
    pub cut: bool,
}

/// What ending an utterance gave.
#[derive(Debug, Clone, PartialEq)]
pub enum Cut {
    Utterance(Utterance),
    /// Too little voiced audio: dropped without transcription.
    Noise {
        guild: u64,
        ssrc: u32,
        voiced_frames: u32,
    },
}

struct Speaker {
    downsampler: StereoDownsampler,
    audio: Vec<f32>,
    voiced_frames: u32,
    last_tick: u64,
    last_frame_at: Instant,
}

/// The cutter's state for every speaker of every guild.
#[derive(Default)]
pub struct Cutter {
    speakers: HashMap<(u64, u32), Speaker>,
}

fn finish(guild: u64, ssrc: u32, speaker: &mut Speaker, cut: bool) -> Cut {
    let mut audio = std::mem::take(&mut speaker.audio);
    let voiced_frames = std::mem::take(&mut speaker.voiced_frames);
    if voiced_frames < MIN_VOICED_FRAMES {
        return Cut::Noise {
            guild,
            ssrc,
            voiced_frames,
        };
    }
    let speech_seconds = audio.len() as f64 / RATE as f64;
    audio.extend(std::iter::repeat_n(
        0.0,
        (TRAILING_SILENCE_SECONDS * RATE as f64) as usize,
    ));
    let minimum = (MIN_CLIP_SECONDS * RATE as f64) as usize;
    if audio.len() < minimum {
        audio.resize(minimum, 0.0);
    }
    Cut::Utterance(Utterance {
        guild,
        ssrc,
        audio,
        speech_seconds,
        speech_end: speaker.last_frame_at,
        cut,
    })
}

impl Cutter {
    pub fn new() -> Self {
        Self::default()
    }

    /// One speaker's 48 kHz stereo frame of receive tick `tick`, handed over at `at`.
    /// Returns the utterance when it reached [`MAX_SECONDS`].
    pub fn on_frame(
        &mut self,
        guild: u64,
        ssrc: u32,
        tick: u64,
        at: Instant,
        frame: &[i16],
    ) -> Option<Cut> {
        let speaker = self
            .speakers
            .entry((guild, ssrc))
            .or_insert_with(|| Speaker {
                downsampler: StereoDownsampler::new(3),
                audio: Vec::new(),
                voiced_frames: 0,
                last_tick: tick,
                last_frame_at: at,
            });
        let mono = speaker.downsampler.process(frame);
        speaker
            .audio
            .extend(mono.iter().map(|&sample| f32::from(sample) / 32_768.0));
        if peak(frame) >= VOICED_PEAK {
            speaker.voiced_frames += 1;
        }
        speaker.last_tick = tick;
        speaker.last_frame_at = at;
        (speaker.audio.len() as f64 / RATE as f64 >= MAX_SECONDS)
            .then(|| finish(guild, ssrc, speaker, true))
    }

    /// Receive tick `tick` of `guild` ended: every speaker absent from
    /// [`END_AFTER_TICKS`] ticks ends their utterance.
    pub fn on_tick(&mut self, guild: u64, tick: u64) -> Vec<Cut> {
        let mut cuts = Vec::new();
        self.speakers.retain(|(speaker_guild, ssrc), speaker| {
            if *speaker_guild != guild || tick.saturating_sub(speaker.last_tick) < END_AFTER_TICKS {
                return true;
            }
            if !speaker.audio.is_empty() {
                cuts.push(finish(guild, *ssrc, speaker, false));
            }
            false
        });
        cuts
    }

    /// Drops every speaker of `guild` (the bot left, or mute ended the session).
    pub fn clear_guild(&mut self, guild: u64) {
        self.speakers
            .retain(|(speaker_guild, _), _| *speaker_guild != guild);
    }

    pub fn speakers(&self) -> usize {
        self.speakers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voiced() -> Vec<i16> {
        vec![2_000; 1920]
    }

    fn quiet() -> Vec<i16> {
        vec![0; 1920]
    }

    /// Feeds `frames` (true: voiced) from `start`, one per tick, then ticks without frames
    /// until `until`; returns the cuts.
    fn play(cutter: &mut Cutter, start: u64, frames: &[bool], until: u64) -> Vec<Cut> {
        let at = Instant::now();
        let mut cuts = Vec::new();
        let mut tick = start;
        for &frame in frames {
            if let Some(cut) =
                cutter.on_frame(1, 7, tick, at, &if frame { voiced() } else { quiet() })
            {
                cuts.push(cut);
            }
            cuts.extend(cutter.on_tick(1, tick));
            tick += 1;
        }
        while tick <= until {
            cuts.extend(cutter.on_tick(1, tick));
            tick += 1;
        }
        cuts
    }

    fn utterances(cuts: &[Cut]) -> Vec<&Utterance> {
        cuts.iter()
            .filter_map(|cut| match cut {
                Cut::Utterance(utterance) => Some(utterance),
                Cut::Noise { .. } => None,
            })
            .collect()
    }

    /// Task 5.1: a 300 ms pause (15 ticks without frames) stays one utterance; 35 silent
    /// ticks end it, with the trailing silence.
    #[test]
    fn a_pause_inside_a_sentence_stays_one_utterance() {
        let mut cutter = Cutter::new();
        let at = Instant::now();
        for tick in 0..20 {
            cutter.on_frame(1, 7, tick, at, &voiced());
            assert!(cutter.on_tick(1, tick).is_empty());
        }
        for tick in 20..35 {
            assert!(cutter.on_tick(1, tick).is_empty(), "a pause of 15 ticks");
        }
        for tick in 35..55 {
            cutter.on_frame(1, 7, tick, at, &voiced());
            assert!(cutter.on_tick(1, tick).is_empty());
        }
        let mut cuts = Vec::new();
        for tick in 55..=54 + END_AFTER_TICKS {
            cuts.extend(cutter.on_tick(1, tick));
            if tick < 54 + END_AFTER_TICKS {
                assert!(cuts.is_empty(), "ended early at tick {tick}");
            }
        }
        let found = utterances(&cuts);
        assert_eq!(found.len(), 1);
        assert!(
            (found[0].speech_seconds - 0.8).abs() < 1e-9,
            "40 frames of 20 ms"
        );
        assert!((found[0].audio.len() as f64 / 16_000.0 - 1.1).abs() < 1e-9);
        assert_eq!(cutter.speakers(), 0);
    }

    /// Task 5.1: 28 s cuts and the rest continues as the next utterance.
    #[test]
    fn a_long_monologue_is_cut_at_28_seconds_and_goes_on() {
        let mut cutter = Cutter::new();
        let cuts = play(&mut cutter, 0, &vec![true; 1_500], 1_540);
        let found = utterances(&cuts);
        assert_eq!(found.len(), 2, "{} cuts", cuts.len());
        assert!(found[0].cut && (found[0].speech_seconds - 28.0).abs() < 1e-9);
        assert!(!found[1].cut && (found[1].speech_seconds - 2.0).abs() < 1e-9);
    }

    /// Task 5.1: a 100 ms click is dropped as noise; padding makes a short utterance 1 s.
    #[test]
    fn a_click_is_noise_and_a_short_utterance_is_padded() {
        let mut cutter = Cutter::new();
        let cuts = play(&mut cutter, 0, &[true; 5], 60);
        assert_eq!(
            cuts,
            vec![Cut::Noise {
                guild: 1,
                ssrc: 7,
                voiced_frames: 5
            }]
        );
        let cuts = play(&mut cutter, 100, &[true; 12], 160);
        let found = utterances(&cuts);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].audio.len(), 16_000, "padded to 1 s");
    }

    /// Task 5.1: frames that reach the cutter late and in a burst, with their ticks in
    /// order, do not end the utterance: the cutter counts ticks, not its own clock.
    #[test]
    fn frames_arriving_late_in_a_burst_do_not_end_the_utterance() {
        let mut cutter = Cutter::new();
        let late = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(50));
        for tick in 0..30 {
            cutter.on_frame(1, 7, tick, late, &voiced());
            assert!(cutter.on_tick(1, tick).is_empty());
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
        for tick in 30..60 {
            cutter.on_frame(1, 7, tick, late, &voiced());
            assert!(
                cutter.on_tick(1, tick).is_empty(),
                "the burst after a stall is still one utterance"
            );
        }
        assert_eq!(cutter.speakers(), 1);
        let cuts = cutter.on_tick(1, 59 + END_AFTER_TICKS);
        assert_eq!(utterances(&cuts).len(), 1);
        assert!((utterances(&cuts)[0].speech_seconds - 1.2).abs() < 1e-9);
    }

    #[test]
    fn speakers_and_guilds_are_cut_apart() {
        let mut cutter = Cutter::new();
        let at = Instant::now();
        for tick in 0..20 {
            cutter.on_frame(1, 7, tick, at, &voiced());
            cutter.on_frame(2, 7, tick, at, &voiced());
        }
        assert!(cutter.on_tick(2, 30).is_empty());
        assert_eq!(utterances(&cutter.on_tick(1, 60)).len(), 1);
        assert_eq!(cutter.speakers(), 1, "guild 2's speaker is still talking");
        cutter.clear_guild(2);
        assert_eq!(cutter.speakers(), 0);
    }
}
