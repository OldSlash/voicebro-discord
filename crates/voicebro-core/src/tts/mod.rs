//! The local voice of the Whisper mode (change `whisper-mode`, designs D7 and D9b):
//! sherpa-onnx with Supertonic 3, Piper and Kokoro, and the voices Windows has; the
//! sentence splitter, the player on the guild's output and the manager that loads a voice
//! at a join. See `docs/ARCHITECTURE.md` 4.12.

pub mod catalog;
pub mod manager;
pub mod phonemize;
pub mod player;
pub mod sherpa;
pub mod text;
pub mod windows;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TtsError {
    #[error("{0}")]
    Load(String),
    #[error("{0}")]
    Synthesize(String),
}

/// A loaded voice. One thread owns it.
pub trait Synth: Send {
    /// The rate of what [`Synth::synthesize`] returns.
    fn sample_rate(&self) -> u32;
    /// Mono audio in -1..1 of `text`; `None` when `stop` turned true during the synthesis.
    /// `language` is the ISO 639-1 code of the text, when known.
    fn synthesize(
        &mut self,
        text: &str,
        language: Option<&str>,
        speed: f32,
        stop: &dyn Fn() -> bool,
    ) -> Result<Option<Vec<f32>>, TtsError>;
}

/// The sentence Preview reads, in the voice's language: the app's own neutral sample.
pub fn preview_sentence(language: &str) -> &'static str {
    match language.split('-').next().unwrap_or_default() {
        "ru" => "Сегодня в Лондоне облачно, около девяти градусов, вечером возможен дождь.",
        "cs" => "Dnes je v Londýně zataženo, kolem devíti stupňů, večer může pršet.",
        "nl" => "Vandaag is het bewolkt in Londen, rond de negen graden, vanavond kan het regenen.",
        "fr" => {
            "Aujourd'hui, il fait nuageux à Londres, environ neuf degrés, avec de la pluie ce soir."
        }
        "de" => "Heute ist es in London bewölkt, etwa neun Grad, am Abend kann es regnen.",
        "it" => "Oggi a Londra è nuvoloso, circa nove gradi, stasera potrebbe piovere.",
        "pl" => "Dziś w Londynie jest pochmurno, około dziewięciu stopni, wieczorem może padać.",
        "pt" => "Hoje está nublado em Londres, cerca de nove graus, e pode chover à noite.",
        "es" => "Hoy está nublado en Londres, unos nueve grados, y puede llover por la noche.",
        _ => "It is cloudy in London today, around nine degrees, with some rain in the evening.",
    }
}

/// 16-bit mono WAV bytes of `samples` at `rate`, for Preview and the probe.
pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut bytes = Vec::with_capacity(44 + data as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes());
    }
    bytes
}
