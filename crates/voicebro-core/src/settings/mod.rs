//! One settings document for the whole app, validated in one place.
//!
//! Every section has a default, unknown fields survive a load/save round trip, and a
//! document from an older schema version loads with its missing fields filled in. Secrets
//! are never part of this document; see [`crate::secrets`].

pub mod store;

use crate::wake::catalog;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The schema version this build writes.
pub const CURRENT_VERSION: u32 = 1;

pub const DEFAULT_MUTE_HOTKEY: &str = "Ctrl+Alt+M";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub discord: DiscordSettings,
    pub session: SessionSettings,
    pub wake: WakeSettings,
    pub assistant: AssistantSettings,
    pub codex: CodexSettings,
    pub hotkey: HotkeySettings,
    /// Fields this build does not know (from a newer version). Kept through load and save.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            discord: DiscordSettings::default(),
            session: SessionSettings::default(),
            wake: WakeSettings::default(),
            assistant: AssistantSettings::default(),
            codex: CodexSettings::default(),
            hotkey: HotkeySettings::default(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordSettings {
    /// Connect the bot at start-up when a token is stored.
    pub auto_connect: bool,
    /// The server whose slash commands are registered directly (they appear at once).
    /// Empty registers them globally, which Discord can take up to an hour to show.
    pub guild_id: String,
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            auto_connect: true,
            guild_id: String::new(),
        }
    }
}

impl DiscordSettings {
    pub fn guild_id_value(&self) -> Option<u64> {
        let trimmed = self.guild_id.trim();
        if trimmed.is_empty() {
            None
        } else {
            trimmed.parse().ok()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// The assistant hears the channel whenever the bot is connected.
    Always,
    /// A session opens on an activation phrase and closes on a deactivation phrase or
    /// the idle timeout.
    WakeWord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionSettings {
    pub mode: SessionMode,
    /// Seconds without input before a session ends. 0 disables; otherwise 5..=3600.
    pub idle_timeout_seconds: u32,
    pub earcons: bool,
    /// 0.0..=1.0
    pub earcon_volume: f32,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            mode: SessionMode::WakeWord,
            idle_timeout_seconds: 60,
            earcons: true,
            earcon_volume: 0.18,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WakeSettings {
    /// Engine id from the catalog (`vosk`).
    pub engine: String,
    /// Model id from the catalog.
    pub model: String,
    pub activation_phrases: Vec<String>,
    pub deactivation_phrases: Vec<String>,
}

impl Default for WakeSettings {
    /// No phrases by default: the user chooses them in the settings. The wake engine
    /// reports an empty list in its status instead of listening for nothing.
    fn default() -> Self {
        let model = catalog::default_entry();
        Self {
            engine: model.engine.to_owned(),
            model: model.id.to_owned(),
            activation_phrases: Vec::new(),
            deactivation_phrases: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerLabels {
    /// Names only at call start, inside the initial items.
    #[default]
    Start,
    /// Also a note when the speaker changes.
    Changes,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantSettings {
    pub engine: String,
    pub speaker_labels: SpeakerLabels,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            engine: "codex".to_owned(),
            speaker_labels: SpeakerLabels::Start,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CodexSettings {
    /// Path to `codex.exe`; empty means auto-detect.
    pub executable: String,
    /// Empty means the account default.
    pub model: String,
    /// Empty means the model default.
    pub reasoning_effort: String,
    pub voice: String,
    /// Instructions for the assistant. There is no default: the user writes their own.
    pub instructions: String,
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            executable: String::new(),
            model: String::new(),
            reasoning_effort: String::new(),
            voice: "juniper".to_owned(),
            instructions: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySettings {
    /// Global mute toggle, `Ctrl+Alt+M` style. Empty disables it.
    pub mute: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            mute: DEFAULT_MUTE_HOTKEY.to_owned(),
        }
    }
}

/// A rejected settings document: which field and why.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {message}")]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

fn invalid(field: &str, message: impl Into<String>) -> ValidationError {
    ValidationError {
        field: field.to_owned(),
        message: message.into(),
    }
}

/// Lower-cased, trimmed, empty entries removed. The form the recognizer matches against.
pub fn normalize_phrases(phrases: &[String]) -> Vec<String> {
    phrases
        .iter()
        .map(|phrase| phrase.trim().to_lowercase())
        .filter(|phrase| !phrase.is_empty())
        .collect()
}

const MODIFIERS: &[&str] = &[
    "ctrl", "control", "alt", "shift", "super", "win", "cmd", "meta",
];

/// `Ctrl+Alt+M`: one or more modifiers plus exactly one key, or empty.
fn validate_shortcut(field: &str, shortcut: &str) -> Result<(), ValidationError> {
    if shortcut.trim().is_empty() {
        return Ok(());
    }
    let tokens: Vec<&str> = shortcut.split('+').map(str::trim).collect();
    if tokens.iter().any(|token| token.is_empty()) {
        return Err(invalid(field, "empty part in the shortcut"));
    }
    let keys = tokens
        .iter()
        .filter(|token| !MODIFIERS.contains(&token.to_lowercase().as_str()))
        .count();
    if keys != 1 {
        return Err(invalid(
            field,
            "a shortcut needs modifiers plus exactly one key, like Ctrl+Alt+M",
        ));
    }
    if tokens.len() < 2 {
        return Err(invalid(field, "a shortcut needs at least one modifier"));
    }
    Ok(())
}

impl Settings {
    /// The only validator. Returns the first problem with its field path.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let guild = self.discord.guild_id.trim();
        if !guild.is_empty() && !guild.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid(
                "discord.guild_id",
                "a server id is digits only; leave it empty to register commands globally",
            ));
        }
        let timeout = self.session.idle_timeout_seconds;
        if timeout != 0 && !(5..=3600).contains(&timeout) {
            return Err(invalid(
                "session.idle_timeout_seconds",
                format!("{timeout} is outside 0 or 5..3600"),
            ));
        }
        if !(0.0..=1.0).contains(&self.session.earcon_volume) {
            return Err(invalid("session.earcon_volume", "must be between 0 and 1"));
        }

        // Empty lists are allowed: they are the starting state, and the wake engine shows
        // them as "no phrases configured" rather than the save being refused.
        let activation = normalize_phrases(&self.wake.activation_phrases);
        let deactivation = normalize_phrases(&self.wake.deactivation_phrases);
        if let Some(shared) = activation
            .iter()
            .find(|phrase| deactivation.contains(phrase))
        {
            return Err(invalid(
                "wake.deactivation_phrases",
                format!("«{shared}» is in both the activation and the deactivation list"),
            ));
        }
        match catalog::find(&self.wake.model) {
            None => {
                return Err(invalid(
                    "wake.model",
                    format!("«{}» is not in the model catalog", self.wake.model),
                ))
            }
            Some(entry) if entry.engine != self.wake.engine => {
                return Err(invalid(
                    "wake.engine",
                    format!(
                        "model «{}» belongs to engine «{}», not «{}»",
                        entry.id, entry.engine, self.wake.engine
                    ),
                ))
            }
            Some(_) => {}
        }

        if self.assistant.engine != "codex" {
            return Err(invalid(
                "assistant.engine",
                format!(
                    "«{}» is not a known assistant engine",
                    self.assistant.engine
                ),
            ));
        }
        if self.codex.voice.trim().is_empty() {
            return Err(invalid("codex.voice", "a voice is needed"));
        }
        validate_shortcut("hotkey.mute", &self.hotkey.mute)?;
        Ok(())
    }

    /// Applies a JSON patch (a partial document, nested objects merged key by key) and
    /// returns the resulting document. Validation is the caller's next step.
    pub fn with_patch(&self, patch: &Value) -> Result<Settings, serde_json::Error> {
        let mut current = serde_json::to_value(self)?;
        merge(&mut current, patch);
        let mut merged: Settings = serde_json::from_value(current)?;
        merged.version = CURRENT_VERSION;
        Ok(merged)
    }

    /// Which top-level sections differ between two documents.
    pub fn changed_sections(&self, other: &Settings) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if self.discord != other.discord {
            changed.push("discord");
        }
        if self.session != other.session {
            changed.push("session");
        }
        if self.wake != other.wake {
            changed.push("wake");
        }
        if self.assistant != other.assistant {
            changed.push("assistant");
        }
        if self.codex != other.codex {
            changed.push("codex");
        }
        if self.hotkey != other.hotkey {
            changed.push("hotkey");
        }
        changed
    }
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(existing) if existing.is_object() && value.is_object() => {
                        merge(existing, value)
                    }
                    _ => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_validate() {
        Settings::default().validate().expect("defaults are valid");
    }

    #[test]
    fn unknown_fields_survive_a_round_trip_and_missing_ones_take_defaults() {
        let text = r#"{ "version": 1, "session": { "idle_timeout_seconds": 120 }, "future_field": { "a": 1 } }"#;
        let settings: Settings = serde_json::from_str(text).expect("parses");
        assert_eq!(settings.session.idle_timeout_seconds, 120);
        assert!(settings.session.earcons, "missing field takes the default");
        assert_eq!(settings.codex.voice, "juniper");
        let back = serde_json::to_value(&settings).expect("serializes");
        assert_eq!(back["future_field"]["a"], 1);
        assert_eq!(back["hotkey"]["mute"], DEFAULT_MUTE_HOTKEY);
    }

    #[test]
    fn overlapping_phrases_are_rejected_with_both_lists_named() {
        let mut settings = Settings::default();
        settings.wake.activation_phrases = vec!["alpha".to_owned(), "beta".to_owned()];
        settings.wake.deactivation_phrases = vec![" BETA ".to_owned()];
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "wake.deactivation_phrases");
        assert!(error.message.contains("beta"), "{}", error.message);
        assert!(error.message.contains("activation"), "{}", error.message);
    }

    #[test]
    fn guild_id_is_digits_or_empty() {
        let mut settings = Settings::default();
        settings.discord.guild_id = " 1091401169441656933 ".to_owned();
        settings.validate().expect("digits with spaces are fine");
        assert_eq!(settings.discord.guild_id_value(), Some(1091401169441656933));
        settings.discord.guild_id = "my-server".to_owned();
        assert_eq!(settings.validate().unwrap_err().field, "discord.guild_id");
        settings.discord.guild_id = String::new();
        settings.validate().expect("empty means global");
        assert_eq!(settings.discord.guild_id_value(), None);
    }

    #[test]
    fn phrase_lists_start_empty_and_that_is_valid() {
        let settings = Settings::default();
        assert!(settings.wake.activation_phrases.is_empty());
        assert!(settings.wake.deactivation_phrases.is_empty());
        settings
            .validate()
            .expect("empty lists are the starting state");
    }

    #[test]
    fn timeout_out_of_range_names_the_field_and_the_range() {
        let mut settings = Settings::default();
        settings.session.idle_timeout_seconds = 3;
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "session.idle_timeout_seconds");
        assert!(error.message.contains("5..3600"));
        settings.session.idle_timeout_seconds = 0;
        settings.validate().expect("0 disables the timeout");
    }

    #[test]
    fn unknown_model_and_engine_mismatch_are_rejected() {
        let mut settings = Settings::default();
        settings.wake.model = "nope".to_owned();
        assert_eq!(settings.validate().unwrap_err().field, "wake.model");
        let mut settings = Settings::default();
        settings.wake.engine = "sherpa".to_owned();
        assert_eq!(settings.validate().unwrap_err().field, "wake.engine");
    }

    #[test]
    fn shortcut_syntax_is_checked() {
        let mut settings = Settings::default();
        for ok in ["", "Ctrl+Alt+M", "shift+F9", "Super+Space"] {
            settings.hotkey.mute = ok.to_owned();
            settings.validate().unwrap_or_else(|e| panic!("{ok}: {e}"));
        }
        for bad in ["M", "Ctrl+", "Ctrl+Alt", "Ctrl+A+B"] {
            settings.hotkey.mute = bad.to_owned();
            assert_eq!(
                settings.validate().unwrap_err().field,
                "hotkey.mute",
                "{bad}"
            );
        }
    }

    #[test]
    fn a_patch_merges_nested_objects_and_leaves_the_rest() {
        let settings = Settings::default();
        let patched = settings
            .with_patch(&json!({ "session": { "idle_timeout_seconds": 90 }, "codex": { "voice": "maple" } }))
            .expect("patch applies");
        assert_eq!(patched.session.idle_timeout_seconds, 90);
        assert!(patched.session.earcons);
        assert_eq!(patched.codex.voice, "maple");
        assert_eq!(patched.codex.instructions, settings.codex.instructions);
        assert_eq!(
            settings.changed_sections(&patched),
            vec!["session", "codex"]
        );
    }

    #[test]
    fn a_patch_with_a_wrong_type_is_an_error_not_a_panic() {
        let settings = Settings::default();
        assert!(settings
            .with_patch(&json!({ "session": { "idle_timeout_seconds": "soon" } }))
            .is_err());
    }
}
