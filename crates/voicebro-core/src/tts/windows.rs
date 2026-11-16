//! The voices Windows already has (OneCore, WinRT), with no download (change
//! `whisper-mode`, design D9b): `Windows.Media.SpeechSynthesis.SpeechSynthesizer` writes a
//! 16 kHz mono 16-bit WAV, read by walking its RIFF chunks.

use super::{Synth, TtsError};

/// A voice Windows lists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WindowsVoice {
    /// `windows:<token>`: the last part of the voice's registry id.
    pub id: String,
    pub display_name: String,
    /// BCP 47 (`ru-RU`).
    pub language: String,
}

/// The 16-bit PCM samples of a RIFF WAV as floats, and its sample rate.
pub fn parse_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), TtsError> {
    let bad = |what: &str| TtsError::Synthesize(format!("the voice's WAV {what}"));
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("is not RIFF/WAVE"));
    }
    let mut index = 12;
    let (mut rate, mut channels, mut bits) = (0u32, 0u16, 0u16);
    while index + 8 <= bytes.len() {
        let id = &bytes[index..index + 4];
        let size = u32::from_le_bytes(bytes[index + 4..index + 8].try_into().expect("4")) as usize;
        let body = index + 8;
        let end = (body + size).min(bytes.len());
        if id == b"fmt " && size >= 16 {
            channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().expect("4"));
            bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
        } else if id == b"data" {
            if bits != 16 || channels == 0 {
                return Err(bad("is not 16-bit PCM"));
            }
            let samples: Vec<f32> = bytes[body..end]
                .as_chunks::<2>()
                .0
                .iter()
                .step_by(usize::from(channels))
                .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0)
                .collect();
            return Ok((samples, rate));
        }
        index = body + size + (size & 1);
    }
    Err(bad("has no data chunk"))
}

#[cfg(windows)]
mod imp {
    use super::{parse_wav, Synth, TtsError, WindowsVoice};
    use ::windows::core::HSTRING;
    use ::windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceInformation};
    use ::windows::Storage::Streams::DataReader;

    fn error(what: &str, error: ::windows::core::Error) -> TtsError {
        TtsError::Synthesize(format!("Windows speech: {what}: {error}"))
    }

    fn token(information: &VoiceInformation) -> Result<String, TtsError> {
        let id = information
            .Id()
            .map_err(|e| error("voice id", e))?
            .to_string();
        Ok(id.rsplit('\\').next().unwrap_or(&id).to_owned())
    }

    fn all() -> Result<Vec<VoiceInformation>, TtsError> {
        let voices = SpeechSynthesizer::AllVoices().map_err(|e| error("list", e))?;
        let count = voices.Size().map_err(|e| error("list", e))?;
        (0..count)
            .map(|index| voices.GetAt(index).map_err(|e| error("list", e)))
            .collect()
    }

    pub fn voices() -> Result<Vec<WindowsVoice>, TtsError> {
        all()?
            .iter()
            .map(|information| {
                Ok(WindowsVoice {
                    id: format!(
                        "{}{}",
                        crate::tts::catalog::WINDOWS_PREFIX,
                        token(information)?
                    ),
                    display_name: information
                        .DisplayName()
                        .map_err(|e| error("name", e))?
                        .to_string(),
                    language: information
                        .Language()
                        .map_err(|e| error("language", e))?
                        .to_string(),
                })
            })
            .collect()
    }

    pub struct WindowsSynth {
        synthesizer: SpeechSynthesizer,
    }

    impl WindowsSynth {
        /// The voice whose token is `token`.
        pub fn load(token_wanted: &str) -> Result<Self, TtsError> {
            let information = all()?
                .into_iter()
                .find(|information| token(information).is_ok_and(|found| found == token_wanted))
                .ok_or_else(|| TtsError::Load(format!("Windows has no voice {token_wanted}")))?;
            let synthesizer = SpeechSynthesizer::new().map_err(|e| error("create", e))?;
            synthesizer
                .SetVoice(&information)
                .map_err(|e| error("voice", e))?;
            Ok(Self { synthesizer })
        }
    }

    impl Synth for WindowsSynth {
        fn sample_rate(&self) -> u32 {
            16_000
        }

        fn synthesize(
            &mut self,
            text: &str,
            _: Option<&str>,
            speed: f32,
            _: &dyn Fn() -> bool,
        ) -> Result<Option<Vec<f32>>, TtsError> {
            self.synthesizer
                .Options()
                .and_then(|options| options.SetSpeakingRate(f64::from(speed)))
                .map_err(|e| error("speed", e))?;
            let stream = self
                .synthesizer
                .SynthesizeTextToStreamAsync(&HSTRING::from(text))
                .and_then(|operation| operation.join())
                .map_err(|e| error("synthesize", e))?;
            let size = stream.Size().map_err(|e| error("size", e))? as u32;
            let input = stream.GetInputStreamAt(0).map_err(|e| error("read", e))?;
            let reader = DataReader::CreateDataReader(&input).map_err(|e| error("read", e))?;
            reader
                .LoadAsync(size)
                .and_then(|operation| operation.join())
                .map_err(|e| error("read", e))?;
            let mut bytes = vec![0u8; size as usize];
            reader.ReadBytes(&mut bytes).map_err(|e| error("read", e))?;
            let (samples, rate) = parse_wav(&bytes)?;
            if rate != 16_000 {
                return Ok(Some(crate::audio::dsp::Resampler::convert(
                    &samples, rate, 16_000,
                )));
            }
            Ok(Some(samples))
        }
    }
}

#[cfg(windows)]
pub use imp::{voices, WindowsSynth};

#[cfg(not(windows))]
pub fn voices() -> Result<Vec<WindowsVoice>, TtsError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_is_read_through_its_chunks() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&0u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&3u32.to_le_bytes());
        wav.extend_from_slice(b"abc\0");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        for value in [1u16, 1] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&32_000u32.to_le_bytes());
        for value in [2u16, 16] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        for value in [16_384i16, -16_384] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        let (samples, rate) = parse_wav(&wav).unwrap();
        assert_eq!(rate, 16_000);
        assert_eq!(samples, vec![0.5, -0.5]);
    }

    /// Task 6.3 of change `whisper-mode`: the voices of this PC, each speaking 16 kHz audio.
    /// Windows only; no download.
    #[cfg(windows)]
    #[test]
    fn the_windows_voices_list_and_speak() {
        let found = voices().unwrap();
        eprintln!("{found:#?}");
        let names: Vec<&str> = found
            .iter()
            .map(|voice| voice.display_name.as_str())
            .collect();
        for wanted in ["David", "Zira", "Mark", "Irina", "Pavel"] {
            if !names.iter().any(|name| name.contains(wanted)) {
                eprintln!("{wanted} is not installed here; skipped");
            }
        }
        for voice in &found {
            let token = voice
                .id
                .strip_prefix(crate::tts::catalog::WINDOWS_PREFIX)
                .unwrap();
            let mut synth = WindowsSynth::load(token).unwrap();
            let text = if voice.language.starts_with("ru") {
                "Сегодня в Лондоне облачно."
            } else {
                "It is cloudy in London today."
            };
            let started = std::time::Instant::now();
            let audio = synth
                .synthesize(text, None, 1.0, &|| false)
                .unwrap()
                .unwrap();
            let peak = audio.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
            eprintln!(
                "{}: {:.2} s of audio in {:?}, peak {peak:.2}",
                voice.display_name,
                audio.len() as f64 / 16_000.0,
                started.elapsed()
            );
            assert!(audio.len() > 8_000 && peak > 0.05, "{}", voice.display_name);
        }
    }
}
