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
    /// Claude Code as the engine (change `claude-and-opencode-engines`).
    pub claude: ClaudeSettings,
    /// OpenCode as the engine (change `claude-and-opencode-engines`).
    pub opencode: OpencodeSettings,
    pub hotkey: HotkeySettings,
    pub ui: UiSettings,
    pub diagnostics: DiagnosticsSettings,
    /// Speech recognition of the Whisper mode (change `whisper-mode`).
    pub whisper: WhisperSettings,
    /// The local voice of the Whisper mode.
    pub tts: TtsSettings,
    /// The Linux headless app only (change `cross-platform-headless`).
    pub linux: LinuxSettings,
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
            claude: ClaudeSettings::default(),
            opencode: OpencodeSettings::default(),
            hotkey: HotkeySettings::default(),
            ui: UiSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            whisper: WhisperSettings::default(),
            tts: TtsSettings::default(),
            linux: LinuxSettings::default(),
            extra: Map::new(),
        }
    }
}

/// How people talk to the assistant (change `whisper-mode`, design D1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum AssistantMode {
    /// Vosk spots the phrases and a Codex realtime voice call hears and speaks.
    #[default]
    Realtime,
    /// Whisper turns speech into text and spots the phrases, Codex answers in text, a local
    /// voice speaks.
    Whisper,
    /// The text answers mode (change `text-answers-mode`): the Whisper mode without the
    /// voice; each answer is posted as text in a Discord chat.
    Text,
}

impl AssistantMode {
    pub const ALL: [AssistantMode; 3] = [
        AssistantMode::Realtime,
        AssistantMode::Whisper,
        AssistantMode::Text,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            AssistantMode::Realtime => "realtime",
            AssistantMode::Whisper => "whisper",
            AssistantMode::Text => "text",
        }
    }

    /// Whisper recognizes the speech and the engine answers in text turns: the Whisper mode
    /// and the text answers mode (change `text-answers-mode`, D1).
    pub const fn hears_with_whisper(self) -> bool {
        matches!(self, AssistantMode::Whisper | AssistantMode::Text)
    }

    /// A local voice reads the answers into the voice channel: the Whisper mode only.
    pub const fn speaks_locally(self) -> bool {
        matches!(self, AssistantMode::Whisper)
    }
}

impl TryFrom<String> for AssistantMode {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.name() == value)
            .ok_or_else(|| {
                format!(
                    "assistant.mode: «{value}» is not an assistant mode; use one of realtime, whisper, text"
                )
            })
    }
}

/// Which engine answers (change `claude-and-opencode-engines`). Only Codex has the realtime
/// voice mode; Claude Code and OpenCode answer in the Whisper mode only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum AssistantEngine {
    #[default]
    Codex,
    /// Claude Code, Anthropic's command-line agent, run headless.
    Claude,
    /// OpenCode behind its local server.
    Opencode,
}

impl AssistantEngine {
    pub const ALL: [AssistantEngine; 3] = [
        AssistantEngine::Codex,
        AssistantEngine::Claude,
        AssistantEngine::Opencode,
    ];

    /// The value in the settings file.
    pub const fn name(self) -> &'static str {
        match self {
            AssistantEngine::Codex => "codex",
            AssistantEngine::Claude => "claude",
            AssistantEngine::Opencode => "opencode",
        }
    }

    /// The short name people see in Discord, the chat and the log.
    pub const fn display_name(self) -> &'static str {
        match self {
            AssistantEngine::Codex => "Codex",
            AssistantEngine::Claude => "Claude",
            AssistantEngine::Opencode => "OpenCode",
        }
    }

    /// The product's name, for the window and the replies that say what to install.
    pub const fn product_name(self) -> &'static str {
        match self {
            AssistantEngine::Codex => "Codex",
            AssistantEngine::Claude => "Claude Code",
            AssistantEngine::Opencode => "OpenCode",
        }
    }

    /// Whether the engine has the realtime voice mode.
    pub const fn has_voice_mode(self) -> bool {
        matches!(self, AssistantEngine::Codex)
    }

    pub fn parse(name: &str) -> Option<AssistantEngine> {
        Self::ALL.into_iter().find(|engine| engine.name() == name)
    }
}

impl TryFrom<String> for AssistantEngine {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value).ok_or_else(|| {
            format!(
                "assistant.engine: «{value}» is not an assistant engine; use one of codex, claude, opencode"
            )
        })
    }
}

/// Where Whisper runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum WhisperDevice {
    #[default]
    Cpu,
    /// An NVIDIA GPU through CUDA; the CPU when none is available (design D11).
    Gpu,
}

impl WhisperDevice {
    pub const fn name(self) -> &'static str {
        match self {
            WhisperDevice::Cpu => "cpu",
            WhisperDevice::Gpu => "gpu",
        }
    }
}

impl TryFrom<String> for WhisperDevice {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "cpu" => Ok(WhisperDevice::Cpu),
            "gpu" => Ok(WhisperDevice::Gpu),
            _ => Err(format!(
                "whisper.device: «{value}» is not a device; use one of cpu, gpu"
            )),
        }
    }
}

/// The highest CPU thread count `whisper.threads` takes; 0 means automatic.
pub const MAX_WHISPER_THREADS: u32 = 64;

/// Speech recognition of the Whisper mode (design D14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WhisperSettings {
    /// A catalog id, or `local:<file>` for a model in `whisper-models\`.
    pub model: String,
    pub device: WhisperDevice,
    /// CPU threads, 0 automatic, at most [`MAX_WHISPER_THREADS`].
    pub threads: u32,
    /// `auto`, or an ISO 639-1 code Whisper knows.
    pub language: String,
    /// One pair of lists for every language: Whisper writes any word. A list absent from
    /// the file means the default phrases of the wake catalog for `language` (English for
    /// `auto`), as for the wake models.
    pub phrases: ModelPhrases,
}

impl Default for WhisperSettings {
    fn default() -> Self {
        Self {
            model: crate::stt::catalog::DEFAULT_MODEL.to_owned(),
            device: WhisperDevice::Cpu,
            threads: 0,
            language: "auto".to_owned(),
            phrases: ModelPhrases::default(),
        }
    }
}

/// The Whisper phrases a list never set falls back to, whatever `whisper.language` is (the
/// user's decision of 2026-11-17, change `whisper-config-layout`).
pub const WHISPER_DEFAULT_ACTIVATION: &[&str] = &["bro"];
pub const WHISPER_DEFAULT_DEACTIVATION: &[&str] = &["peace out"];

impl WhisperSettings {
    /// The phrases in effect: each list as stored, or "bro" and "peace out" for a list
    /// never set.
    pub fn phrases_in_effect(&self) -> PhraseLists {
        let pick = |stored: Option<&Vec<String>>, default: &[&str]| -> Vec<String> {
            match stored {
                Some(list) => list.clone(),
                None => default.iter().map(|phrase| (*phrase).to_owned()).collect(),
            }
        };
        PhraseLists {
            activation: pick(self.phrases.activation.as_ref(), WHISPER_DEFAULT_ACTIVATION),
            deactivation: pick(
                self.phrases.deactivation.as_ref(),
                WHISPER_DEFAULT_DEACTIVATION,
            ),
        }
    }
}

/// The local voice of the Whisper mode (design D14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsSettings {
    /// A voice catalog id, or `windows:<voice id>`.
    pub voice: String,
    /// 0.5 to 2.0.
    pub speed: f32,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            voice: crate::tts::catalog::DEFAULT_VOICE.to_owned(),
            speed: 1.0,
        }
    }
}

/// The speed range of `tts.speed`.
pub const TTS_SPEEDS: std::ops::RangeInclusive<f32> = 0.5..=2.0;

/// What only the Linux headless app reads (change `cross-platform-headless`, design D8).
/// Windows ignores it, and the window does not show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinuxSettings {
    /// Whether the stored Discord token, the owner link and the consent records are
    /// encrypted on Linux; `false` keeps them as plain files only their user can read.
    #[serde(deserialize_with = "encrypt_files_flag")]
    pub encrypt_files: bool,
}

impl Default for LinuxSettings {
    fn default() -> Self {
        Self {
            encrypt_files: true,
        }
    }
}

/// `linux.encrypt_files` takes only `true` or `false`; anything else is refused with the
/// field's path.
fn encrypt_files_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Bool(on) => Ok(on),
        other => Err(serde::de::Error::custom(format!(
            "linux.encrypt_files must be true or false, not {other}"
        ))),
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

/// Whose voice reaches the wake word and the assistant (change `voice-consent`, design D1).
/// The commands stay open to everyone in every mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum AccessMode {
    /// Every member in the bot's voice channel.
    #[default]
    Everyone,
    /// The linked owner and every person with a consent record ([`crate::consent`]).
    Agreed,
    /// Only the linked owner (change `owner-only-access`).
    Owner,
}

impl AccessMode {
    pub const ALL: [AccessMode; 3] = [AccessMode::Everyone, AccessMode::Agreed, AccessMode::Owner];

    pub const fn name(self) -> &'static str {
        match self {
            AccessMode::Everyone => "everyone",
            AccessMode::Agreed => "agreed",
            AccessMode::Owner => "owner",
        }
    }
}

impl TryFrom<String> for AccessMode {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.name() == value)
            .ok_or_else(|| {
                format!(
                    "discord.access: «{value}» is not an access mode; use one of everyone, agreed, owner"
                )
            })
    }
}

/// The longest `discord.privacy_url` the settings take.
pub const MAX_PRIVACY_URL_LEN: usize = 512;

/// The bot always connects at start when a token is stored; an `auto_connect` field in an
/// older file is ignored and gone after the next save.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "DiscordSettingsFile")]
pub struct DiscordSettings {
    /// A server whose slash commands are also registered directly, so they appear there at
    /// once. They are always registered globally as well.
    pub guild_id: String,
    /// Whose voice reaches the wake word and the assistant (change `voice-consent`);
    /// everyone by default. Neither the owner nor the consent records are stored here: they
    /// live in their own encrypted files ([`crate::owner`], [`crate::consent`]).
    pub access: AccessMode,
    /// The operator's privacy policy, shown by `/privacy` and in the `/optin` text: empty or
    /// an `https://` URL, stored trimmed.
    pub privacy_url: String,
}

/// `discord` as a file holds it: `owner_only` of builds 1.0.2 to 1.1.0 is read once to set
/// the access mode when `access` is missing, and never written again (design D1).
#[derive(Deserialize, Default)]
#[serde(default)]
struct DiscordSettingsFile {
    guild_id: String,
    access: Option<AccessMode>,
    #[serde(deserialize_with = "owner_only_flag")]
    owner_only: Option<bool>,
    privacy_url: String,
}

impl From<DiscordSettingsFile> for DiscordSettings {
    fn from(file: DiscordSettingsFile) -> Self {
        let access = file.access.unwrap_or(if file.owner_only == Some(true) {
            AccessMode::Owner
        } else {
            AccessMode::Everyone
        });
        Self {
            guild_id: file.guild_id,
            access,
            privacy_url: file.privacy_url.trim().to_owned(),
        }
    }
}

/// `discord.owner_only` of an older file takes only `true` or `false`; anything else is
/// refused with the field's path, as the lists of fixed values are.
fn owner_only_flag<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Bool(on) => Ok(Some(on)),
        other => Err(serde::de::Error::custom(format!(
            "discord.owner_only must be true or false, not {other}"
        ))),
    }
}

/// Why `url` is not a valid `discord.privacy_url`, if it is not: empty, or `https://` with a
/// host, no spaces, at most [`MAX_PRIVACY_URL_LEN`] characters.
pub fn privacy_url_problem(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    if url.chars().count() > MAX_PRIVACY_URL_LEN {
        return Some(format!(
            "the URL is longer than {MAX_PRIVACY_URL_LEN} characters"
        ));
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return Some("the URL must start with https://".to_owned());
    };
    if url.chars().any(char::is_whitespace) {
        return Some("the URL must not contain spaces".to_owned());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return Some("the URL has no host after https://".to_owned());
    }
    None
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
    /// Codex, Claude Code or OpenCode (change `claude-and-opencode-engines`); its own part,
    /// `engine`, in `changed_sections`.
    pub engine: AssistantEngine,
    /// Realtime voice call or the Whisper mode (change `whisper-mode`); its own part,
    /// `mode`, in `changed_sections`.
    pub mode: AssistantMode,
    pub speaker_labels: SpeakerLabels,
    /// The user's instruction for whichever engine runs; there is no default. Moved here
    /// from `codex.instructions` (change `claude-and-opencode-engines`, design D11; see
    /// [`store::migrate_codex_instructions`]). Its own part, `instructions`.
    pub instructions: String,
    /// Every chat entry is also appended to `logs\chat-<date>.log`. Off by default: nothing
    /// of the chat is written to disk.
    pub save_chat_history: bool,
    /// Where the agent's final answers are posted in Discord; off by default, since a
    /// posted answer is public in that channel and stays there.
    pub post_answers: PostAnswers,
    /// The channel id for [`PostAnswers::Channel`]: digits, or empty. Kept while another
    /// destination is chosen, so choosing `channel` again needs no new pick.
    pub answers_channel: String,
    /// One speaker at a time (change `speaker-turns`, design D0): the voice call gets one
    /// person at a time and the others wait, and in the Whisper mode a question waits for
    /// the running answer instead of stopping it. Off by default: the mix and the barge-in
    /// as before.
    #[serde(deserialize_with = "one_speaker_flag")]
    pub one_speaker_at_a_time: bool,
    /// Stop the spoken answer when someone has talked for 600 ms, before Whisper wrote the
    /// phrase down (change `interrupt-on-speech`). Off by default; it acts only in the
    /// Whisper mode and not while one speaker at a time is on.
    #[serde(deserialize_with = "interrupt_flag")]
    pub interrupt_on_speech: bool,
}

/// `assistant.one_speaker_at_a_time` takes only `true` or `false`; anything else is refused
/// with the field's path.
fn one_speaker_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Bool(on) => Ok(on),
        other => Err(serde::de::Error::custom(format!(
            "assistant.one_speaker_at_a_time must be true or false, not {other}"
        ))),
    }
}

/// `assistant.interrupt_on_speech` takes only `true` or `false`; anything else is refused
/// with the field's path.
fn interrupt_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Bool(on) => Ok(on),
        other => Err(serde::de::Error::custom(format!(
            "assistant.interrupt_on_speech must be true or false, not {other}"
        ))),
    }
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            engine: AssistantEngine::Codex,
            mode: AssistantMode::Realtime,
            speaker_labels: SpeakerLabels::Start,
            instructions: String::new(),
            save_chat_history: false,
            post_answers: PostAnswers::Off,
            answers_channel: String::new(),
            one_speaker_at_a_time: false,
            interrupt_on_speech: false,
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

    /// The section without the parts that are their own parts in `changed_sections`: the
    /// answer posting fields, the mode, the engine and the instruction.
    fn without_answer_posting(&self) -> AssistantSettings {
        AssistantSettings {
            post_answers: PostAnswers::Off,
            answers_channel: String::new(),
            mode: AssistantMode::Realtime,
            engine: AssistantEngine::Codex,
            instructions: String::new(),
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
            handoff_mode: DEFAULT_HANDOFF_MODE.to_owned(),
            refresh_lists_at_start: false,
        }
    }
}

/// The longest model name `claude.model` and `opencode.model` take.
pub const MAX_ENGINE_MODEL_LEN: usize = 200;

/// Claude Code as the engine (change `claude-and-opencode-engines`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeSettings {
    /// Path to `claude.exe`; empty means found automatically.
    pub executable: String,
    /// A model name or alias Claude Code accepts; empty means Claude Code's default for the
    /// sign-in. Applies from the guild's next Claude Code conversation.
    pub model: String,
    /// The window asked once, at the first choice of Claude Code with a subscription,
    /// whether only the owner's voice should pass (design D6).
    pub subscription_notice_shown: bool,
    /// The user pressed Hide in the subscription notice of the Claude Code card: the notice
    /// is not shown again (change `claude-notice-hide`, D2). The dialog does not read it.
    #[serde(deserialize_with = "notice_hidden_flag")]
    pub subscription_notice_hidden: bool,
}

/// `claude.subscription_notice_hidden` takes only `true` or `false`; anything else is
/// refused with the field's path.
fn notice_hidden_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Bool(hidden) => Ok(hidden),
        other => Err(serde::de::Error::custom(format!(
            "claude.subscription_notice_hidden must be true or false, not {other}"
        ))),
    }
}

/// OpenCode as the engine (change `claude-and-opencode-engines`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpencodeSettings {
    /// Path to `opencode.exe`; empty means found automatically.
    pub executable: String,
    /// `provider/model` (the model part may hold more `/`); empty means OpenCode's own
    /// default model.
    pub model: String,
}

impl OpencodeSettings {
    /// The provider of `model`, when one is set: the part before the first `/`.
    pub fn provider(&self) -> Option<&str> {
        let model = self.model.trim();
        model
            .split_once('/')
            .map(|(provider, _)| provider)
            .filter(|provider| !provider.is_empty())
    }
}

/// Why `model` is not a valid engine model name, if it is not.
fn engine_model_problem(model: &str, provider_needed: bool) -> Option<String> {
    if model.is_empty() {
        return None;
    }
    if model.chars().any(char::is_whitespace) {
        return Some(format!("«{model}» must not contain spaces"));
    }
    if model.chars().count() > MAX_ENGINE_MODEL_LEN {
        return Some(format!(
            "the model is longer than {MAX_ENGINE_MODEL_LEN} characters"
        ));
    }
    if provider_needed {
        match model.split_once('/') {
            Some((provider, rest)) if !provider.is_empty() && !rest.is_empty() => {}
            _ => {
                return Some(format!(
                    "«{model}» must be written as provider/model, for example openai/gpt-5"
                ))
            }
        }
    }
    None
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
        if let Some(problem) = privacy_url_problem(&self.discord.privacy_url) {
            return Err(invalid("discord.privacy_url", problem));
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

        // An unknown engine is refused while the document is read (`AssistantEngine`).
        if !self.assistant.engine.has_voice_mode() && self.assistant.mode == AssistantMode::Realtime
        {
            return Err(invalid(
                "assistant.mode",
                format!(
                    "the voice mode works only with Codex; {} answers in the Whisper mode only",
                    self.assistant.engine.product_name()
                ),
            ));
        }
        if let Some(problem) = engine_model_problem(self.claude.model.trim(), false) {
            return Err(invalid("claude.model", problem));
        }
        if let Some(problem) = engine_model_problem(self.opencode.model.trim(), true) {
            return Err(invalid("opencode.model", problem));
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
        self.validate_whisper()?;
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

    /// The `whisper` and `tts` sections (change `whisper-mode`, design D14).
    fn validate_whisper(&self) -> Result<(), ValidationError> {
        use crate::stt::catalog as stt;
        let model = self.whisper.model.as_str();
        if stt::find(model).is_none() {
            if model.starts_with(stt::LOCAL_PREFIX) && stt::local_file(model).is_none() {
                return Err(invalid(
                    "whisper.model",
                    format!(
                        "«{model}»: a local model is a plain file name of the Whisper models folder (no \\, /, : or ..)"
                    ),
                ));
            }
            if stt::local_file(model).is_none() {
                return Err(invalid(
                    "whisper.model",
                    format!(
                        "«{model}» is neither in the Whisper model catalog nor a {}<file> of the Whisper models folder",
                        stt::LOCAL_PREFIX
                    ),
                ));
            }
        }
        if self.whisper.threads > MAX_WHISPER_THREADS {
            return Err(invalid(
                "whisper.threads",
                format!(
                    "{} is outside 0 (automatic) to {MAX_WHISPER_THREADS}",
                    self.whisper.threads
                ),
            ));
        }
        if !stt::is_known_language(&self.whisper.language) {
            return Err(invalid(
                "whisper.language",
                format!(
                    "«{}» is not a language Whisper knows; use auto or an ISO 639-1 code such as en or ru",
                    self.whisper.language
                ),
            ));
        }
        for (field, list) in [
            (
                "whisper.phrases.activation",
                &self.whisper.phrases.activation,
            ),
            (
                "whisper.phrases.deactivation",
                &self.whisper.phrases.deactivation,
            ),
        ] {
            if let Some(phrase) = list
                .iter()
                .flatten()
                .find(|phrase| !phrase.trim().is_empty() && phrase_words(phrase).is_empty())
            {
                return Err(invalid(
                    field,
                    format!("«{}» has no letter or digit", phrase.trim()),
                ));
            }
        }
        let lists = self.whisper.phrases_in_effect();
        for activation in &lists.activation {
            let words = phrase_words(activation);
            if words.is_empty() {
                continue;
            }
            if let Some(shared) = lists
                .deactivation
                .iter()
                .find(|other| phrase_words(other) == words)
            {
                return Err(invalid(
                    "whisper.phrases.deactivation",
                    format!(
                        "«{}» in the activation list and «{}» in the deactivation list are the same phrase",
                        activation.trim(),
                        shared.trim()
                    ),
                ));
            }
        }
        let voice = self.tts.voice.as_str();
        if crate::tts::catalog::find(voice).is_none()
            && crate::tts::catalog::windows_voice(voice).is_none()
        {
            return Err(invalid(
                "tts.voice",
                format!(
                    "«{voice}» is neither in the voice catalog nor a {}<voice id> of Windows",
                    crate::tts::catalog::WINDOWS_PREFIX
                ),
            ));
        }
        if !TTS_SPEEDS.contains(&self.tts.speed) {
            return Err(invalid(
                "tts.speed",
                format!("{} is outside 0.5 to 2.0", self.tts.speed),
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
    /// answers read them when they arrive. `discord.access` and `discord.privacy_url` are
    /// their own part, `owner`, which only sets the voice filter and the replies.
    pub fn changed_sections(&self, other: &Settings) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if self.discord.guild_id != other.discord.guild_id {
            changed.push("discord");
        }
        if (self.discord.access, &self.discord.privacy_url)
            != (other.discord.access, &other.discord.privacy_url)
        {
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
        // Change `claude-and-opencode-engines`: the engine switches at once (D1); the
        // instruction and the two engines' sections apply from the next conversation or
        // question.
        if self.assistant.engine != other.assistant.engine {
            changed.push("engine");
        }
        if self.assistant.instructions != other.assistant.instructions {
            changed.push("instructions");
        }
        if self.claude != other.claude {
            changed.push("claude");
        }
        if self.opencode != other.opencode {
            changed.push("opencode");
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
        // Change `whisper-mode`: the mode switches at once (D1); the two sections reload
        // in the background or apply from the next utterance or sentence (D12).
        if self.assistant.mode != other.assistant.mode {
            changed.push("mode");
        }
        if self.whisper != other.whisper {
            changed.push("whisper");
        }
        if self.tts != other.tts {
            changed.push("tts");
        }
        // Read only at the Linux headless app's start; nothing restarts for it.
        if self.linux != other.linux {
            changed.push("linux");
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

    /// Change `voice-consent`, requirement "Access mode setting": everyone by default, the
    /// three names only, and a change of it is the `owner` part only.
    #[test]
    fn access_defaults_to_everyone_and_is_its_own_part() {
        assert_eq!(Settings::default().discord.access, AccessMode::Everyone);
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert_eq!(
            older.discord.access,
            AccessMode::Everyone,
            "1.0.1: neither field"
        );
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["discord"]["access"], "everyone",
            "the next save writes it"
        );
        assert!(written["discord"].get("owner_only").is_none());

        let agreed = older
            .with_patch(&json!({ "discord": { "access": "agreed" } }))
            .unwrap();
        agreed.validate().expect("a known mode is valid");
        assert_eq!(agreed.discord.access, AccessMode::Agreed);
        assert_eq!(older.changed_sections(&agreed), vec!["owner"]);

        let error = older
            .with_patch(&json!({ "discord": { "access": "friends" } }))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("discord.access") && error.contains("everyone, agreed, owner"),
            "{error}"
        );

        let guild = older
            .with_patch(&json!({ "discord": { "guild_id": "1091401169441656933" } }))
            .unwrap();
        assert_eq!(older.changed_sections(&guild), vec!["discord"]);
    }

    /// Requirement "The owner-only switch of older files": `owner_only: true` of 1.1.0 loads
    /// as `owner` and is never written again; `access` decides when both are there.
    #[test]
    fn owner_only_of_older_files_becomes_the_access_mode() {
        let on: Settings = serde_json::from_str(
            r#"{ "version": 2, "discord": { "guild_id": "", "owner_only": true } }"#,
        )
        .unwrap();
        assert_eq!(on.discord.access, AccessMode::Owner);
        let written = serde_json::to_value(&on).unwrap();
        assert_eq!(written["discord"]["access"], "owner");
        assert!(written["discord"].get("owner_only").is_none(), "{written}");

        let off: Settings =
            serde_json::from_str(r#"{ "discord": { "owner_only": false } }"#).unwrap();
        assert_eq!(off.discord.access, AccessMode::Everyone);

        let both: Settings =
            serde_json::from_str(r#"{ "discord": { "owner_only": true, "access": "agreed" } }"#)
                .unwrap();
        assert_eq!(both.discord.access, AccessMode::Agreed, "access decides");

        let error = serde_json::from_str::<Settings>(r#"{ "discord": { "owner_only": "yes" } }"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("discord.owner_only"), "{error}");
    }

    /// Change `speaker-turns`, requirement "One speaker at a time setting": off by default
    /// and in older files, a boolean only, and a change of it is the `assistant` part.
    #[test]
    fn one_speaker_at_a_time_defaults_to_off() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert!(!older.assistant.one_speaker_at_a_time);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(written["assistant"]["one_speaker_at_a_time"], false);
        let on = older
            .with_patch(&json!({ "assistant": { "one_speaker_at_a_time": true } }))
            .unwrap();
        assert!(on.assistant.one_speaker_at_a_time);
        assert_eq!(older.changed_sections(&on), vec!["assistant"]);
        let error = older
            .with_patch(&json!({ "assistant": { "one_speaker_at_a_time": "yes" } }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("assistant.one_speaker_at_a_time"), "{error}");
    }

    /// Change `interrupt-on-speech`, requirement "Interrupt on speech setting": off by
    /// default and in older files, written by the next save, a boolean only, and a change
    /// of it is the `assistant` part.
    #[test]
    fn interrupt_on_speech_defaults_to_off_and_takes_only_a_boolean() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert!(!older.assistant.interrupt_on_speech);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(written["assistant"]["interrupt_on_speech"], false);
        let on = older
            .with_patch(&json!({ "assistant": { "interrupt_on_speech": true } }))
            .unwrap();
        assert!(on.assistant.interrupt_on_speech);
        assert_eq!(older.changed_sections(&on), vec!["assistant"]);
        let error = older
            .with_patch(&json!({ "assistant": { "interrupt_on_speech": "yes" } }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("assistant.interrupt_on_speech"), "{error}");
    }

    /// Requirement "Privacy policy URL setting": empty or `https://` with a host, trimmed, at
    /// most 512 characters; its own `owner` part.
    #[test]
    fn privacy_url_is_https_or_empty() {
        let base = Settings::default();
        assert!(base.discord.privacy_url.is_empty());
        base.validate().expect("empty is valid");

        let set = base
            .with_patch(
                &json!({ "discord": { "privacy_url": " https://example.org/voicebro-privacy " } }),
            )
            .unwrap();
        set.validate().expect("https with a host is valid");
        assert_eq!(
            set.discord.privacy_url,
            "https://example.org/voicebro-privacy"
        );
        assert_eq!(base.changed_sections(&set), vec!["owner"]);

        for (url, words) in [
            ("http://example.org/privacy", "https://"),
            ("example.org/privacy", "https://"),
            ("https:///privacy", "no host"),
            ("https://example.org/my privacy", "spaces"),
        ] {
            let bad = base
                .with_patch(&json!({ "discord": { "privacy_url": url } }))
                .unwrap();
            let error = bad.validate().unwrap_err();
            assert_eq!(error.field, "discord.privacy_url", "{url}");
            assert!(error.to_string().contains(words), "{url}: {error}");
        }
        let long = format!("https://example.org/{}", "a".repeat(MAX_PRIVACY_URL_LEN));
        let bad = base
            .with_patch(&json!({ "discord": { "privacy_url": long } }))
            .unwrap();
        assert_eq!(bad.validate().unwrap_err().field, "discord.privacy_url");
        let longest = format!(
            "https://example.org/{}",
            "a".repeat(MAX_PRIVACY_URL_LEN - "https://example.org/".len())
        );
        let ok = base
            .with_patch(&json!({ "discord": { "privacy_url": longest } }))
            .unwrap();
        ok.validate().expect("512 characters are valid");
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

    /// Change `whisper-mode`, task 1.2, scenario "Older file": a 1.0.1 file loads in the
    /// realtime mode with the new sections at their defaults, and the next save writes them.
    #[test]
    fn an_older_file_loads_in_the_realtime_mode_with_the_whisper_defaults() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        assert_eq!(older.assistant.mode, AssistantMode::Realtime);
        assert_eq!(older.whisper, WhisperSettings::default());
        assert_eq!(older.whisper.model, "whisper-base-q8_0");
        assert_eq!(older.whisper.device, WhisperDevice::Cpu);
        assert_eq!(older.tts.voice, "supertonic3:F1");
        older.validate().expect("the defaults are valid");
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(written["assistant"]["mode"], "realtime");
        assert_eq!(written["whisper"]["language"], "auto");
        assert_eq!(written["tts"]["speed"], 1.0);
        assert_eq!(
            older.whisper.phrases_in_effect(),
            PhraseLists {
                activation: strings(&["bro"]),
                deactivation: strings(&["peace out"]),
            },
            "the defaults"
        );
        // Change `whisper-config-layout`: the same defaults whatever the language, and a
        // stored list keeps its value.
        for language in ["ru", "ja"] {
            let other = older
                .with_patch(&json!({ "whisper": { "language": language } }))
                .unwrap();
            assert_eq!(
                other.whisper.phrases_in_effect(),
                PhraseLists {
                    activation: strings(&["bro"]),
                    deactivation: strings(&["peace out"]),
                },
                "{language}"
            );
        }
        let stored = older
            .with_patch(&json!({ "whisper": { "language": "ru", "phrases": { "activation": ["братан"] } } }))
            .unwrap();
        assert_eq!(stored.whisper.phrases_in_effect().activation, ["братан"]);
        assert_eq!(
            stored.whisper.phrases_in_effect().deactivation,
            ["peace out"]
        );
    }

    /// Requirement "Assistant mode setting" (change `text-answers-mode`): `text` is the third
    /// mode, taken with every engine, and the two questions on a mode answer as designed (D1).
    #[test]
    fn the_text_answers_mode_is_a_third_mode_for_every_engine() {
        for engine in ["codex", "claude", "opencode"] {
            let settings = Settings::default()
                .with_patch(&json!({ "assistant": { "engine": engine, "mode": "text" } }))
                .unwrap();
            settings.validate().unwrap();
            assert_eq!(settings.assistant.mode, AssistantMode::Text);
            assert_eq!(
                serde_json::to_value(&settings).unwrap()["assistant"]["mode"],
                "text"
            );
        }
        assert!(AssistantMode::Text.hears_with_whisper() && !AssistantMode::Text.speaks_locally());
        assert!(
            AssistantMode::Whisper.hears_with_whisper() && AssistantMode::Whisper.speaks_locally()
        );
        assert!(
            !AssistantMode::Realtime.hears_with_whisper()
                && !AssistantMode::Realtime.speaks_locally()
        );
    }

    /// Task 1.2: an unknown mode, a speed of 3 and a path as a local model are refused, each
    /// naming its field.
    #[test]
    fn whisper_and_voice_values_outside_their_range_are_refused() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        let error = older
            .with_patch(&json!({ "assistant": { "mode": "local" } }))
            .unwrap_err()
            .to_string();
        for part in ["assistant.mode", "local", "realtime", "whisper", "text"] {
            assert!(error.contains(part), "{part}: {error}");
        }
        let check = |patch: Value, field: &str, part: &str| {
            let error = older.with_patch(&patch).unwrap().validate().unwrap_err();
            assert_eq!(error.field, field, "{patch}");
            assert!(error.message.contains(part), "{}", error.message);
        };
        check(
            json!({ "tts": { "speed": 3.0 } }),
            "tts.speed",
            "0.5 to 2.0",
        );
        check(
            json!({ "whisper": { "model": "local:..\\other.bin" } }),
            "whisper.model",
            "plain file name",
        );
        check(
            json!({ "whisper": { "model": "whisper-huge" } }),
            "whisper.model",
            "catalog",
        );
        check(
            json!({ "whisper": { "threads": 65 } }),
            "whisper.threads",
            "64",
        );
        check(
            json!({ "whisper": { "language": "xx" } }),
            "whisper.language",
            "xx",
        );
        check(
            json!({ "tts": { "voice": "espeak:en" } }),
            "tts.voice",
            "espeak:en",
        );
        assert!(older
            .with_patch(&json!({ "whisper": { "device": "tpu" } }))
            .unwrap_err()
            .to_string()
            .contains("whisper.device"));
        for good in [
            json!({ "assistant": { "mode": "whisper" } }),
            json!({ "whisper": { "model": "local:my-fine-tune.bin", "device": "gpu", "threads": 8, "language": "ru" } }),
            json!({ "tts": { "voice": "windows:MSTTS_V110_ruRU_IrinaM", "speed": 0.5 } }),
            json!({ "tts": { "voice": "piper:ru_RU-denis-medium", "speed": 2.0 } }),
        ] {
            older
                .with_patch(&good)
                .unwrap()
                .validate()
                .unwrap_or_else(|e| panic!("{good}: {e}"));
        }
    }

    /// Tasks 1.2 and 1.3: a phrase in both lists, written two ways, is refused; a phrase
    /// with no letter or digit is refused naming it; a phrase change is its own part.
    #[test]
    fn whisper_phrases_compare_in_the_matching_form() {
        let older: Settings = serde_json::from_str(SETTINGS_1_0_1).unwrap();
        let shared = older
            .with_patch(&json!({ "whisper": { "phrases": {
                "activation": ["Goodbye!"], "deactivation": ["goodbye"] } } }))
            .unwrap();
        let error = shared.validate().unwrap_err();
        assert_eq!(error.field, "whisper.phrases.deactivation");
        assert!(error.message.contains("Goodbye!"), "{}", error.message);
        let empty = older
            .with_patch(&json!({ "whisper": { "phrases": { "activation": ["hey bro", "?!"] } } }))
            .unwrap();
        let error = empty.validate().unwrap_err();
        assert_eq!(error.field, "whisper.phrases.activation");
        assert!(error.message.contains("?!"), "{}", error.message);
        assert_eq!(phrase_words("Отбой братан!"), phrase_words("отбой братан"));
        assert_eq!(phrase_words("подъём"), phrase_words("подъем"));
        let changed = older
            .with_patch(&json!({ "whisper": { "phrases": { "deactivation": ["bye bye"] } } }))
            .unwrap();
        assert_eq!(older.changed_sections(&changed), vec!["whisper"]);
        let mode = older
            .with_patch(&json!({ "assistant": { "mode": "whisper" } }))
            .unwrap();
        assert_eq!(older.changed_sections(&mode), vec!["mode"]);
        let voice = older
            .with_patch(&json!({ "tts": { "speed": 1.5 } }))
            .unwrap();
        assert_eq!(older.changed_sections(&voice), vec!["tts"]);
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
        assert_eq!(patched.codex.handoff_mode, settings.codex.handoff_mode);
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

    /// Requirement "Assistant engine setting" (change `claude-and-opencode-engines`).
    #[test]
    fn an_unknown_engine_is_refused_naming_the_field_and_the_three_values() {
        let error = Settings::default()
            .with_patch(&json!({ "assistant": { "engine": "gemini" } }))
            .unwrap_err()
            .to_string();
        for words in ["assistant.engine", "gemini", "codex, claude, opencode"] {
            assert!(error.contains(words), "{words}: {error}");
        }
        assert_eq!(Settings::default().assistant.engine, AssistantEngine::Codex);
    }

    /// Requirements "Assistant engine setting" and "Assistant mode setting": the engine and
    /// the mode change in one save, each its own part; the voice mode needs Codex.
    #[test]
    fn engine_and_mode_change_in_one_save_and_realtime_needs_codex() {
        let settings = Settings::default();
        let both = settings
            .with_patch(&json!({ "assistant": { "engine": "opencode", "mode": "whisper" } }))
            .unwrap();
        both.validate().unwrap();
        assert_eq!(settings.changed_sections(&both), vec!["engine", "mode"]);

        let realtime = settings
            .with_patch(&json!({ "assistant": { "engine": "claude" } }))
            .unwrap();
        let error = realtime.validate().unwrap_err();
        assert_eq!(error.field, "assistant.mode");
        assert!(
            error.message.contains("voice mode works only with Codex"),
            "{}",
            error.message
        );
        assert!(error.message.contains("Claude Code"), "{}", error.message);
    }

    /// Change `realtime-switches-to-codex`, design D1: the window's way back to the voice
    /// call is one save with the engine Codex and the realtime mode, which validates; the
    /// realtime mode alone with another engine is still refused.
    #[test]
    fn the_way_back_to_the_realtime_voice_saves_codex_with_the_mode() {
        for (engine, mode) in [("claude", "whisper"), ("opencode", "text")] {
            let from = Settings::default()
                .with_patch(&json!({ "assistant": { "engine": engine, "mode": mode } }))
                .unwrap();
            from.validate().unwrap();
            let back = from
                .with_patch(&json!({ "assistant": { "engine": "codex", "mode": "realtime" } }))
                .unwrap();
            back.validate().unwrap();
            assert_eq!(
                from.changed_sections(&back),
                vec!["engine", "mode"],
                "{engine}"
            );

            let alone = from
                .with_patch(&json!({ "assistant": { "mode": "realtime" } }))
                .unwrap();
            let error = alone.validate().unwrap_err();
            assert_eq!(error.field, "assistant.mode");
            assert!(
                error.message.contains("voice mode works only with Codex"),
                "{engine}: {}",
                error.message
            );
        }
    }

    /// Requirement "Assistant instruction setting": its own part, which rebuilds no gate.
    #[test]
    fn an_instruction_change_is_its_own_part() {
        let settings = Settings::default();
        let patched = settings
            .with_patch(&json!({ "assistant": { "instructions": "Answer briefly." } }))
            .unwrap();
        assert_eq!(patched.assistant.instructions, "Answer briefly.");
        assert_eq!(settings.changed_sections(&patched), vec!["instructions"]);
    }

    /// Requirements "Claude Code settings" and "OpenCode settings".
    #[test]
    fn the_two_engine_sections_have_defaults_and_check_their_models() {
        let defaults = Settings::default();
        assert_eq!(defaults.claude, ClaudeSettings::default());
        assert!(defaults.claude.executable.is_empty() && defaults.claude.model.is_empty());
        assert!(!defaults.claude.subscription_notice_shown);
        assert_eq!(defaults.opencode, OpencodeSettings::default());

        let mut settings = Settings::default();
        settings.claude.model = "sonnet 5".to_owned();
        assert_eq!(settings.validate().unwrap_err().field, "claude.model");
        settings.claude.model = "claude-sonnet-5-5".to_owned();
        settings.validate().unwrap();

        settings.opencode.model = "gpt-5".to_owned();
        let error = settings.validate().unwrap_err();
        assert_eq!(error.field, "opencode.model");
        assert!(
            error.message.contains("provider/model"),
            "{}",
            error.message
        );
        settings.opencode.model = "openrouter/anthropic/claude-sonnet-4.5".to_owned();
        settings.validate().unwrap();
        assert_eq!(settings.opencode.provider(), Some("openrouter"));
        settings.opencode.model = "/gpt-5".to_owned();
        assert_eq!(settings.validate().unwrap_err().field, "opencode.model");

        let claude = Settings::default()
            .with_patch(&json!({ "claude": { "subscription_notice_shown": true } }))
            .unwrap();
        assert_eq!(
            Settings::default().changed_sections(&claude),
            vec!["claude"]
        );
    }

    /// Requirement "Claude Code settings" (change `claude-notice-hide`): the Hide flag of the
    /// subscription notice defaults to false, an older file loads it as false and the next
    /// save writes it, and only a boolean is taken.
    #[test]
    fn the_notice_hidden_flag_defaults_to_false_and_takes_only_a_boolean() {
        assert!(!Settings::default().claude.subscription_notice_hidden);
        let older: Settings = serde_json::from_value(json!({
            "version": 2,
            "claude": { "executable": "", "model": "", "subscription_notice_shown": true }
        }))
        .unwrap();
        assert!(!older.claude.subscription_notice_hidden);
        assert!(older.claude.subscription_notice_shown);
        let written = serde_json::to_value(&older).unwrap();
        assert_eq!(
            written["claude"]["subscription_notice_hidden"],
            json!(false)
        );

        let hidden = Settings::default()
            .with_patch(&json!({ "claude": { "subscription_notice_hidden": true } }))
            .unwrap();
        assert!(hidden.claude.subscription_notice_hidden);
        assert_eq!(
            Settings::default().changed_sections(&hidden),
            vec!["claude"]
        );

        let error = Settings::default()
            .with_patch(&json!({ "claude": { "subscription_notice_hidden": "yes" } }))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("claude.subscription_notice_hidden"),
            "{error}"
        );
    }
}
