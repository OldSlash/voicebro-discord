//! Speech to text for the Whisper mode (change `whisper-mode`): whisper.cpp in the app's
//! process, its catalog, the utterance cutter, the transcriber and the manager that loads it
//! at a join and unloads it after the last leave. See `docs/ARCHITECTURE.md` 4.12.

pub mod catalog;
pub mod cutter;
pub mod local;
pub mod manager;
pub mod phrases;
pub mod transcriber;
pub mod whisper;

/// 16 kHz mono: what Whisper consumes.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SttError {
    #[error("{0}")]
    Load(String),
    #[error("{0}")]
    Transcribe(String),
    #[error("{0}")]
    Assets(String),
}

/// One segment of a transcription, before the noise guards.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSegment {
    pub text: String,
    pub no_speech_prob: f32,
}

/// What the model returned for one utterance, before the noise guards.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawTranscript {
    pub segments: Vec<RawSegment>,
    /// The language used or detected (ISO 639-1).
    pub language: Option<String>,
}

/// How to decode (design D5), from the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeOptions {
    /// `auto`, or a code Whisper knows.
    pub language: String,
    /// CPU threads; 0 is the automatic value.
    pub threads: u32,
    /// Whisper's `initial_prompt`: the phrases, so they are spelled as typed (D4).
    pub prompt: Option<String>,
}

/// Raised to stop a transcription early: the bot left the utterance's channel, or the model
/// unloads (change `whisper-config-layout`, design D8).
#[derive(Debug, Clone, Default)]
pub struct AbortFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl AbortFlag {
    pub fn raise(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn lower(&self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }

    pub fn is_raised(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }

    /// The flag itself, for a C callback that reads it during the call.
    pub fn as_atomic(&self) -> &std::sync::atomic::AtomicBool {
        &self.0
    }
}

/// A loaded speech-to-text model. One thread owns it.
pub trait SpeechToText: Send {
    /// Transcribes 16 kHz mono samples in -1..1 with the VAD.
    fn transcribe(
        &mut self,
        audio: &[f32],
        options: &DecodeOptions,
    ) -> Result<RawTranscript, SttError>;

    /// As [`Self::transcribe`], stopping early once `abort` is raised; what an aborted
    /// transcription returns is not used. A model that cannot stop early runs to the end.
    fn transcribe_until(
        &mut self,
        audio: &[f32],
        options: &DecodeOptions,
        abort: &AbortFlag,
    ) -> Result<RawTranscript, SttError> {
        let _ = abort;
        self.transcribe(audio, options)
    }
}

/// The thread count of `whisper.threads = 0`, set from the measurement of task 5.6 of
/// change `whisper-mode` (design D5): half the logical processors, at most 16. On the
/// 24-core test PC that is 12, which beat 4, 6 and 8 for every model.
pub fn auto_threads() -> u32 {
    let logical = std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(4);
    (logical / 2).clamp(1, 16)
}

/// A 16-bit PCM WAV as Whisper takes it: 16 kHz mono floats in -1..1. A 48 kHz file goes
/// through the app's own downsampler; a stereo file is mixed to mono. For the probes and the
/// gated tests.
pub fn read_wav_16k(path: &std::path::Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let chunk = |id: &[u8]| {
        bytes
            .windows(4)
            .position(|window| window == id)
            .map(|index| index + 8)
            .ok_or_else(|| {
                format!(
                    "{}: no {} chunk",
                    path.display(),
                    String::from_utf8_lossy(id)
                )
            })
    };
    let format = chunk(b"fmt ")?;
    let field = |offset: usize, length: usize| -> Result<u32, String> {
        bytes
            .get(format + offset..format + offset + length)
            .map(|raw| {
                raw.iter()
                    .rev()
                    .fold(0u32, |value, &byte| value << 8 | u32::from(byte))
            })
            .ok_or_else(|| format!("{}: a short fmt chunk", path.display()))
    };
    let (channels, rate, bits) = (field(2, 2)?, field(4, 4)?, field(14, 2)?);
    if bits != 16 || !(1..=2).contains(&channels) {
        return Err(format!(
            "{}: want 16-bit mono or stereo, got {bits}-bit with {channels} channels",
            path.display()
        ));
    }
    let data = chunk(b"data")?;
    let samples: Vec<i16> = bytes[data..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes(*pair))
        .collect();
    let mono: Vec<i16> = if channels == 2 {
        samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[left, right]| ((i32::from(*left) + i32::from(*right)) / 2) as i16)
            .collect()
    } else {
        samples
    };
    let mono = match rate {
        16_000 => mono,
        48_000 => {
            let stereo: Vec<i16> = mono.iter().flat_map(|&sample| [sample, sample]).collect();
            crate::audio::dsp::StereoDownsampler::new(3).process(&stereo)
        }
        other => {
            return Err(format!(
                "{}: want 16 or 48 kHz, got {other}",
                path.display()
            ))
        }
    };
    Ok(mono
        .iter()
        .map(|&sample| f32::from(sample) / 32_768.0)
        .collect())
}
