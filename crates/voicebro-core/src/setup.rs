//! What the current settings need on disk (change `cross-platform-headless`, design D3):
//! the same choices a join makes, for the headless app's `--install-assets` and its list of
//! missing files at start.

use crate::assets::{self, Asset};
use crate::settings::{AssistantMode, SessionMode, Settings};
use crate::wake::vosk::VoskEngine;
use crate::wake::{ModelSource, WakeEngine};
use std::path::Path;

/// Every file the settings' mode needs that is not installed under `data_dir`'s assets
/// folder, in install order: the wake word's runtime and model in the realtime mode with
/// the wake word; the Whisper runtime for the chosen device, the model and the VAD model in
/// the Whisper and text answers modes; the voice engine, the phonemizer when the voice
/// needs it, and the voice in the Whisper mode. A local model is never downloaded: a
/// missing one is an error naming its folder.
pub fn missing_assets(settings: &Settings, data_dir: &Path) -> Result<Vec<Asset>, String> {
    let assets_dir = crate::paths::assets_dir(data_dir);
    let mut needed = Vec::new();
    match settings.assistant.mode {
        AssistantMode::Realtime => {
            if settings.session.mode == SessionMode::WakeWord {
                let source = ModelSource::from_id(&settings.wake.model)
                    .ok_or_else(|| format!("«{}» is not a wake model", settings.wake.model))?;
                if let ModelSource::Local { .. } = &source {
                    let folder = source.directory(&assets_dir, &crate::paths::models_dir(data_dir));
                    if !crate::wake::local::is_model_dir(&folder) {
                        return Err(format!("no Vosk model in {}", folder.display()));
                    }
                }
                needed.extend(VoskEngine.assets(&source));
            }
        }
        AssistantMode::Whisper | AssistantMode::Text => {
            let whisper = &settings.whisper;
            needed.push(crate::stt::catalog::runtime_for(
                whisper.device,
                &assets_dir,
            ));
            match crate::stt::catalog::find(&whisper.model) {
                Some(entry) => needed.push(entry.asset),
                None => {
                    let folder = crate::paths::whisper_models_dir(data_dir);
                    let path = crate::stt::local::path(&folder, &whisper.model)
                        .ok_or_else(|| format!("«{}» is not a Whisper model", whisper.model))?;
                    if !path.is_file() {
                        return Err(format!(
                            "the Whisper model {} is not in the models folder",
                            path.display()
                        ));
                    }
                }
            }
            needed.push(crate::stt::catalog::VAD_MODEL);
            if settings.assistant.mode == AssistantMode::Whisper {
                let voice = &settings.tts.voice;
                if crate::tts::catalog::windows_voice(voice).is_none() {
                    let entry = crate::tts::catalog::find(voice)
                        .ok_or_else(|| format!("«{voice}» is not a voice"))?;
                    needed.extend(crate::tts::catalog::assets_for(entry));
                }
            }
        }
    }
    Ok(needed
        .into_iter()
        .filter(|asset| !assets::is_installed(&assets_dir, asset))
        .collect())
}
