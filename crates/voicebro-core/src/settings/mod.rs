//! One settings document for the whole app, validated in one place.
//!
//! Every section has a default, unknown fields survive a load/save round trip, and a
//! document from an older schema version loads with its missing fields filled in. Secrets
//! are never part of this document; see [`crate::secrets`].

pub mod store;

use crate::wake::catalog;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The schema version this build writes. Version 2 keeps the wake phrases per model
/// (`wake.phrases`); see [`store::migrate_v1`].
pub const CURRENT_VERSION: u32 = 2;

pub const DEFAULT_MUTE_HOTKEY: &str = "Ctrl+Alt+M";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub discord: DiscordSettings,
    pub session: SessionSettings,
    pub audio: AudioSettings,
    pub wake: WakeSettings,
    pub assistant: AssistantSettings,
    pub codex: CodexSettings,
    pub hotkey: HotkeySettings,
    pub ui: UiSettings,
    pub diagnostics: DiagnosticsSettings,
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
            audio: AudioSettings::default(),
            wake: WakeSettings::default(),
            assistant: AssistantSettings::default(),
            codex: CodexSettings::default(),
            hotkey: HotkeySettings::default(),
            ui: UiSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            extra: Map::new(),
        }
    }
}

/// The measurements built for the tuning work, off by default so that a user who never
/// opens the Performance tab pays nothing for them. Nothing in the core reads this section:
/// the window polls and samples by `performance`, and the desktop shell runs its memory
/// reader by `memory`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiagnosticsSettings {
    /// The window samples the status once a second and keeps five minutes of it for the
    /// Performance tab's charts and cards.
    pub performance: bool,
    /// The desktop shell reads the private working set of the app's processes every 3 s
    /// for the status bar and the System tab.
    pub memory: bool,
    /// The file log also gets the detail lines (what the recognizer heard, latency, call
    /// counters, chat copies) and third-party warnings; read by the desktop shell.
    pub verbose_log: bool,
}

/// The window's themes, as in the first version.
pub const THEMES: &[&str] = &["dark", "darcula", "light", "intellij"];
/// The window's font size range in pixels.
pub const FONT_SIZES: std::ops::RangeInclusive<u32> = 11..=20;

/// How the window looks. Kept in the settings file so it travels with the portable app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// One of [`THEMES`].
    pub theme: String,
    /// Pixels, within [`FONT_SIZES`].
    pub font_size: u32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
            font_size: 13,
        }
    }
}

/// The bot always connects at start when a token is stored; an `auto_connect` field in an
/// older file is ignored and gone after the next save.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordSettings {
    /// A server whose slash commands are also registered directly, so they appear there at
    /// once. They are always registered globally as well.
    pub guild_id: String,
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

/// The highest gain either direction accepts (400 %), as in the first version.
pub const MAX_GAIN: f32 = 4.0;

/// How loud the assistant hears the channel and how loud it speaks in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Multiplies every sample the session gate sends to the assistant. 0.0..=4.0.
    pub input_gain: f32,
    /// Multiplies every sample of the assistant's audio played into Discord. 0.0..=4.0.
    pub output_gain: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_gain: 1.0,
            output_gain: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WakeSettings {
    /// Engine id from the catalog (`vosk`).
    pub engine: String,
    /// A catalog model id, or `local:<folder>` for a model in the models folder.
    pub model: String,
    /// The phrases of each model, keyed by model id. Only the selected model's are used.
    pub phrases: BTreeMap<String, ModelPhrases>,
}

impl Default for WakeSettings {
    /// No phrases by default: the user chooses them in the settings. The wake engine
    /// reports an empty list in its status instead of listening for nothing.
    fn default() -> Self {
        let model = catalog::default_entry();
        Self {
            engine: model.engine.to_owned(),
            model: model.id.to_owned(),
            phrases: BTreeMap::new(),
        }
    }
}

impl WakeSettings {
    /// The phrases of the selected model; empty when it has none yet.
    pub fn selected_phrases(&self) -> ModelPhrases {
        self.phrases.get(&self.model).cloned().unwrap_or_default()
    }
}

/// One model's activation and deactivation phrases, as the user entered them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelPhrases {
    pub activation: Vec<String>,
    pub deactivation: Vec<String>,
}

/// The prefix of a model found in the models folder, followed by its folder name.
pub const LOCAL_MODEL_PREFIX: &str = "local:";

/// The folder name of a `local:` model id, when the id is one and the name is a plain
/// folder name (no separators, no `..`, no drive colon).
pub fn local_model_folder(id: &str) -> Option<&str> {
    let folder = id.strip_prefix(LOCAL_MODEL_PREFIX)?;
    let plain = !folder.trim().is_empty()
        && folder.trim() == folder
        && !folder.contains(['/', '\\', ':'])
        && folder != "."
        && !folder.contains("..");
    plain.then_some(folder)
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
    /// Every chat entry is also appended to `logs\codex-chat-<date>.log`. Off by default:
    /// nothing of the chat is written to disk.
    pub save_chat_history: bool,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            engine: "codex".to_owned(),
            speaker_labels: SpeakerLabels::Start,
            save_chat_history: false,
        }
    }
}

/// How the Codex agent's answer reaches the voice model, one of these. `items` adds it to
/// the conversation as its own item and is the default: in the tests of 2026-11-10 it kept
/// the agent's facts in every call. `thinking` is Codex's built-in routing.
pub const HANDOFF_MODES: &[&str] = &["items", "commentary", "bem_tags", "thinking"];
pub const DEFAULT_HANDOFF_MODE: &str = "items";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CodexSettings {
    /// Path to `codex.exe`; empty means auto-detect.
    pub executable: String,
    /// Empty means the account default.
    pub model: String,
    /// Empty means the model default.
    pub reasoning_effort: String,
    /// Empty (the default) means the server's default voice, which is what the first
    /// version effectively used.
    pub voice: String,
    /// Instructions for the assistant. There is no default: the user writes their own.
    pub instructions: String,
    /// One of [`HANDOFF_MODES`].
    pub handoff_mode: String,
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            executable: String::new(),
            model: String::new(),
            reasoning_effort: String::new(),
            voice: String::new(),
            instructions: String::new(),
            handoff_mode: DEFAULT_HANDOFF_MODE.to_owned(),
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
        for (field, gain) in [
            ("audio.input_gain", self.audio.input_gain),
            ("audio.output_gain", self.audio.output_gain),
        ] {
            if !(0.0..=MAX_GAIN).contains(&gain) {
                return Err(invalid(
                    field,
                    format!("{gain} is outside 0 to 4 (0 to 400 %)"),
                ));
            }
        }

        // Empty lists are allowed: they are the starting state, and the wake engine shows
        // them as "no phrases configured" rather than the save being refused. The same
        // phrase may belong to two models; only one model's lists must not overlap.
        for (model, lists) in &self.wake.phrases {
            let activation = normalize_phrases(&lists.activation);
            let deactivation = normalize_phrases(&lists.deactivation);
            if let Some(shared) = activation
                .iter()
                .find(|phrase| deactivation.contains(phrase))
            {
                return Err(invalid(
                    &format!("wake.phrases.{model}.deactivation"),
                    format!(
                        "«{shared}» is in both the activation and the deactivation list of «{model}»"
                    ),
                ));
            }
        }
        let model = self.wake.model.as_str();
        let model_engine = match catalog::find(model) {
            Some(entry) => entry.engine,
            None if local_model_folder(model).is_some() => catalog::VOSK_ENGINE,
            None if model.starts_with(LOCAL_MODEL_PREFIX) => {
                return Err(invalid(
                    "wake.model",
                    format!(
                        "«{model}» is not a plain folder name of the models folder (no \\, /, : or ..)"
                    ),
                ))
            }
            None => {
                return Err(invalid(
                    "wake.model",
                    format!(
                        "«{model}» is neither in the model catalog nor a {LOCAL_MODEL_PREFIX}<folder> model"
                    ),
                ))
            }
        };
        if model_engine != self.wake.engine {
            return Err(invalid(
                "wake.engine",
                format!(
                    "model «{model}» belongs to engine «{model_engine}», not «{}»",
                    self.wake.engine
                ),
            ));
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
        // An empty voice is valid: the call then uses the server's default voice.
        if !HANDOFF_MODES.contains(&self.codex.handoff_mode.as_str()) {
            return Err(invalid(
                "codex.handoff_mode",
                format!(
                    "«{}» is not an answer handoff mode; use one of {}",
                    self.codex.handoff_mode,
                    HANDOFF_MODES.join(", ")
                ),
            ));
        }
        validate_shortcut("hotkey.mute", &self.hotkey.mute)?;
        if !THEMES.contains(&self.ui.theme.as_str()) {
            return Err(invalid(
                "ui.theme",
                format!(
                    "«{}» is not a theme; use one of {}",
                    self.ui.theme,
                    THEMES.join(", ")
                ),
            ));
        }
        if !FONT_SIZES.contains(&self.ui.font_size) {
            return Err(invalid(
                "ui.font_size",
                format!(
                    "{} is outside {} to {}",
                    self.ui.font_size,
                    FONT_SIZES.start(),
                    FONT_SIZES.end()
                ),
            ));
        }
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
        if self.audio != other.audio {
            changed.push("audio");
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
        if self.ui != other.ui {
            changed.push("ui");
        }
        if self.diagnostics != other.diagnostics {
            changed.push("diagnostics");
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
    fn the_handoff_mode_takes_four_values_and_defaults_to_items() {
        let older: Settings =
            serde_json::from_str(r#"{ "version": 1, "codex": { "voice": "juniper" } }"#).unwrap();
        assert_eq!(older.codex.handoff_mode, "items");
        assert_eq!(older.codex.voice, "juniper");
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["codex"]["handoff_mode"], "items",
            "the next save writes it"
        );
        for mode in HANDOFF_MODES {
            let patched = older
                .with_patch(&json!({ "codex": { "handoff_mode": mode } }))
                .unwrap();
            patched.validate().unwrap_or_else(|e| panic!("{mode}: {e}"));
        }
        let mut settings = Settings::default();
        settings.codex.handoff_mode = "verbatim".to_owned();
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "codex.handoff_mode");
        for mode in HANDOFF_MODES {
            assert!(error.message.contains(mode), "{}", error.message);
        }
    }

    #[test]
    fn diagnostics_default_to_off_and_change_only_their_own_section() {
        let older: Settings = serde_json::from_str(r#"{ "version": 1, "ui": {} }"#).unwrap();
        assert_eq!(older.diagnostics, DiagnosticsSettings::default());
        assert!(!older.diagnostics.performance && !older.diagnostics.memory);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["diagnostics"],
            json!({ "performance": false, "memory": false, "verbose_log": false }),
            "the next save writes the section"
        );

        let patched = older
            .with_patch(&json!({ "diagnostics": { "memory": true } }))
            .unwrap();
        patched.validate().expect("a boolean is valid");
        assert!(patched.diagnostics.memory && !patched.diagnostics.performance);
        assert_eq!(older.changed_sections(&patched), vec!["diagnostics"]);

        assert!(
            older
                .with_patch(&json!({ "diagnostics": { "performance": "yes" } }))
                .is_err(),
            "a string is not a switch"
        );
    }

    #[test]
    fn the_log_switches_default_to_off_and_change_only_their_own_section() {
        let older: Settings =
            serde_json::from_str(r#"{ "version": 2, "diagnostics": { "memory": true }, "assistant": { "engine": "codex" } }"#)
                .unwrap();
        assert!(!older.diagnostics.verbose_log && !older.assistant.save_chat_history);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["diagnostics"]["verbose_log"], false,
            "written by the next save"
        );
        assert_eq!(written["assistant"]["save_chat_history"], false);

        let verbose = older
            .with_patch(&json!({ "diagnostics": { "verbose_log": true } }))
            .unwrap();
        verbose.validate().unwrap();
        assert!(verbose.diagnostics.verbose_log);
        assert_eq!(older.changed_sections(&verbose), vec!["diagnostics"]);

        let history = older
            .with_patch(&json!({ "assistant": { "save_chat_history": true } }))
            .unwrap();
        assert!(history.assistant.save_chat_history);
        assert_eq!(older.changed_sections(&history), vec!["assistant"]);

        assert!(older
            .with_patch(&json!({ "assistant": { "save_chat_history": "yes" } }))
            .is_err());
        assert!(older
            .with_patch(&json!({ "diagnostics": { "verbose_log": 1 } }))
            .is_err());
    }

    #[test]
    fn appearance_takes_the_four_themes_and_font_sizes_11_to_20() {
        let older: Settings = serde_json::from_str(r#"{ "version": 1 }"#).unwrap();
        assert_eq!(older.ui, UiSettings::default());
        assert_eq!((older.ui.theme.as_str(), older.ui.font_size), ("dark", 13));
        let patched = older
            .with_patch(&json!({ "ui": { "theme": "intellij", "font_size": 15 } }))
            .unwrap();
        patched.validate().expect("IntelliJ at 15 px is valid");
        assert_eq!(older.changed_sections(&patched), vec!["ui"]);

        let mut settings = Settings::default();
        settings.ui.theme = "pink".to_owned();
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "ui.theme");
        assert!(error.message.contains("darcula"), "{}", error.message);
        let mut settings = Settings::default();
        settings.ui.font_size = 25;
        assert_eq!(settings.validate().unwrap_err().field, "ui.font_size");
        settings.ui.font_size = 11;
        settings.validate().expect("11 is the smallest size");
    }

    #[test]
    fn an_old_auto_connect_field_loads_and_is_gone_after_a_save() {
        let old: Settings = serde_json::from_str(
            r#"{ "version": 1, "discord": { "auto_connect": false, "guild_id": "1091401169441656933" } }"#,
        )
        .expect("an older file loads");
        assert_eq!(old.discord.guild_id, "1091401169441656933");
        old.validate().expect("and is valid");
        let saved = serde_json::to_value(&old).unwrap();
        assert!(saved["discord"].get("auto_connect").is_none(), "{saved}");
        assert!(saved.get("auto_connect").is_none());
    }

    #[test]
    fn unknown_fields_survive_a_round_trip_and_missing_ones_take_defaults() {
        let text = r#"{ "version": 1, "session": { "idle_timeout_seconds": 120 }, "future_field": { "a": 1 } }"#;
        let settings: Settings = serde_json::from_str(text).expect("parses");
        assert_eq!(settings.session.idle_timeout_seconds, 120);
        assert!(settings.session.earcons, "missing field takes the default");
        assert_eq!(
            settings.codex.voice, "",
            "new files use the server's default voice"
        );
        let kept: Settings =
            serde_json::from_str(r#"{ "codex": { "voice": "juniper" } }"#).expect("parses");
        assert_eq!(kept.codex.voice, "juniper", "an existing voice is kept");
        assert!(
            Settings::default().validate().is_ok(),
            "an empty voice is valid"
        );
        let back = serde_json::to_value(&settings).expect("serializes");
        assert_eq!(back["future_field"]["a"], 1);
        assert_eq!(back["hotkey"]["mute"], DEFAULT_MUTE_HOTKEY);
    }

    fn phrases(activation: &[&str], deactivation: &[&str]) -> ModelPhrases {
        ModelPhrases {
            activation: activation.iter().map(|p| (*p).to_owned()).collect(),
            deactivation: deactivation.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    const ENGLISH: &str = "vosk-model-small-en-us-0.15";

    #[test]
    fn overlapping_phrases_of_one_model_are_rejected_with_the_model_and_both_lists_named() {
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            catalog::RUSSIAN_SMALL.to_owned(),
            phrases(&["alpha", "beta"], &[" BETA "]),
        );
        let error = settings.validate().unwrap_err();
        assert_eq!(
            error.field,
            format!("wake.phrases.{}.deactivation", catalog::RUSSIAN_SMALL)
        );
        assert!(error.message.contains("beta"), "{}", error.message);
        assert!(error.message.contains("activation"), "{}", error.message);
        assert!(
            error.message.contains(catalog::RUSSIAN_SMALL),
            "{}",
            error.message
        );
    }

    #[test]
    fn the_same_phrase_for_two_models_is_valid() {
        let mut settings = Settings::default();
        settings
            .wake
            .phrases
            .insert(ENGLISH.to_owned(), phrases(&["bro"], &["goodbye"]));
        settings
            .wake
            .phrases
            .insert(catalog::RUSSIAN_SMALL.to_owned(), phrases(&["bro"], &[]));
        settings.validate().expect("one phrase in two models");
    }

    #[test]
    fn a_patch_to_one_model_leaves_the_other_untouched() {
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            catalog::RUSSIAN_SMALL.to_owned(),
            phrases(&["братан"], &["спасибо братан"]),
        );
        let patched = settings
            .with_patch(&json!({ "wake": { "phrases": { ENGLISH: { "activation": ["bro"] } } } }))
            .unwrap();
        patched.validate().unwrap();
        assert_eq!(patched.wake.phrases[ENGLISH], phrases(&["bro"], &[]));
        assert_eq!(
            patched.wake.phrases[catalog::RUSSIAN_SMALL],
            settings.wake.phrases[catalog::RUSSIAN_SMALL]
        );
        assert_eq!(settings.changed_sections(&patched), vec!["wake"]);
    }

    #[test]
    fn the_selected_model_without_an_entry_has_no_phrases() {
        let mut settings = Settings::default();
        settings
            .wake
            .phrases
            .insert(catalog::RUSSIAN_SMALL.to_owned(), phrases(&["братан"], &[]));
        assert_eq!(settings.wake.model, ENGLISH);
        assert_eq!(settings.wake.selected_phrases(), ModelPhrases::default());
        settings.wake.model = catalog::RUSSIAN_SMALL.to_owned();
        assert_eq!(settings.wake.selected_phrases().activation, ["братан"]);
    }

    #[test]
    fn local_model_ids_are_plain_folder_names() {
        let mut settings = Settings::default();
        settings.wake.model = "local:vosk-model-small-de-0.15".to_owned();
        settings.validate().expect("a plain folder name is valid");
        assert_eq!(
            local_model_folder(&settings.wake.model),
            Some("vosk-model-small-de-0.15")
        );
        for bad in [
            "local:",
            "local: ",
            "local:a/b",
            "local:a\\b",
            "local:..",
            "local:x..y",
            "local:C:x",
            "local:.",
        ] {
            settings.wake.model = bad.to_owned();
            let error = settings.validate().unwrap_err();
            assert_eq!(error.field, "wake.model", "{bad}");
        }
        settings.wake.model = "local:vosk-model-small-de-0.15".to_owned();
        settings.wake.engine = "sherpa".to_owned();
        assert_eq!(settings.validate().unwrap_err().field, "wake.engine");
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
        assert!(settings.wake.phrases.is_empty());
        assert_eq!(settings.wake.selected_phrases(), ModelPhrases::default());
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
    fn gains_default_to_one_accept_the_range_and_name_the_field_when_outside() {
        let older: Settings = serde_json::from_str(r#"{ "version": 1, "session": {} }"#).unwrap();
        assert_eq!(older.audio, AudioSettings::default());
        assert_eq!(
            (older.audio.input_gain, older.audio.output_gain),
            (1.0, 1.0)
        );
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["audio"]["input_gain"], 1.0,
            "the next save writes it"
        );

        let patched = older
            .with_patch(&json!({ "audio": { "input_gain": 3.39, "output_gain": 2.52 } }))
            .unwrap();
        patched.validate().expect("339 % and 252 % are valid");
        assert_eq!(older.changed_sections(&patched), vec!["audio"]);
        for edge in [0.0, MAX_GAIN] {
            let mut settings = Settings::default();
            settings.audio.output_gain = edge;
            settings
                .validate()
                .unwrap_or_else(|e| panic!("{edge}: {e}"));
        }

        let mut settings = Settings::default();
        settings.audio.input_gain = 5.0;
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "audio.input_gain");
        assert!(error.message.contains("0 to 4"), "{}", error.message);
        settings.audio.input_gain = 1.0;
        settings.audio.output_gain = -0.5;
        assert_eq!(settings.validate().unwrap_err().field, "audio.output_gain");
    }

    #[test]
    fn a_patch_with_a_wrong_type_is_an_error_not_a_panic() {
        let settings = Settings::default();
        assert!(settings
            .with_patch(&json!({ "session": { "idle_timeout_seconds": "soon" } }))
            .is_err());
    }
}
