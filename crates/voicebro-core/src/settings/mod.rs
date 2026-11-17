//! One settings document for the whole app, validated in one place.
//!
//! Every section has a default, unknown fields survive a load/save round trip, and a
//! document from an older schema version loads with its missing fields filled in. Secrets
//! are never part of this document; see [`crate::secrets`].

pub mod store;

use crate::audio::codec::OpusCodec;
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
/// opens the Performance tab pays nothing for them, and the Opus codec experiment. The
/// window polls and samples by `performance`, and the desktop shell runs its memory reader
/// by `memory`; the core reads only `opus_codec`, at each join and each call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The Opus codec (change `opus-codec-experiment`): `libopus` or `opus-rs`. Read when
    /// the bot joins a voice channel and when a call opens; a save changes neither a running
    /// join nor a running call.
    pub opus_codec: String,
}

impl Default for DiagnosticsSettings {
    fn default() -> Self {
        Self {
            performance: false,
            memory: false,
            verbose_log: false,
            opus_codec: OpusCodec::default().name().to_owned(),
        }
    }
}

impl DiagnosticsSettings {
    /// The selected codec; libopus when the field holds no codec (validation keeps that
    /// from being saved).
    pub fn codec(&self) -> OpusCodec {
        OpusCodec::parse(&self.opus_codec).unwrap_or_default()
    }

    /// For a stored document: a codec this build does not have (`ropus` or `mousiki` from
    /// a trial build of the four-codec comparison) becomes libopus, so the file loads and
    /// every later save passes validation. Returns the value it replaced.
    pub fn replace_unknown_codec(&mut self) -> Option<String> {
        if OpusCodec::parse(&self.opus_codec).is_some() {
            return None;
        }
        Some(std::mem::replace(
            &mut self.opus_codec,
            OpusCodec::default().name().to_owned(),
        ))
    }
}

/// The window's themes.
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
    /// The ids of the collapsed cards of the Settings tab (`appearance`, `diagnostics`) and
    /// the Config tab (`discord`, `audio`, `assistant`, `codex`, `hotkey`); empty, every card
    /// expanded, by default. Only the window reads it, and an id it does not know is kept
    /// and ignored, so no validation beyond the type.
    pub collapsed_cards: Vec<String>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
            font_size: 13,
            collapsed_cards: Vec::new(),
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
    /// Only the linked owner's voice reaches the wake word and the assistant (change
    /// `owner-only-access`); off by default. The owner is not stored here: the link lives in
    /// its own encrypted file ([`crate::owner`]). The commands stay open to everyone.
    #[serde(deserialize_with = "owner_only_flag")]
    pub owner_only: bool,
}

/// `discord.owner_only` takes only `true` or `false`; anything else is refused with the
/// field's path, as the lists of fixed values are.
fn owner_only_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Bool(on) => Ok(on),
        other => Err(serde::de::Error::custom(format!(
            "discord.owner_only must be true or false, not {other}"
        ))),
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

/// The highest gain either direction accepts (400 %).
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
    /// No stored phrases: every model starts with its default phrases (change
    /// `voice-recognition-rework`, design D7), which the user can change or remove.
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
    /// The phrases in effect for the selected model: each list as stored, or the model's
    /// defaults for a list never set.
    pub fn selected_phrases(&self) -> PhraseLists {
        self.phrases_of(&self.model)
    }

    /// The phrases in effect for `model`: each list as stored, or the model's defaults
    /// (a local model's those of its folder's catalog language) for a list never set.
    pub fn phrases_of(&self, model: &str) -> PhraseLists {
        let stored = self.phrases.get(model);
        let defaults = catalog::default_phrases(model);
        let pick = |stored: Option<&Vec<String>>, default: &[&str]| -> Vec<String> {
            match stored {
                Some(list) => list.clone(),
                None => default.iter().map(|phrase| (*phrase).to_owned()).collect(),
            }
        };
        let (activation, deactivation) = defaults.unwrap_or((&[], &[]));
        PhraseLists {
            activation: pick(stored.and_then(|s| s.activation.as_ref()), activation),
            deactivation: pick(stored.and_then(|s| s.deactivation.as_ref()), deactivation),
        }
    }
}

/// One model's activation and deactivation phrases, as the user entered them. A list that
/// is absent (`None`) was never set and means the model's default phrases; a list that is
/// present, even empty, is the user's. Serde leaves an absent list out of the file, so a save
/// that names one list writes only that one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelPhrases {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivation: Option<Vec<String>>,
}

/// The phrases in effect for one model, stored or default, as the user typed them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PhraseLists {
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

/// Where the assistant agent's final answers are posted in Discord (change
/// `post-assistant-answers`). Read at each answer, so a save applies to the next one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum PostAnswers {
    /// Nothing is posted.
    #[default]
    Off,
    /// The built-in text chat of the voice channel the bot is in.
    VoiceChannel,
    /// The channel the current voice session's `/join` was typed in.
    CommandChannel,
    /// The channel `assistant.answers_channel` names, for answers from its own server.
    Channel,
}

impl PostAnswers {
    pub const ALL: [PostAnswers; 4] = [
        PostAnswers::Off,
        PostAnswers::VoiceChannel,
        PostAnswers::CommandChannel,
        PostAnswers::Channel,
    ];

    /// The value in the settings file.
    pub const fn name(self) -> &'static str {
        match self {
            PostAnswers::Off => "off",
            PostAnswers::VoiceChannel => "voice_channel",
            PostAnswers::CommandChannel => "command_channel",
            PostAnswers::Channel => "channel",
        }
    }

    fn names() -> String {
        Self::ALL.map(Self::name).join(", ")
    }
}

/// A value that is not one of the four is refused with the field's path and the values,
/// at a save and at a load alike, as the other lists of fixed values are.
impl TryFrom<String> for PostAnswers {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.name() == value)
            .ok_or_else(|| {
                format!(
                    "assistant.post_answers: «{value}» is not a destination; use one of {}",
                    Self::names()
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantSettings {
    pub engine: String,
    pub speaker_labels: SpeakerLabels,
    /// Every chat entry is also appended to `logs\codex-chat-<date>.log`. Off by default:
    /// nothing of the chat is written to disk.
    pub save_chat_history: bool,
    /// Where the agent's final answers are posted in Discord; off by default, since a
    /// posted answer is public in that channel and stays there.
    pub post_answers: PostAnswers,
    /// The channel id for [`PostAnswers::Channel`]: digits, or empty. Kept while another
    /// destination is chosen, so choosing `channel` again needs no new pick.
    pub answers_channel: String,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            engine: "codex".to_owned(),
            speaker_labels: SpeakerLabels::Start,
            save_chat_history: false,
            post_answers: PostAnswers::Off,
            answers_channel: String::new(),
        }
    }
}

impl AssistantSettings {
    /// The chosen channel's id, when one is stored.
    pub fn answers_channel_id(&self) -> Option<u64> {
        let trimmed = self.answers_channel.trim();
        if trimmed.is_empty() {
            None
        } else {
            trimmed.parse().ok()
        }
    }

    /// The parts of the section that only the answer posting reads.
    fn without_answer_posting(&self) -> AssistantSettings {
        AssistantSettings {
            post_answers: PostAnswers::Off,
            answers_channel: String::new(),
            ..self.clone()
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
    /// Empty (the default) means the server's default voice.
    pub voice: String,
    /// Instructions for the assistant. There is no default: the user writes their own.
    pub instructions: String,
    /// One of [`HANDOFF_MODES`].
    pub handoff_mode: String,
    /// Every start reads the models, reasoning efforts and voices from Codex and saves them
    /// (change `config-tab-and-codex-lists`). Off by default: the saved lists are shown, and
    /// Codex starts at app start only while none are saved. Read once, at start.
    pub refresh_lists_at_start: bool,
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
            refresh_lists_at_start: false,
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

/// The words of a text in the matching form every phrase comparison uses (change
/// `voice-recognition-rework`, design D3): lower case, "ё" as "е", and every character that
/// is not a letter or a digit a space. A phrase is its word list; a phrase with no word is
/// no phrase. The wake engine, the call's transcript and the save validation all compare
/// in this form.
pub fn phrase_words(text: &str) -> Vec<String> {
    lower_words(text)
        .into_iter()
        .map(|word| fold_word(&word))
        .collect()
}

/// The words of a text, lower case, split at every character that is not a letter or a
/// digit, without folding "ё": what a vocabulary is asked about besides the folded form.
pub fn lower_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// One lower-case word in the matching form: "ё" as "е".
pub fn fold_word(word: &str) -> String {
    word.replace('ё', "е")
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

        // Empty lists are allowed: the wake engine shows them as "no phrases configured"
        // rather than the save being refused. The same phrase may belong to two models;
        // only one model's lists must not overlap, compared in the matching form and with
        // the model's defaults in effect for a list never set.
        for model in self.wake.phrases.keys() {
            let lists = self.wake.phrases_of(model);
            let deactivation: Vec<(&String, Vec<String>)> = lists
                .deactivation
                .iter()
                .map(|phrase| (phrase, phrase_words(phrase)))
                .filter(|(_, words)| !words.is_empty())
                .collect();
            for activation in &lists.activation {
                let words = phrase_words(activation);
                if words.is_empty() {
                    continue;
                }
                if let Some((shared, _)) = deactivation.iter().find(|(_, other)| *other == words) {
                    return Err(invalid(
                        &format!("wake.phrases.{model}.deactivation"),
                        format!(
                            "«{}» in the activation list and «{}» in the deactivation list of «{model}» are the same phrase",
                            activation.trim(),
                            shared.trim()
                        ),
                    ));
                }
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
        let channel = self.assistant.answers_channel.trim();
        if !channel.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid(
                "assistant.answers_channel",
                format!("«{channel}» is not a channel id; a channel id is digits only"),
            ));
        }
        if self.assistant.post_answers == PostAnswers::Channel && channel.is_empty() {
            return Err(invalid(
                "assistant.answers_channel",
                "pick the channel the answers go to",
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
        if OpusCodec::parse(&self.diagnostics.opus_codec).is_none() {
            return Err(invalid(
                "diagnostics.opus_codec",
                format!(
                    "«{}» is not an Opus codec; use one of {}",
                    self.diagnostics.opus_codec,
                    OpusCodec::names()
                ),
            ));
        }
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

    /// Which top-level sections differ between two documents. The answer posting fields of
    /// `assistant` count as their own part, `answers`, which nothing has to apply: the
    /// answers read them when they arrive. `discord.owner_only` is its own part, `owner`,
    /// which only sets the voice filter.
    pub fn changed_sections(&self, other: &Settings) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if self.discord.guild_id != other.discord.guild_id {
            changed.push("discord");
        }
        if self.discord.owner_only != other.discord.owner_only {
            changed.push("owner");
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
        if self.assistant.without_answer_posting() != other.assistant.without_answer_posting() {
            changed.push("assistant");
        }
        if (self.assistant.post_answers, &self.assistant.answers_channel)
            != (
                other.assistant.post_answers,
                &other.assistant.answers_channel,
            )
        {
            changed.push("answers");
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
            json!({ "performance": false, "memory": false, "verbose_log": false, "opus_codec": "libopus" }),
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
    fn the_opus_codec_takes_two_values_and_defaults_to_libopus() {
        let older: Settings =
            serde_json::from_str(r#"{ "version": 2, "diagnostics": { "memory": true } }"#).unwrap();
        assert_eq!(older.diagnostics.opus_codec, "libopus");
        assert_eq!(older.diagnostics.codec(), OpusCodec::Libopus);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["diagnostics"]["opus_codec"], "libopus",
            "the next save writes it"
        );

        for codec in OpusCodec::ALL {
            let patched = older
                .with_patch(&json!({ "diagnostics": { "opus_codec": codec.name() } }))
                .unwrap();
            patched
                .validate()
                .unwrap_or_else(|e| panic!("{codec}: {e}"));
            assert_eq!(patched.diagnostics.codec(), codec);
            if codec != OpusCodec::Libopus {
                assert_eq!(older.changed_sections(&patched), vec!["diagnostics"]);
            }
        }

        // A save that names a codec this build does not have is refused, the removed ones
        // included; the message lists the two.
        for name in ["opus2", "ropus", "mousiki"] {
            let unknown = older
                .with_patch(&json!({ "diagnostics": { "opus_codec": name } }))
                .unwrap();
            let error = unknown.validate().unwrap_err();
            assert_eq!(error.field, "diagnostics.opus_codec");
            assert!(
                error.message.contains("libopus, opus-rs"),
                "{}",
                error.message
            );
        }
    }

    #[test]
    fn a_stored_codec_this_build_lacks_becomes_libopus() {
        let mut stored: Settings =
            serde_json::from_str(r#"{ "version": 2, "diagnostics": { "opus_codec": "mousiki" } }"#)
                .unwrap();
        assert_eq!(
            stored.diagnostics.replace_unknown_codec().as_deref(),
            Some("mousiki")
        );
        assert_eq!(stored.diagnostics.opus_codec, "libopus");
        stored.validate().expect("valid after the replacement");
        assert_eq!(
            stored.diagnostics.replace_unknown_codec(),
            None,
            "a known codec stays"
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

    /// Change `config-tab-and-codex-lists`: a settings file of 1.0.1 has neither field.
    const SETTINGS_1_0_1: &str = r#"{ "version": 2, "codex": { "model": "gpt-6-luna", "handoff_mode": "items" }, "ui": { "theme": "light", "font_size": 14 } }"#;

    #[test]
    fn the_lists_refresh_switch_defaults_to_off_and_changes_only_codex() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert!(!older.codex.refresh_lists_at_start);
        assert_eq!(
            older.codex.model, "gpt-6-luna",
            "the rest of the section stays"
        );
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["codex"]["refresh_lists_at_start"], false,
            "the next save writes it"
        );

        let on = older
            .with_patch(&json!({ "codex": { "refresh_lists_at_start": true } }))
            .unwrap();
        on.validate().expect("a boolean is valid");
        assert!(on.codex.refresh_lists_at_start);
        assert_eq!(older.changed_sections(&on), vec!["codex"]);

        assert!(older
            .with_patch(&json!({ "codex": { "refresh_lists_at_start": "yes" } }))
            .is_err());
    }

    /// Change `post-assistant-answers`, scenario "Older file": a 1.0.1 file has neither
    /// field; it loads with `off` and no channel, and the next save writes both.
    #[test]
    fn answer_posting_defaults_to_off_and_the_next_save_writes_both_fields() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert_eq!(older.assistant.post_answers, PostAnswers::Off);
        assert_eq!(older.assistant.answers_channel, "");
        older.validate().expect("the defaults are valid");
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(written["assistant"]["post_answers"], "off");
        assert_eq!(written["assistant"]["answers_channel"], "");

        for choice in PostAnswers::ALL {
            let patched = older
                .with_patch(&json!({ "assistant": {
                    "post_answers": choice.name(), "answers_channel": "1091401169890443327" } }))
                .unwrap();
            patched
                .validate()
                .unwrap_or_else(|e| panic!("{}: {e}", choice.name()));
            assert_eq!(patched.assistant.post_answers, choice);
            assert_eq!(
                patched.assistant.answers_channel_id(),
                Some(1091401169890443327)
            );
            let back: Settings =
                serde_json::from_str(&serde_json::to_string(&patched).unwrap()).unwrap();
            assert_eq!(back.assistant, patched.assistant, "a round trip keeps it");
        }
    }

    /// Scenario "Unknown destination": the save is refused with the field and the four
    /// values in the message.
    #[test]
    fn an_unknown_destination_is_refused_naming_the_field_and_the_four_values() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        let error = older
            .with_patch(&json!({ "assistant": { "post_answers": "thread" } }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("assistant.post_answers"), "{error}");
        assert!(error.contains("thread"), "{error}");
        for choice in PostAnswers::ALL {
            assert!(error.contains(choice.name()), "{error}");
        }
        assert!(older
            .with_patch(&json!({ "assistant": { "post_answers": true } }))
            .is_err());
    }

    /// Scenarios "A channel without an id" and "Not an id".
    #[test]
    fn a_chosen_channel_needs_an_id_of_digits() {
        let mut settings = Settings::default();
        settings.assistant.post_answers = PostAnswers::Channel;
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "assistant.answers_channel");
        settings.assistant.answers_channel = "   ".to_owned();
        assert_eq!(
            settings.validate().unwrap_err().field,
            "assistant.answers_channel"
        );

        settings.assistant.answers_channel = "#general".to_owned();
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "assistant.answers_channel");
        assert!(error.message.contains("#general"), "{}", error.message);
        settings.assistant.post_answers = PostAnswers::VoiceChannel;
        assert_eq!(
            settings.validate().unwrap_err().field,
            "assistant.answers_channel",
            "not digits is refused whatever the destination"
        );

        settings.assistant.post_answers = PostAnswers::Channel;
        settings.assistant.answers_channel = " 1091401169890443327 ".to_owned();
        settings.validate().expect("digits are an id");
    }

    /// Scenario "A stored channel kept while off", and a change of these two fields is the
    /// `answers` part only, which nothing has to apply.
    #[test]
    fn a_stored_channel_is_kept_while_off_and_changes_only_the_answers_part() {
        let mut chosen = Settings::default();
        chosen.assistant.post_answers = PostAnswers::Channel;
        chosen.assistant.answers_channel = "1091401169890443327".to_owned();
        let off = chosen
            .with_patch(&json!({ "assistant": { "post_answers": "off" } }))
            .unwrap();
        off.validate().expect("an id may stay while off");
        assert_eq!(off.assistant.answers_channel, "1091401169890443327");
        assert_eq!(chosen.changed_sections(&off), vec!["answers"]);

        let both = Settings::default()
            .with_patch(&json!({ "assistant": { "post_answers": "channel",
                "answers_channel": "42", "save_chat_history": true } }))
            .unwrap();
        assert_eq!(
            Settings::default().changed_sections(&both),
            vec!["assistant", "answers"]
        );
    }

    /// Change `owner-only-access`, scenarios "Older file" and "Not a boolean": a 1.0.1 file
    /// loads with the mode off and the next save writes it; a change of it is the `owner`
    /// part only.
    #[test]
    fn owner_only_defaults_to_off_and_is_its_own_part() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert!(!older.discord.owner_only);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["discord"]["owner_only"], false,
            "the next save writes it"
        );

        let on = older
            .with_patch(&json!({ "discord": { "owner_only": true } }))
            .unwrap();
        on.validate().expect("a boolean is valid");
        assert!(on.discord.owner_only);
        assert_eq!(older.changed_sections(&on), vec!["owner"]);

        let error = older
            .with_patch(&json!({ "discord": { "owner_only": "yes" } }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("owner_only"), "{error}");

        let guild = older
            .with_patch(&json!({ "discord": { "guild_id": "1091401169441656933" } }))
            .unwrap();
        assert_eq!(older.changed_sections(&guild), vec!["discord"]);
    }

    #[test]
    fn collapsed_cards_default_to_none_and_keep_unknown_ids() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert!(older.ui.collapsed_cards.is_empty());
        assert_eq!((older.ui.theme.as_str(), older.ui.font_size), ("light", 14));
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["ui"]["collapsed_cards"],
            json!([]),
            "written by the next save"
        );

        let patched = older
            .with_patch(&json!({ "ui": { "collapsed_cards": ["audio", "later-card"] } }))
            .unwrap();
        patched.validate().expect("any ids are valid");
        assert_eq!(older.changed_sections(&patched), vec!["ui"]);
        let text = serde_json::to_string(&patched).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.ui.collapsed_cards, ["audio", "later-card"]);

        assert!(older
            .with_patch(&json!({ "ui": { "collapsed_cards": 3 } }))
            .is_err());
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
            activation: Some(activation.iter().map(|p| (*p).to_owned()).collect()),
            deactivation: Some(deactivation.iter().map(|p| (*p).to_owned()).collect()),
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    /// Task 4.1: the matching form.
    #[test]
    fn phrase_words_lower_case_fold_yo_and_split_at_non_letters() {
        assert_eq!(phrase_words("Отбой братан"), ["отбой", "братан"]);
        assert_eq!(phrase_words(" отбой   братан "), ["отбой", "братан"]);
        assert_eq!(phrase_words("Братан всё"), ["братан", "все"]);
        assert_eq!(phrase_words("Заканчивай"), ["заканчивай"]);
        assert_eq!(phrase_words("Отбой братан!"), phrase_words("отбой, братан"));
        assert_eq!(phrase_words("подъём"), phrase_words("подъем"));
        assert_eq!(phrase_words("по-моему"), ["по", "моему"]);
        assert!(phrase_words("").is_empty());
        assert!(phrase_words("   ").is_empty());
        assert!(
            phrase_words(" , ! ").is_empty(),
            "no letter or digit is no phrase"
        );
        assert_eq!(lower_words("Братан ВСЁ"), ["братан", "всё"]);
    }

    /// Task 4.3, scenario "One model's list saved": only the named list is written; the
    /// other stays absent and keeps its default.
    #[test]
    fn a_save_naming_one_list_writes_only_that_one() {
        let patched = Settings::default()
            .with_patch(&json!({ "wake": { "phrases": { ENGLISH: { "activation": ["bro"] } } } }))
            .unwrap();
        patched.validate().unwrap();
        assert_eq!(
            patched.wake.phrases[ENGLISH],
            ModelPhrases {
                activation: Some(strings(&["bro"])),
                deactivation: None
            }
        );
        let written = serde_json::to_value(&patched).unwrap();
        assert_eq!(
            written["wake"]["phrases"][ENGLISH],
            json!({ "activation": ["bro"] }),
            "the absent list is not written"
        );
        assert_eq!(patched.wake.selected_phrases().deactivation, ["peace out"]);
    }

    /// Scenario "An empty list stays empty".
    #[test]
    fn an_empty_list_stays_empty() {
        let patched = Settings::default()
            .with_patch(&json!({ "wake": { "phrases": { ENGLISH: { "deactivation": [] } } } }))
            .unwrap();
        let text = serde_json::to_string(&patched).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.wake.phrases[ENGLISH].deactivation, Some(Vec::new()));
        assert!(back.wake.selected_phrases().deactivation.is_empty());
        assert_eq!(back.wake.selected_phrases().activation, ["bro"]);
    }

    /// Scenario "A settings file without phrases", and an entry an earlier build wrote
    /// (both lists present) kept as it is.
    #[test]
    fn defaults_fill_lists_never_set_and_stored_lists_stay_the_users() {
        let without: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert!(without.wake.phrases.is_empty());
        assert_eq!(
            without.wake.selected_phrases(),
            PhraseLists {
                activation: strings(&["bro"]),
                deactivation: strings(&["peace out"]),
            }
        );
        let written = serde_json::to_value(&without).unwrap();
        assert_eq!(
            written["wake"]["phrases"],
            json!({}),
            "the defaults are not written into the file"
        );
        let earlier: Settings = serde_json::from_str(
            r#"{ "version": 2, "wake": { "model": "vosk-model-small-ru-0.22", "phrases": {
                 "vosk-model-small-ru-0.22": { "activation": ["братан"], "deactivation": [] } } } }"#,
        )
        .unwrap();
        assert_eq!(earlier.wake.selected_phrases().activation, ["братан"]);
        assert!(
            earlier.wake.selected_phrases().deactivation.is_empty(),
            "an earlier build's empty list stays the user's"
        );
        let mut settings = Settings::default();
        settings.wake.model = catalog::RUSSIAN_SMALL.to_owned();
        assert_eq!(settings.wake.selected_phrases().activation, ["братан"]);
        assert_eq!(settings.wake.selected_phrases().deactivation, ["отбой"]);
    }

    /// Scenario "A list set by the user": the other list keeps its default.
    #[test]
    fn a_set_list_replaces_only_its_own_default() {
        let mut settings = Settings::default();
        settings.wake.model = catalog::RUSSIAN_SMALL.to_owned();
        settings.wake.phrases.insert(
            catalog::RUSSIAN_SMALL.to_owned(),
            ModelPhrases {
                activation: None,
                deactivation: Some(strings(&["отбой братан", "заканчивай"])),
            },
        );
        let lists = settings.wake.selected_phrases();
        assert_eq!(lists.activation, ["братан"]);
        assert_eq!(lists.deactivation, ["отбой братан", "заканчивай"]);
    }

    /// Scenarios "A local model of a catalog language" and "… of another language".
    #[test]
    fn a_local_model_takes_the_defaults_of_its_folders_language() {
        let mut settings = Settings::default();
        settings.wake.model = "local:vosk-model-small-de-0.21".to_owned();
        let lists = settings.wake.selected_phrases();
        assert_eq!(lists.activation, ["bruder"]);
        assert_eq!(lists.deactivation, ["bis dann"]);
        settings.wake.model = "local:vosk-model-small-en-us-zamia-0.5".to_owned();
        assert_eq!(settings.wake.selected_phrases().activation, ["bro"]);
        settings.wake.model = "local:vosk-model-small-ca-0.4".to_owned();
        assert_eq!(settings.wake.selected_phrases(), PhraseLists::default());
        settings.wake.model = "local:my-model".to_owned();
        assert_eq!(settings.wake.selected_phrases(), PhraseLists::default());
    }

    /// Task 4.4, scenario "The same phrase written two ways in both lists".
    #[test]
    fn the_same_phrase_written_two_ways_in_both_lists_is_refused() {
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            catalog::RUSSIAN_SMALL.to_owned(),
            phrases(&["Братан всё"], &["братан  все"]),
        );
        let error = settings.validate().unwrap_err();
        assert_eq!(
            error.field,
            format!("wake.phrases.{}.deactivation", catalog::RUSSIAN_SMALL)
        );
        for part in [
            "activation",
            "deactivation",
            catalog::RUSSIAN_SMALL,
            "Братан всё",
        ] {
            assert!(error.message.contains(part), "{part}: {}", error.message);
        }
    }

    /// Scenario "A phrase kept as typed".
    #[test]
    fn a_phrase_is_stored_as_typed() {
        let patched = Settings::default()
            .with_patch(&json!({ "wake": { "phrases": { catalog::RUSSIAN_SMALL: {
                "deactivation": ["Отбой братан", "Заканчивай", "Братан всё"] } } } }))
            .unwrap();
        patched.validate().expect("valid");
        let text = serde_json::to_string(&patched).unwrap();
        assert!(text.contains("Отбой братан"), "{text}");
        assert!(text.contains("Братан всё"), "{text}");
    }

    /// A user activation list that holds the model's default deactivation phrase is refused.
    #[test]
    fn a_users_list_that_holds_the_other_default_is_refused() {
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            ENGLISH.to_owned(),
            ModelPhrases {
                activation: Some(strings(&["bro", "Peace out!"])),
                deactivation: None,
            },
        );
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, format!("wake.phrases.{ENGLISH}.deactivation"));
        assert!(error.message.contains("Peace out!"), "{}", error.message);
        assert!(error.message.contains("peace out"), "{}", error.message);
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
        assert_eq!(
            patched.wake.phrases[ENGLISH],
            ModelPhrases {
                activation: Some(strings(&["bro"])),
                deactivation: None
            }
        );
        assert_eq!(
            patched.wake.phrases[catalog::RUSSIAN_SMALL],
            settings.wake.phrases[catalog::RUSSIAN_SMALL]
        );
        assert_eq!(settings.changed_sections(&patched), vec!["wake"]);
    }

    /// Scenario "Phrases of another model": the selected model without an entry has its
    /// own defaults, not the other model's phrases.
    #[test]
    fn the_selected_model_without_an_entry_has_its_defaults() {
        let mut settings = Settings::default();
        settings
            .wake
            .phrases
            .insert(catalog::RUSSIAN_SMALL.to_owned(), phrases(&["братан"], &[]));
        assert_eq!(settings.wake.model, ENGLISH);
        assert_eq!(
            settings.wake.selected_phrases(),
            PhraseLists {
                activation: strings(&["bro"]),
                deactivation: strings(&["peace out"]),
            }
        );
        settings.wake.model = catalog::RUSSIAN_SMALL.to_owned();
        assert_eq!(settings.wake.selected_phrases().activation, ["братан"]);
        assert!(settings.wake.selected_phrases().deactivation.is_empty());
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
    fn no_phrases_are_stored_at_first_and_the_defaults_are_valid() {
        let settings = Settings::default();
        assert!(settings.wake.phrases.is_empty());
        assert_eq!(settings.wake.selected_phrases().activation, ["bro"]);
        settings.validate().expect("the starting state is valid");
        let mut cleared = Settings::default();
        cleared
            .wake
            .phrases
            .insert(ENGLISH.to_owned(), phrases(&[], &[]));
        cleared.validate().expect("emptied lists are valid");
        assert_eq!(cleared.wake.selected_phrases(), PhraseLists::default());
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
