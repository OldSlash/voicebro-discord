//! The composition root. It owns the settings, the secret store, the event bus, the Discord
//! adapter, the gates, the wake engine and the assistant, and it is the only place that
//! knows every subsystem.

use crate::access::{AccessState, VoiceFilter};
use crate::assistant::{AgentAnswer, AnswerSink, Assistant, StubAssistant};
use crate::audio::codec::{CodecFailure, CodecFailureHub, CodecFailures, OpusCodec};
use crate::audio::{frame_channel, AudioPorts, Frame, OutputHandle, INPUT_CAPACITY};
use crate::consent::{AgreeOutcome, ConsentRecord, ConsentState, WithdrawOutcome};
use crate::discord::{
    AnswerDestination, AssistantState, DiscordError, DiscordRuntime, JoinPreparation, PostOutcome,
    TextChannelGroup, VoiceHost,
};
use crate::events::{AppEvent, EventBus};
use crate::gate::{ErrorReporter, GateSettings, RelayGate, RelayRegistry};
use crate::owner::{CompleteOutcome, OwnerState, PendingCode};
use crate::secrets::{SecretError, SecretKey, SecretStore};
use crate::settings::store::{SettingsStore, StoreError};
use crate::settings::{
    AccessMode, AssistantSettings, PostAnswers, SessionMode, Settings, ValidationError,
};
use crate::status::{ConnectionErrors, HotkeyStatus, SecretStatus, Status};
use crate::stt::manager::WhisperManager;
use crate::tts::manager::TtsManager;
use crate::wake::{WakeFeed, WakeManager};
use serde_json::Value;
use serenity::all::GuildId;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Validation(#[from] ValidationError),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Secret(#[from] SecretError),
    #[error("the patch is not a settings document: {0}")]
    Patch(#[from] serde_json::Error),
    #[error("unknown secret «{0}»")]
    UnknownSecret(String),
    #[error("{0}")]
    Discord(#[from] DiscordError),
}

impl ErrorReporter for ConnectionErrors {
    fn report(&self, source: &str, message: Option<String>) {
        ConnectionErrors::report(self, source, message);
    }
}

/// What Discord needs from the app: a gate per voice channel, built on join from the
/// current settings and attached to the assistant.
struct GateHost {
    registry: RelayRegistry,
    wake: WakeManager,
    /// Whisper and the local voice of the Whisper mode (change `whisper-mode`).
    whisper: WhisperManager,
    tts: TtsManager,
    assistant: Arc<dyn Assistant>,
    events: EventBus,
    errors: Arc<ConnectionErrors>,
    gate_settings: Mutex<GateSettings>,
    discord: Mutex<Option<Arc<DiscordRuntime>>>,
    /// `diagnostics.opus_codec`, read by the Discord side at each join.
    opus_codec: Mutex<OpusCodec>,
    /// Where the joins' codec panics are reported (design D5).
    codec_failures: Arc<CodecFailureHub>,
    /// Where every session sends its agent's answers for posting.
    answers: AnswerSink,
    /// The owner link (change `owner-only-access`).
    owner: Arc<OwnerState>,
    /// The consent records (change `voice-consent`).
    consent: Arc<ConsentState>,
    /// The access mode and the voice filter of the three modes.
    access: Arc<AccessState>,
    /// The current settings, shared with the runtime (`discord.privacy_url`).
    settings: Arc<RwLock<Settings>>,
    /// The people in each joined guild's channel (account id, name), so that a change of
    /// the access filter can resend the names it lets through (design D6).
    people: Mutex<std::collections::HashMap<u64, Vec<(u64, String)>>>,
}

impl GateHost {
    /// Gives every joined guild's gate the names the current filter lets through.
    fn reapply_participants(&self) {
        let people = self.people.lock().expect("people lock").clone();
        for (guild, people) in people {
            if let Some(gate) = self.registry.get(guild) {
                gate.set_participants(&self.access.passing_names(&people));
            }
        }
    }
}

/// The fingerprint of the assistant account a status reports (change `owner-only-access`,
/// design D4); `None` while it is not signed in.
fn signed_in_account(status: &crate::assistant::AssistantStatus) -> Option<(String, String)> {
    // OpenCode has no account of its own: a link made with it is bound to none (change
    // `claude-and-opencode-engines`, D9).
    if status.engine == crate::settings::AssistantEngine::Opencode.name() {
        return Some((status.engine.clone(), String::new()));
    }
    status.signed_in.then(|| {
        (
            status.engine.clone(),
            crate::owner::account_fingerprint(
                status.account_type.as_deref(),
                status.email.as_deref(),
            ),
        )
    })
}

/// The gates' feed: the receive path's frames reach Vosk in realtime mode and Whisper in
/// the Whisper mode; each takes them only while it runs.
struct ModeFeed {
    wake: WakeManager,
    whisper: WhisperManager,
}

impl WakeFeed for ModeFeed {
    fn feed(&self, guild: u64, ssrc: u32, tick: u64, frame: &[i16]) {
        self.wake.feed(guild, ssrc, tick, frame);
        self.whisper.feed(guild, ssrc, tick, frame);
    }

    fn tick(&self, guild: u64, tick: u64) {
        self.wake.tick(guild, tick);
        self.whisper.tick(guild, tick);
    }
}

impl VoiceHost for GateHost {
    fn voice_filter(&self) -> Arc<VoiceFilter> {
        self.access.filter()
    }

    fn access_mode(&self) -> AccessMode {
        self.access.mode()
    }

    fn privacy_url(&self) -> String {
        self.settings
            .read()
            .expect("settings lock")
            .discord
            .privacy_url
            .clone()
    }

    fn consent_provider(&self) -> String {
        let settings = self.settings.read().expect("settings lock");
        crate::consent::provider_name(&crate::consent::recipient_key(&settings))
    }

    fn engine_name(&self) -> &'static str {
        self.settings
            .read()
            .expect("settings lock")
            .assistant
            .engine
            .display_name()
    }

    fn consent_record(&self, user: u64) -> Option<ConsentRecord> {
        self.consent.record(user)
    }

    fn agree(&self, user: u64, name: &str, version: u32) -> AgreeOutcome {
        self.consent
            .agree(user, name, version, &chrono::Utc::now().to_rfc3339())
    }

    fn withdraw(&self, user: u64) -> WithdrawOutcome {
        self.consent.withdraw(user, false)
    }

    fn passing_names(&self, people: &[(u64, String)]) -> Vec<String> {
        self.access.passing_names(people)
    }

    fn complete_link(&self, user: u64, name: &str, code: &str) -> CompleteOutcome {
        self.owner.complete(user, name, code)
    }

    fn opus_codec(&self) -> OpusCodec {
        *self.opus_codec.lock().expect("opus codec lock")
    }

    fn codec_failures(&self, guild: u64) -> CodecFailures {
        self.codec_failures.reset(guild);
        self.codec_failures.for_guild(guild)
    }

    fn prepare_join(&self, guild: u64) -> JoinPreparation<'_> {
        Box::pin(async move {
            // The wake word's files first (change `models-in-settings`, D5): a refusal for
            // them starts nothing of the assistant.
            if let Some(reason) = self.wake_files_missing() {
                self.events
                    .log(format!("join to guild {guild} refused: {reason}"));
                return Err(reason);
            }
            let prepared = match self.assistant.prepare(guild).await {
                Ok(()) => self
                    .prepare_whisper()
                    .await
                    .map_err(crate::assistant::AssistantError::Unavailable),
                Err(error) => Err(error),
            };
            match prepared {
                Ok(()) => {
                    self.errors.report("assistant", None);
                    // The account was just read: a link made with another one ends here,
                    // before the first frame of this join is filtered. The join goes on
                    // (change `owner-only-access`, D4).
                    if let Some((engine, account)) = signed_in_account(&self.assistant.status()) {
                        self.owner.check_account_of(&engine, &account);
                    }
                    Ok(())
                }
                Err(error) => {
                    let reason = error.to_string();
                    self.events
                        .log(format!("join to guild {guild} refused: {reason}"));
                    Err(reason)
                }
            }
        })
    }

    fn on_tick(&self, guild: u64, mixed: &Frame, speakers: &[(u32, &[i16])]) {
        if let Some(gate) = self.registry.get(guild) {
            gate.process_tick(mixed, speakers);
        }
    }

    fn set_speaker_name(&self, guild: u64, ssrc: u32, name: &str) {
        if let Some(gate) = self.registry.get(guild) {
            gate.set_speaker_name(ssrc, name);
        }
    }

    fn set_participants(&self, guild: u64, people: &[(u64, String)]) {
        self.people
            .lock()
            .expect("people lock")
            .insert(guild, people.to_vec());
        if let Some(gate) = self.registry.get(guild) {
            gate.set_participants(&self.access.passing_names(people));
        }
    }

    fn on_join(&self, guild: u64, output: OutputHandle) {
        let (sender, receiver) = frame_channel(INPUT_CAPACITY);
        output.use_gain(Arc::clone(&self.registry.gains().output));
        let gate = RelayGate::new(
            guild,
            sender,
            Arc::clone(&self.registry.gains().input),
            output.clone(),
            self.gate_settings
                .lock()
                .expect("gate settings lock")
                .clone(),
            self.registry.is_muted(),
            self.events.clone(),
            Some(Arc::new(ModeFeed {
                wake: self.wake.clone(),
                whisper: self.whisper.clone(),
            })),
            Some(Arc::clone(&self.errors) as Arc<dyn ErrorReporter>),
        );
        output.use_latency(gate.latency(), guild);
        self.registry.register(Arc::clone(&gate));
        self.attach(guild, &gate, receiver, output);
    }

    fn on_leave(&self, guild: u64) {
        self.people.lock().expect("people lock").remove(&guild);
        self.assistant.detach(guild);
        self.whisper.clear_guild(guild);
        if let Some(gate) = self.registry.unregister(guild) {
            gate.output().flush();
        }
        // Whisper and the voice unload 0.5 s after the last leave, as Codex stops.
        if self.registry.all().is_empty() {
            self.whisper.release();
            self.tts.release();
        }
    }

    fn assistant_state(&self, guild: u64) -> AssistantState {
        let Some(gate) = self.registry.get(guild) else {
            return AssistantState::Off;
        };
        let status = gate.status();
        if status.muted {
            AssistantState::Muted
        } else if status.mode == SessionMode::WakeWord && !status.active {
            AssistantState::WaitingForWakeWord
        } else {
            AssistantState::Listening
        }
    }

    fn assistant_mode(&self) -> &'static str {
        if !self
            .gate_settings
            .lock()
            .expect("gate settings lock")
            .whisper
        {
            return crate::discord::MODE_REALTIME;
        }
        // The gates hear with Whisper in both of its modes; the settings tell them apart
        // (change `text-answers-mode`).
        if self.settings.read().expect("settings lock").assistant.mode
            == crate::settings::AssistantMode::Text
        {
            crate::discord::MODE_TEXT
        } else {
            crate::discord::MODE_WHISPER
        }
    }

    fn toggle_mute(&self) -> bool {
        let muted = self.registry.toggle_muted();
        self.after_mute_change(muted);
        muted
    }

    fn is_muted(&self) -> bool {
        self.registry.is_muted()
    }
}

impl GateHost {
    /// Attaches the assistant to a gate, in the mode the settings name now.
    fn attach(
        &self,
        guild: u64,
        gate: &Arc<RelayGate>,
        receiver: crate::audio::FrameReceiver,
        output: OutputHandle,
    ) {
        match self.assistant.attach(
            guild,
            AudioPorts {
                input: receiver,
                output,
                failures: self.codec_failures.for_guild(guild),
                answers: self.answers.clone(),
                heard: Arc::new(self.registry.clone()),
            },
        ) {
            Ok(session) => gate.set_session(session),
            Err(error) => {
                self.events.log(format!(
                    "assistant could not attach to guild {guild}: {error}"
                ));
                self.errors.report("assistant", Some(error.to_string()));
            }
        }
    }

    /// The refusal of a join in the realtime mode with the wake word while the selected
    /// model or the Vosk library is not on disk (change `models-in-settings`, D5). Reads the
    /// disk only; `None` in always listening, in the Whisper mode, and with the files there.
    fn wake_files_missing(&self) -> Option<String> {
        let (mode, session, model) = {
            let settings = self.settings.read().expect("settings lock");
            (
                settings.assistant.mode,
                settings.session.mode,
                settings.wake.model.clone(),
            )
        };
        if mode != crate::settings::AssistantMode::Realtime || session != SessionMode::WakeWord {
            return None;
        }
        match self.wake.requirements(&model) {
            Ok(missing) if !missing.is_empty() && self.events.terminal_guidance() => {
                Some(crate::wake::terminal_join_refusal(&missing))
            }
            Ok(missing) if !missing.is_empty() => Some(crate::wake::join_refusal(&missing)),
            Ok(_) => None,
            // An unknown model is the wake state's own error, shown on the Status tab; it
            // needs no download, so it does not refuse the join.
            Err(_) => None,
        }
    }

    /// In the Whisper mode a join also loads Whisper and the voice (design D12), off the
    /// async threads; missing files refuse it with the reply that sends the user to the
    /// Settings tab's Whisper and Local voice cards. The text answers mode loads Whisper
    /// only (the voice's `prepare_join` passes), and its reply names the Whisper card alone.
    async fn prepare_whisper(&self) -> Result<(), String> {
        let (whisper, tts) = (self.whisper.clone(), self.tts.clone());
        let text_answers = self.settings.read().expect("settings lock").assistant.mode
            == crate::settings::AssistantMode::Text;
        let result = tokio::task::spawn_blocking(move || {
            let heard = whisper.prepare_join();
            let spoken = tts.prepare_join();
            heard.and(spoken)
        })
        .await
        .map_err(|error| format!("the Whisper mode did not load: {error}"))
        .and_then(|result| result)
        .map_err(|error| {
            let missing_files = error == crate::stt::manager::JOIN_NEEDS_DOWNLOADS;
            if missing_files && self.events.terminal_guidance() {
                self.terminal_whisper_refusal(text_answers)
            } else if text_answers && missing_files {
                crate::stt::manager::JOIN_NEEDS_WHISPER_FILES.to_owned()
            } else {
                error
            }
        });
        // A refused join holds nothing: a download agreed later loads only for a join.
        if result.is_err() && self.registry.all().is_empty() {
            self.whisper.release();
            self.tts.release();
        }
        result
    }

    /// The headless app's reply to a Whisper join refused for missing files (change
    /// `cross-platform-headless`): the files, and the terminal command that installs them.
    fn terminal_whisper_refusal(&self, text_answers: bool) -> String {
        let settings = self.settings.read().expect("settings lock").clone();
        let mut missing = self
            .whisper
            .requirements(&settings.whisper.model, settings.whisper.device)
            .unwrap_or_default();
        if !text_answers {
            missing.extend(
                self.tts
                    .requirements(&settings.tts.voice)
                    .unwrap_or_default(),
            );
        }
        let labels = missing
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "The Whisper mode needs downloads ({labels}): stop the bot and run voicebro-server {}, then run /join again",
            crate::wake::INSTALL_ASSETS_ARG
        )
    }

    /// The mode changed while the bot is in voice channels (design D1): every session ends
    /// with the falling tone, the assistant is attached again in the new mode to a new gate
    /// on the same output, and the bot stays in the channel.
    fn switch_mode(&self, settings: &Settings) {
        for (guild, gate, receiver, output) in self.rebuild_gates(settings) {
            self.attach(guild, &gate, receiver, output);
            self.events.log(format!(
                "guild {guild}: the assistant switched to the {} mode",
                settings.assistant.mode.name()
            ));
        }
        if let Some(discord) = self.discord.lock().expect("discord lock").as_ref() {
            discord.mark_panels_dirty();
        }
    }

    /// The engine changed while the bot is in voice channels (change
    /// `claude-and-opencode-engines`, D1): the gates are rebuilt as for a mode change, and
    /// each guild is attached to the new engine once it is prepared for it. A guild whose
    /// preparation fails stays in its channel without a session, with the engine's error.
    async fn switch_engine(
        self: Arc<Self>,
        settings: Settings,
        rebuilt: Vec<(
            u64,
            Arc<RelayGate>,
            crate::audio::FrameReceiver,
            OutputHandle,
        )>,
    ) {
        let product = settings.assistant.engine.product_name();
        for (guild, gate, receiver, output) in rebuilt {
            match self.assistant.prepare(guild).await {
                Ok(()) => {
                    self.attach(guild, &gate, receiver, output);
                    self.errors.report("assistant", None);
                    self.events.log(format!(
                        "guild {guild}: the assistant switched to {product}"
                    ));
                }
                Err(error) => {
                    let message = format!("{product} could not start for this channel: {error}");
                    self.events.log(format!("guild {guild}: {message}"));
                    self.errors.report("assistant", Some(message));
                }
            }
        }
        if let Some(discord) = self.discord.lock().expect("discord lock").as_ref() {
            discord.mark_panels_dirty();
        }
    }

    /// Every guild's gate rebuilt in `settings`: the running session ends with the falling
    /// tone, the old session is detached, and a new gate takes the same output. Returns what
    /// each new gate needs for its session.
    #[allow(clippy::type_complexity)]
    fn rebuild_gates(
        &self,
        settings: &Settings,
    ) -> Vec<(
        u64,
        Arc<RelayGate>,
        crate::audio::FrameReceiver,
        OutputHandle,
    )> {
        let gate_settings = GateSettings::from_settings(settings);
        *self.gate_settings.lock().expect("gate settings lock") = gate_settings.clone();
        let mut rebuilt = Vec::new();
        for old in self.registry.all() {
            let guild = old.guild();
            self.assistant.detach(guild);
            old.output().flush();
            if old.status().active && settings.session.earcons {
                old.output().play_earcon(
                    crate::audio::Earcon::Deactivate,
                    settings.session.earcon_volume,
                );
            }
            let output = old.output().clone();
            let (sender, receiver) = frame_channel(INPUT_CAPACITY);
            let gate = RelayGate::new(
                guild,
                sender,
                Arc::clone(&self.registry.gains().input),
                output.clone(),
                gate_settings.clone(),
                self.registry.is_muted(),
                self.events.clone(),
                Some(Arc::new(ModeFeed {
                    wake: self.wake.clone(),
                    whisper: self.whisper.clone(),
                })),
                Some(Arc::clone(&self.errors) as Arc<dyn ErrorReporter>),
            );
            for (ssrc, name) in old.speaker_names() {
                gate.set_speaker_name(ssrc, &name);
            }
            gate.set_participants(&old.participants());
            output.use_latency(gate.latency(), guild);
            self.registry.register(Arc::clone(&gate));
            rebuilt.push((guild, gate, receiver, output));
        }
        rebuilt
    }

    fn after_mute_change(&self, muted: bool) {
        log::info!("mute {}", if muted { "on" } else { "off" });
        self.events.publish(AppEvent::MuteChanged { muted });
        if let Some(discord) = self.discord.lock().expect("discord lock").as_ref() {
            discord.mark_panels_dirty();
        }
    }
}

/// What the codec failure handler needs from Discord: the voice channel a guild is in, and
/// leaving it. The Discord adapter in the app, a fake in the tests.
pub(crate) trait VoiceControl: Send + Sync {
    /// The guild's name and the voice channel's name, when the bot is in one.
    fn voice_channel(&self, guild: u64) -> Option<(String, String)>;
    /// Leaves the guild's voice channel as `/leave` does.
    fn leave(
        &self,
        guild: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

impl VoiceControl for DiscordRuntime {
    fn voice_channel(&self, guild: u64) -> Option<(String, String)> {
        self.status()
            .voice_channels
            .get(&guild.to_string())
            .map(|channel| (channel.guild_name.clone(), channel.channel_name.clone()))
    }

    fn leave(
        &self,
        guild: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(DiscordRuntime::leave(self, guild))
    }
}

/// An opus-rs panic on one of a guild's paths (design D5): logged, the bot leaves that
/// guild's voice channel, and the window gets its notice. The settings stay as they are.
/// A report for a guild with no voice channel (already left) is logged and dropped.
pub(crate) async fn handle_codec_failure(
    failure: CodecFailure,
    voice: &dyn VoiceControl,
    events: &EventBus,
) {
    let path = failure.path.name();
    let codec = failure.codec.label();
    let Some((guild_name, channel_name)) = voice.voice_channel(failure.guild) else {
        log::warn!(
            "{path}: {codec} panicked in guild {}, which has no voice channel now: {}",
            failure.guild,
            failure.message
        );
        return;
    };
    log::error!(
        "{path}: {codec} panicked: {}; the bot leaves the voice channel {channel_name} in {guild_name}",
        failure.message
    );
    events.log(format!(
        "the bot left {channel_name} in {guild_name}: {codec} failed while {}",
        failure.path.activity()
    ));
    voice.leave(failure.guild).await;
    events.publish(AppEvent::CodecFailure {
        guild_name,
        channel_name,
        path: path.to_owned(),
        activity: failure.path.activity().to_owned(),
        codec,
        message: failure.message,
    });
}

/// What the answer handler needs from Discord: posting an answer where a destination
/// points (change `post-assistant-answers`). The Discord adapter in the app, a fake in the
/// tests.
pub(crate) trait AnswerPoster: Send + Sync {
    fn post<'a>(
        &'a self,
        answer: &'a AgentAnswer,
        destination: AnswerDestination,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PostOutcome> + Send + 'a>>;
}

impl AnswerPoster for DiscordRuntime {
    fn post<'a>(
        &'a self,
        answer: &'a AgentAnswer,
        destination: AnswerDestination,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PostOutcome> + Send + 'a>> {
        Box::pin(self.post_answer(answer, destination))
    }
}

/// Where the settings send an answer: `None` while posting is off, except in the text
/// answers mode, whose answers always go somewhere (change `text-answers-mode`, D3).
pub(crate) fn answer_destination(settings: &AssistantSettings) -> Option<AnswerDestination> {
    match settings.post_answers {
        PostAnswers::Off if settings.mode == crate::settings::AssistantMode::Text => {
            Some(AnswerDestination::VoiceChannel)
        }
        PostAnswers::Off => None,
        PostAnswers::VoiceChannel => Some(AnswerDestination::VoiceChannel),
        PostAnswers::CommandChannel => Some(AnswerDestination::CommandChannel),
        // Validation keeps `channel` from being saved without an id; 0 is no channel and
        // resolves to "gone", which says so in the status.
        PostAnswers::Channel => Some(AnswerDestination::Channel(
            settings.answers_channel_id().unwrap_or_default(),
        )),
    }
}

/// One agent answer (design D3, D4, D8, D12): `settings` are the current ones, read when
/// the answer arrives. Off makes no call. A post is logged with its channel, parts and
/// length; a refusal sets the `answers` connection error ("cannot post to #<channel> in
/// <server>: <reason>", which the window shows after the source, "answers: "), and the
/// next post clears it. The text reaches the log file only with the verbose log.
pub(crate) async fn handle_answer(
    answer: AgentAnswer,
    settings: &AssistantSettings,
    poster: &dyn AnswerPoster,
    errors: &ConnectionErrors,
) {
    let guild = answer.guild;
    let Some(destination) = answer_destination(settings) else {
        log::info!("[answers:{guild}] the Codex answer was not posted: posting answers is off");
        return;
    };
    match poster.post(&answer, destination).await {
        PostOutcome::Posted {
            channel,
            server,
            messages,
            characters,
        } => {
            log::info!(
                "[answers:{guild}] posted the Codex answer to #{channel} in {server} ({messages} {}, {characters} characters)",
                if messages == 1 { "message" } else { "messages" }
            );
            crate::detail!("answers", "[answers:{guild}] {}", answer.text);
            errors.report("answers", None);
        }
        PostOutcome::NotPosted { reason, repeat } => {
            if !repeat {
                log::info!("[answers:{guild}] the Codex answer was not posted: {reason}");
            }
        }
        PostOutcome::Refused {
            channel,
            server,
            message,
        } => {
            // The window shows the source in front: "answers: cannot post to …".
            let line = format!("cannot post to #{channel} in {server}: {message}");
            log::warn!("[answers:{guild}] {line}");
            errors.report("answers", Some(line));
        }
    }
}

/// The task that posts the answers one after another, each with the settings stored at the
/// moment it is taken up, so a save applies to the next answer without restarting anything.
pub(crate) fn spawn_answer_handler(
    handle: &Handle,
    mut answers: tokio::sync::mpsc::Receiver<AgentAnswer>,
    settings: Arc<RwLock<Settings>>,
    poster: Arc<dyn AnswerPoster>,
    errors: Arc<ConnectionErrors>,
) -> tokio::task::JoinHandle<()> {
    handle.spawn(async move {
        while let Some(answer) = answers.recv().await {
            let current = settings.read().expect("settings lock").assistant.clone();
            handle_answer(answer, &current, poster.as_ref(), &errors).await;
        }
    })
}

pub struct Runtime {
    build_stamp: String,
    data_dir: PathBuf,
    store: SettingsStore,
    secrets: Arc<dyn SecretStore>,
    events: EventBus,
    handle: Handle,
    /// Shared with the answer handler, which reads the current settings for each answer.
    settings: Arc<RwLock<Settings>>,
    host: Arc<GateHost>,
    discord: Arc<DiscordRuntime>,
    wake: WakeManager,
    assistant: Arc<dyn Assistant>,
    errors: Arc<ConnectionErrors>,
    secrets_set: Mutex<HashSet<SecretKey>>,
    hotkey: Mutex<HotkeyStatus>,
    /// The codec failure reports, taken by `start` for its handler task.
    codec_failures: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<CodecFailure>>>,
    /// The agents' answers, taken by `start` for the answer handler task.
    answers: Mutex<Option<tokio::sync::mpsc::Receiver<AgentAnswer>>>,
    /// The owner link, shared with the gate host.
    owner: Arc<OwnerState>,
    /// The consent records, shared with the gate host (change `voice-consent`).
    consent: Arc<ConsentState>,
    /// The access mode and its voice filter, shared with the gate host.
    access: Arc<AccessState>,
}

impl Runtime {
    /// Builds the runtime with the stub assistant. `with_assistant` swaps in a real one.
    pub fn new(
        build_stamp: impl Into<String>,
        data_dir: PathBuf,
        store: SettingsStore,
        secrets: Arc<dyn SecretStore>,
        events: EventBus,
        handle: Handle,
    ) -> Self {
        Self::with_assistant(
            build_stamp,
            data_dir,
            store,
            secrets,
            events.clone(),
            handle,
            Arc::new(ConnectionErrors::new(events)),
            Arc::new(StubAssistant::default()),
        )
    }

    /// `errors` is shared with the assistant, which reports its own failures there.
    #[allow(clippy::too_many_arguments)]
    pub fn with_assistant(
        build_stamp: impl Into<String>,
        data_dir: PathBuf,
        store: SettingsStore,
        secrets: Arc<dyn SecretStore>,
        events: EventBus,
        handle: Handle,
        errors: Arc<ConnectionErrors>,
        assistant: Arc<dyn Assistant>,
    ) -> Self {
        let registry = RelayRegistry::default();
        let whisper = WhisperManager::new(
            crate::paths::assets_dir(&data_dir),
            crate::paths::whisper_models_dir(&data_dir),
            events.clone(),
            Arc::new(registry.clone()),
        );
        let tts = TtsManager::new(crate::paths::assets_dir(&data_dir), events.clone());
        assistant.use_tts(tts.clone());
        let wake = WakeManager::with_default_engines(
            crate::paths::assets_dir(&data_dir),
            crate::paths::models_dir(&data_dir),
            events.clone(),
            Arc::new(registry.clone()),
        );
        let (codec_failures, failure_reports) = CodecFailureHub::new();
        let (answers, answer_queue) = AnswerSink::channel();
        let owner = Arc::new(OwnerState::new(
            crate::owner::platform_store(&data_dir),
            events.clone(),
        ));
        let settings = Arc::new(RwLock::new(Settings::default()));
        let consent = Arc::new(ConsentState::new(
            crate::consent::platform_store(&data_dir),
            events.clone(),
            &crate::consent::recipient_key(&Settings::default()),
        ));
        let access = AccessState::new(Arc::clone(&owner), Arc::clone(&consent));
        let host = Arc::new(GateHost {
            registry,
            wake: wake.clone(),
            whisper,
            tts,
            assistant: Arc::clone(&assistant),
            events: events.clone(),
            errors: Arc::clone(&errors),
            gate_settings: Mutex::new(GateSettings::from_settings(&Settings::default())),
            discord: Mutex::new(None),
            opus_codec: Mutex::new(OpusCodec::default()),
            codec_failures,
            answers,
            owner: Arc::clone(&owner),
            consent: Arc::clone(&consent),
            access: Arc::clone(&access),
            settings: Arc::clone(&settings),
            people: Mutex::new(std::collections::HashMap::new()),
        });
        let weak = Arc::downgrade(&host);
        access.observe(Box::new(move || {
            if let Some(host) = weak.upgrade() {
                host.reapply_participants();
            }
        }));
        let discord = Arc::new(DiscordRuntime::new(
            events.clone(),
            host.clone(),
            handle.clone(),
        ));
        *host.discord.lock().expect("discord lock") = Some(Arc::clone(&discord));
        Self {
            build_stamp: build_stamp.into(),
            data_dir,
            store,
            secrets,
            events,
            handle,
            settings,
            host,
            discord,
            wake,
            assistant,
            errors,
            secrets_set: Mutex::new(HashSet::new()),
            hotkey: Mutex::new(HotkeyStatus::default()),
            codec_failures: Mutex::new(Some(failure_reports)),
            answers: Mutex::new(Some(answer_queue)),
            owner,
            consent,
            access,
        }
    }

    /// Loads settings (writing the defaults on first start), learns which secrets are set,
    /// configures the wake engine, and connects to Discord whenever a token is stored.
    /// Nothing of the assistant runs until the first `/join`.
    pub fn start(&self) -> Result<(), RuntimeError> {
        self.start_checked(|settings| {
            if let Err(error) = settings.validate() {
                self.events.log(format!(
                    "settings file has a problem ({error}); fix it in Settings"
                ));
            }
            Ok(())
        })
    }

    /// Starts with the shared settings lifecycle, but rejects settings before applying
    /// them if the supplied validator reports an error.
    ///
    /// # Errors
    /// Returns a settings I/O, parsing, or validation error before starting Discord.
    pub fn start_checked(
        &self,
        validate: impl FnOnce(&Settings) -> Result<(), ValidationError>,
    ) -> Result<(), RuntimeError> {
        self.store.cleanup_partial();
        let stored = self.store.load()?;
        let first_start = stored.is_none();
        let settings = stored.unwrap_or_default();
        validate(&settings)?;
        if first_start {
            self.store.save(&settings)?;
            self.events.log(format!(
                "no settings file yet; wrote the defaults to {}",
                self.store.path().display()
            ));
        }
        *self.settings.write().expect("settings lock") = settings;
        // Change `cross-platform-headless` (design D8): how Linux writes the token, the
        // owner link and the records, and each readable file rewritten in that form now.
        crate::protect::set_encryption(self.settings().linux.encrypt_files);
        if !cfg!(windows) {
            self.reseal_private_files();
        }
        let gate_settings = GateSettings::from_settings(&self.settings());
        log::info!(
            "settings: {}",
            crate::gate::transcript_phrases_line(&gate_settings)
        );
        *self.host.gate_settings.lock().expect("gate settings lock") = gate_settings;
        self.host.registry.gains().apply(&self.settings().audio);
        *self.host.opus_codec.lock().expect("opus codec lock") =
            self.settings().diagnostics.codec();
        self.consent
            .set_assistant(&crate::consent::recipient_key(&self.settings()));
        self.access.set_mode(self.settings().discord.access);
        // Every signed-in account the assistant reports (Check, Link, a join, the read at
        // start) is compared with the link's; another account ends the link (D4).
        {
            let mut events = self.events.subscribe();
            let owner = Arc::clone(&self.owner);
            self.handle.spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(AppEvent::Assistant(status)) => {
                            if let Some((engine, account)) = signed_in_account(&status) {
                                owner.check_account_of(&engine, &account);
                            }
                        }
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }
        crate::assets::cleanup_partial(&crate::paths::assets_dir(&self.data_dir));
        self.wake.scan_local_models();
        self.wake.configure(&self.settings());
        // Whisper and the voice report missing files now, never download them, and load
        // nothing until a join in the Whisper mode.
        self.host.whisper.scan_local_models();
        self.host.whisper.configure(&self.settings());
        self.host.tts.configure(&self.settings());
        // Nothing of the assistant runs yet: its process starts at the first /join.
        self.assistant.configure(&self.settings());
        // An opus-rs panic on a join makes the bot leave that channel (design D5).
        if let Some(mut reports) = self
            .codec_failures
            .lock()
            .expect("codec failures lock")
            .take()
        {
            let discord = Arc::clone(&self.discord);
            let events = self.events.clone();
            self.handle.spawn(async move {
                while let Some(failure) = reports.recv().await {
                    handle_codec_failure(failure, discord.as_ref(), &events).await;
                }
            });
        }
        // The agents' answers go to Discord as the settings say at that moment
        // (change `post-assistant-answers`), one after another.
        if let Some(answers) = self.answers.lock().expect("answers lock").take() {
            spawn_answer_handler(
                &self.handle,
                answers,
                Arc::clone(&self.settings),
                Arc::clone(&self.discord) as Arc<dyn AnswerPoster>,
                Arc::clone(&self.errors),
            );
        }

        for key in SecretKey::ALL {
            match self.secrets.get(*key) {
                Ok(Some(_)) => {
                    self.secrets_set.lock().expect("secrets lock").insert(*key);
                }
                Ok(None) => {}
                Err(error) => {
                    self.set_connection_error("secret_store", Some(error.to_string()));
                    log::error!("secret store: {error}");
                }
            }
        }
        log::info!("runtime started; data dir {}", self.data_dir.display());

        if self.is_secret_set(SecretKey::DiscordToken) {
            if let Err(error) = self.connect() {
                self.set_connection_error("discord", Some(error.to_string()));
            }
        }
        Ok(())
    }

    /// Linux only (change `cross-platform-headless`, design D8; not tested): rewrites the
    /// stored token, the owner link and the records in the form `linux.encrypt_files` asks
    /// for, and warns while they are stored unencrypted.
    fn reseal_private_files(&self) {
        let encrypt = crate::protect::encryption();
        let files = [
            (
                crate::secrets::token_path(&self.data_dir),
                crate::secrets::TOKEN_ENTROPY,
            ),
            (
                crate::owner::link_path(&self.data_dir),
                crate::owner::LINK_ENTROPY,
            ),
            (
                crate::consent::consents_path(&self.data_dir),
                crate::consent::ENTROPY,
            ),
        ];
        for (path, entropy) in files {
            match crate::protect::reseal(&path, entropy) {
                Ok(true) => self.events.log(format!(
                    "{} rewritten {}",
                    path.display(),
                    if encrypt {
                        "encrypted"
                    } else {
                        "unencrypted (linux.encrypt_files is off)"
                    }
                )),
                Ok(false) => {}
                Err(error) => log::warn!("{} left as it is: {error}", path.display()),
            }
        }
        if !encrypt {
            self.events.log(
                "warning: linux.encrypt_files is off, so the Discord token, the owner link and the consent records are stored unencrypted, protected only by file permissions",
            );
        }
    }

    /// Stops the wake engine and Discord; waits for Discord so the process can exit cleanly.
    pub fn shutdown(&self) {
        self.wake.stop();
        let mut off = self.settings();
        off.assistant.mode = crate::settings::AssistantMode::Realtime;
        self.host.whisper.configure(&off);
        self.host.tts.configure(&off);
        self.assistant.stop();
        let discord = Arc::clone(&self.discord);
        if discord.is_running() {
            let _ = self.handle.block_on(async move {
                tokio::time::timeout(Duration::from_secs(10), discord.stop()).await
            });
        }
        log::info!("runtime stopped");
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().expect("settings lock").clone()
    }

    pub fn wake(&self) -> &WakeManager {
        &self.wake
    }

    pub fn whisper(&self) -> &WhisperManager {
        &self.host.whisper
    }

    pub fn tts(&self) -> &TtsManager {
        &self.host.tts
    }

    pub fn assistant(&self) -> &Arc<dyn Assistant> {
        &self.assistant
    }

    /// Validates and stores a patched document, then applies the changed sections.
    pub fn save_settings(&self, patch: &Value) -> Result<Settings, RuntimeError> {
        let current = self.settings();
        let next = current.with_patch(patch)?;
        next.validate()?;
        self.store.save(&next)?;
        let changed = current.changed_sections(&next);
        *self.settings.write().expect("settings lock") = next.clone();
        self.apply(&changed);
        self.events.publish(AppEvent::SettingsChanged);
        Ok(next)
    }

    /// Reconfigures only the subsystems whose section changed.
    fn apply(&self, changed: &[&'static str]) {
        let settings = self.settings();
        let mut wake_dirty = false;
        let mut gates_dirty = false;
        let mut whisper_dirty = false;
        let mut mode_switched = false;
        let mut engine_switched = false;
        for section in changed {
            log::info!("settings: {section} changed");
            match *section {
                "discord" => {
                    log::info!("settings: the guild id applies at the next connection");
                }
                // One filter the receive path reads per tick: nothing is rebuilt. The privacy
                // URL is read by each reply.
                "owner" => {
                    self.access.set_mode(settings.discord.access);
                    log::info!(
                        "settings: access mode {}, from the next frame",
                        settings.discord.access.name()
                    );
                }
                "session" => {
                    wake_dirty = true;
                    gates_dirty = true;
                }
                // Shared atomics: the next frame sent and the next frame played use the
                // new gains, and nothing is rebuilt.
                "audio" => self.host.registry.gains().apply(&settings.audio),
                // The gates check the call's transcript for the selected model's
                // deactivation phrases; a phrase change never ends a session.
                "wake" => {
                    wake_dirty = true;
                    gates_dirty = true;
                }
                "assistant" => {
                    gates_dirty = true;
                    self.assistant.configure(&settings);
                }
                "codex" | "claude" | "instructions" => self.assistant.configure(&settings),
                // A consent names the engine's recipient (change `voice-consent`, D2; change
                // `claude-and-opencode-engines`, D10): OpenCode's provider is part of it.
                "opencode" => {
                    self.assistant.configure(&settings);
                    self.consent
                        .set_assistant(&crate::consent::recipient_key(&settings));
                }
                // Change `claude-and-opencode-engines`, D1: the engine switches at once, on
                // the same path as a mode change.
                "engine" => {
                    engine_switched = true;
                    self.assistant.configure(&settings);
                    self.consent
                        .set_assistant(&crate::consent::recipient_key(&settings));
                }
                // Change `whisper-mode`: the mode switches at once (D1); Whisper and the
                // voice reload in the background, phrases apply from the next utterance.
                "mode" => {
                    mode_switched = true;
                    wake_dirty = true;
                    whisper_dirty = true;
                    self.assistant.configure(&settings);
                }
                "whisper" => {
                    whisper_dirty = true;
                    gates_dirty = true;
                }
                "tts" => self.host.tts.configure(&settings),
                // Read by the answer handler when each answer arrives: nothing restarts.
                "answers" => {
                    let channel = match settings.assistant.answers_channel_id() {
                        Some(id) => format!(", chosen channel {id}"),
                        None => String::new(),
                    };
                    log::info!(
                        "settings: posting answers to Discord: {}{channel}, from the next answer",
                        settings.assistant.post_answers.name()
                    );
                }
                // The window and the desktop shell act on the switches themselves from the
                // settings change event. The codec is only noted for the next join and the
                // next call: nothing running is rebuilt.
                "diagnostics" => {
                    let on = |value: bool| if value { "on" } else { "off" };
                    log::info!(
                        "settings: performance monitoring {}, memory measurement {}, verbose log {}",
                        on(settings.diagnostics.performance),
                        on(settings.diagnostics.memory),
                        on(settings.diagnostics.verbose_log)
                    );
                    let codec = settings.diagnostics.codec();
                    let mut current = self.host.opus_codec.lock().expect("opus codec lock");
                    if *current != codec {
                        *current = codec;
                        log::info!(
                            "settings: Opus codec {} from the next join and the next call",
                            codec.label()
                        );
                    }
                    drop(current);
                    self.assistant.configure(&settings);
                }
                _ => {}
            }
        }
        if gates_dirty {
            let gate_settings = GateSettings::from_settings(&settings);
            log::info!(
                "settings: {}",
                crate::gate::transcript_phrases_line(&gate_settings)
            );
            *self.host.gate_settings.lock().expect("gate settings lock") = gate_settings.clone();
            self.host.registry.apply_settings(&gate_settings);
        }
        if wake_dirty {
            self.wake.configure(&settings);
        }
        if whisper_dirty {
            self.host.whisper.configure(&settings);
        }
        if mode_switched || engine_switched {
            self.host.tts.configure(&settings);
            if engine_switched {
                let rebuilt = self.host.rebuild_gates(&settings);
                let host = Arc::clone(&self.host);
                let next = settings.clone();
                self.handle.spawn(host.switch_engine(next, rebuilt));
            } else {
                self.host.switch_mode(&settings);
            }
            if settings.assistant.mode.hears_with_whisper() && !self.host.registry.all().is_empty()
            {
                // In a channel already: load now; missing files wait for the window's OK.
                let (whisper, tts) = (self.host.whisper.clone(), self.host.tts.clone());
                std::thread::spawn(move || {
                    let _ = whisper.prepare_join();
                    let _ = tts.prepare_join();
                });
            }
        }
    }

    pub fn set_secret(&self, name: &str, value: &str) -> Result<(), RuntimeError> {
        let key = SecretKey::from_name(name)
            .ok_or_else(|| RuntimeError::UnknownSecret(name.to_owned()))?;
        self.secrets.set(key, value.trim())?;
        self.secrets_set.lock().expect("secrets lock").insert(key);
        self.set_connection_error("secret_store", None);
        log::info!("secret {} set", key.name());
        self.events.publish(AppEvent::SecretChanged {
            name: key.name().to_owned(),
            is_set: true,
        });
        if key == SecretKey::DiscordToken {
            self.reconnect();
        }
        Ok(())
    }

    pub fn clear_secret(&self, name: &str) -> Result<(), RuntimeError> {
        let key = SecretKey::from_name(name)
            .ok_or_else(|| RuntimeError::UnknownSecret(name.to_owned()))?;
        self.secrets.delete(key)?;
        self.secrets_set.lock().expect("secrets lock").remove(&key);
        log::info!("secret {} cleared", key.name());
        self.events.publish(AppEvent::SecretChanged {
            name: key.name().to_owned(),
            is_set: false,
        });
        if key == SecretKey::DiscordToken {
            self.disconnect();
        }
        Ok(())
    }

    /// The value itself, for the one consumer that needs it. Never logged or serialized.
    pub fn secret(&self, key: SecretKey) -> Result<Option<String>, RuntimeError> {
        Ok(self.secrets.get(key)?)
    }

    pub fn is_secret_set(&self, key: SecretKey) -> bool {
        self.secrets_set
            .lock()
            .expect("secrets lock")
            .contains(&key)
    }

    /// Starts the Discord client with the stored token.
    pub fn connect(&self) -> Result<(), RuntimeError> {
        let token = self
            .secret(SecretKey::DiscordToken)?
            .ok_or(DiscordError::NoToken)?;
        let guild = self.settings().discord.guild_id_value().map(GuildId::new);
        self.discord.start(token, guild)?;
        self.set_connection_error("discord", None);
        Ok(())
    }

    /// Stops the Discord client in the background.
    pub fn disconnect(&self) {
        let discord = Arc::clone(&self.discord);
        self.handle.spawn(async move {
            discord.stop().await;
        });
    }

    /// Stops, then starts again with the current token. Used when the token changes.
    fn reconnect(&self) {
        let discord = Arc::clone(&self.discord);
        let token = self.secret(SecretKey::DiscordToken).ok().flatten();
        let guild = self.settings().discord.guild_id_value().map(GuildId::new);
        let events = self.events.clone();
        self.handle.spawn(async move {
            discord.stop().await;
            if let Some(token) = token {
                if let Err(error) = discord.start(token, guild) {
                    events.log(format!("Discord: could not reconnect: {error}"));
                }
            }
        });
    }

    pub fn is_muted(&self) -> bool {
        self.host.registry.is_muted()
    }

    /// The text channels the window offers for answers; none while not connected.
    pub fn text_channels(&self) -> Vec<TextChannelGroup> {
        self.discord.text_channels()
    }

    pub fn set_muted(&self, muted: bool) -> bool {
        if self.host.registry.set_muted(muted) {
            self.host.after_mute_change(muted);
        }
        muted
    }

    /// The single entry for all three mute sources (app button, hotkey, Discord panel).
    pub fn toggle_mute(&self) -> bool {
        self.set_muted(!self.is_muted())
    }

    /// The desktop shell reports the global hotkey registration here.
    pub fn set_hotkey_status(&self, status: HotkeyStatus) {
        match &status.error {
            Some(error) => self.events.log(format!(
                "hotkey «{}» not registered: {error}; the app button and the Discord panel still work",
                status.shortcut
            )),
            None if status.registered => {
                log::info!("hotkey «{}» registered", status.shortcut);
            }
            None => log::info!("no mute hotkey configured"),
        }
        *self.hotkey.lock().expect("hotkey lock") = status.clone();
        self.events.publish(AppEvent::Hotkey(Box::new(status)));
    }

    pub fn set_connection_error(&self, source: &str, message: Option<String>) {
        self.errors.report(source, message);
    }

    pub fn status(&self) -> Status {
        let set = self.secrets_set.lock().expect("secrets lock").clone();
        Status {
            build_stamp: self.build_stamp.clone(),
            data_dir: self.data_dir.display().to_string(),
            muted: self.is_muted(),
            hotkey: self.hotkey.lock().expect("hotkey lock").clone(),
            secrets: SecretKey::ALL
                .iter()
                .map(|key| {
                    (
                        key.name().to_owned(),
                        SecretStatus {
                            is_set: set.contains(key),
                        },
                    )
                })
                .collect(),
            connection_errors: self.errors.snapshot(),
            discord: self.discord.status(),
            wake: self.wake.status(),
            whisper: self.host.whisper.status(),
            tts: self.host.tts.status(),
            gates: self.host.registry.statuses(),
            audio: self.host.registry.gains().levels(),
            latency: self.host.registry.latency_reports(),
            assistant: self.assistant.status(),
            owner: self.owner.status(),
            consent: self.consent.status(),
            settings: self.settings(),
        }
    }

    /// Remove next to a person in the Access card (change `voice-consent`): their record is
    /// deleted as by their `/optout`, and nothing is sent to them. `id` is the Discord user
    /// id as the window holds it, a string.
    pub fn consent_remove(&self, id: &str) -> Result<crate::consent::ConsentStatus, String> {
        let user: u64 = id
            .trim()
            .parse()
            .map_err(|_| format!("«{id}» is not a Discord user id"))?;
        self.consent.withdraw(user, true);
        Ok(self.consent.status())
    }

    /// Link in the window, after the window had the assistant read its account (as Check
    /// does): a code bound to that account. Refused while it is not signed in.
    pub fn start_link(&self) -> Result<PendingCode, String> {
        let status = self.assistant.status();
        let Some((engine, account)) = signed_in_account(&status) else {
            let product = crate::settings::AssistantEngine::parse(&status.engine)
                .map(crate::settings::AssistantEngine::product_name)
                .unwrap_or("The assistant");
            return Err(if self.events.terminal_guidance() {
                format!(
                    "{product} is not signed in: sign in to {product} from a terminal as the user that runs VoiceBro first, then run voicebro-server --link-owner again"
                )
            } else {
                format!(
                    "{product} is not signed in: use Sign in on the Status tab first, then press Link again"
                )
            });
        };
        self.owner.check_account_of(&engine, &account);
        self.owner
            .issue_code_for(&engine, account)
            .map_err(|error| error.to_string())
    }

    /// Cancel in the window; `false` when no code was waiting.
    pub fn cancel_link(&self) -> bool {
        self.owner.cancel_code()
    }

    /// Unlink in the window; `false` when nothing was linked.
    pub fn unlink(&self) -> Result<bool, String> {
        self.owner.unlink()
    }

    pub fn owner(&self) -> &Arc<OwnerState> {
        &self.owner
    }

    /// The people who agreed, for the headless app's terminal (change
    /// `cross-platform-headless`); the window reads them through its own command.
    pub fn consent_status(&self) -> crate::consent::ConsentStatus {
        self.consent.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::output_pair;
    use crate::secrets::MemoryStore;
    use serde_json::json;

    fn runtime(dir: &tempfile::TempDir, tokio_rt: &tokio::runtime::Runtime) -> Runtime {
        Runtime::new(
            "0.1.0+test 2026-11-09",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            EventBus::new(16),
            tokio_rt.handle().clone(),
        )
    }

    #[test]
    fn first_start_writes_defaults_and_status_has_no_secret_values() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        assert!(dir.path().join("settings.json").exists());
        // A nonsense token: the connect attempt fails in the background; the secret path
        // itself must still behave.
        runtime
            .set_secret("discord_token", "  super-secret-token ")
            .unwrap();
        let status = runtime.status();
        assert!(status.secrets["discord_token"].is_set);
        let json = serde_json::to_string(&status).unwrap();
        assert!(
            !json.contains("super-secret-token"),
            "status must never carry the value"
        );
        assert_eq!(
            runtime.secret(SecretKey::DiscordToken).unwrap().as_deref(),
            Some("super-secret-token"),
            "the consumer gets the trimmed value"
        );
        runtime.clear_secret("discord_token").unwrap();
        assert!(!runtime.status().secrets["discord_token"].is_set);
        runtime.shutdown();
    }

    #[test]
    fn a_saved_opus_codec_is_kept_for_the_next_join_and_rebuilds_nothing() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        assert_eq!(runtime.host.opus_codec(), OpusCodec::Libopus);
        let mut events = runtime.events().subscribe();
        runtime
            .save_settings(&json!({ "diagnostics": { "opus_codec": "opus-rs" } }))
            .unwrap();
        assert_eq!(
            runtime.host.opus_codec(),
            OpusCodec::OpusRs,
            "the next join reads it"
        );
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, AppEvent::Wake(_)),
                "the wake engine is not reconfigured"
            );
        }
        assert!(runtime
            .save_settings(&json!({ "diagnostics": { "opus_codec": "opus2" } }))
            .is_err());
        assert_eq!(runtime.settings().diagnostics.opus_codec, "opus-rs");
        assert_eq!(runtime.host.opus_codec(), OpusCodec::OpusRs);
        runtime.shutdown();

        // A restart takes the stored codec.
        let again = super::tests::runtime(&dir, &tokio_rt);
        again.start().unwrap();
        assert_eq!(again.host.opus_codec(), OpusCodec::OpusRs);
        again.shutdown();
    }

    /// Change `config-tab-and-codex-lists`: the lists switch and the collapsed cards are
    /// stored and restart nothing; the wake engine and the gates are left alone.
    #[test]
    fn the_lists_switch_and_the_collapsed_cards_restart_nothing() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        let gate_settings = runtime
            .host
            .gate_settings
            .lock()
            .expect("gate settings lock")
            .clone();
        let mut events = runtime.events().subscribe();
        runtime
            .save_settings(&json!({ "codex": { "refresh_lists_at_start": true } }))
            .unwrap();
        runtime
            .save_settings(&json!({ "ui": { "collapsed_cards": ["audio", "hotkey"] } }))
            .unwrap();
        let mut changes = 0;
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, AppEvent::Wake(_)),
                "the wake engine is not reconfigured"
            );
            if matches!(event, AppEvent::SettingsChanged) {
                changes += 1;
            }
        }
        assert_eq!(changes, 2, "each save tells the window");
        assert_eq!(
            *runtime
                .host
                .gate_settings
                .lock()
                .expect("gate settings lock"),
            gate_settings,
            "the gates keep their settings"
        );
        runtime.shutdown();

        let again = super::tests::runtime(&dir, &tokio_rt);
        again.start().unwrap();
        let stored = again.settings();
        assert!(stored.codex.refresh_lists_at_start);
        assert_eq!(stored.ui.collapsed_cards, ["audio", "hotkey"]);
        again.shutdown();
    }

    /// Change `voice-consent`, scenario "Changed while in a channel": the access mode is
    /// stored and rebuilds no gate, wake engine or call.
    #[test]
    fn the_access_mode_restarts_nothing() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        let gate_settings = runtime
            .host
            .gate_settings
            .lock()
            .expect("gate settings lock")
            .clone();
        let mut events = runtime.events().subscribe();
        runtime
            .save_settings(&json!({ "discord": { "access": "agreed" } }))
            .unwrap();
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, AppEvent::Wake(_)),
                "the wake engine is not reconfigured"
            );
        }
        assert_eq!(
            *runtime
                .host
                .gate_settings
                .lock()
                .expect("gate settings lock"),
            gate_settings,
            "the gates keep their settings"
        );
        let error = runtime
            .save_settings(&json!({ "discord": { "access": "friends" } }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("discord.access"), "{error}");
        assert_eq!(
            runtime.settings().discord.access,
            AccessMode::Agreed,
            "the stored value stays"
        );
        runtime.shutdown();

        let again = super::tests::runtime(&dir, &tokio_rt);
        again.start().unwrap();
        assert_eq!(again.settings().discord.access, AccessMode::Agreed);
        again.shutdown();
    }

    /// The stub assistant, counting how often the settings reached it.
    struct CountingAssistant {
        inner: StubAssistant,
        configured: std::sync::atomic::AtomicUsize,
    }

    impl Assistant for CountingAssistant {
        fn id(&self) -> &'static str {
            self.inner.id()
        }
        fn attach(
            &self,
            guild: u64,
            ports: AudioPorts,
        ) -> Result<Arc<dyn crate::assistant::AssistantSession>, crate::assistant::AssistantError>
        {
            self.inner.attach(guild, ports)
        }
        fn detach(&self, guild: u64) {
            self.inner.detach(guild);
        }
        fn capabilities(&self) -> crate::assistant::Capabilities {
            self.inner.capabilities()
        }
        fn status(&self) -> crate::assistant::AssistantStatus {
            self.inner.status()
        }
        fn configure(&self, _settings: &Settings) {
            self.configured
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// The stub assistant, signed in as the account the test sets (change
    /// `owner-only-access`).
    #[derive(Default)]
    struct AccountAssistant {
        inner: StubAssistant,
        email: Mutex<Option<String>>,
    }

    impl AccountAssistant {
        fn sign_in(&self, email: Option<&str>) {
            *self.email.lock().unwrap() = email.map(str::to_owned);
        }
    }

    impl Assistant for AccountAssistant {
        fn id(&self) -> &'static str {
            self.inner.id()
        }
        fn attach(
            &self,
            guild: u64,
            ports: AudioPorts,
        ) -> Result<Arc<dyn crate::assistant::AssistantSession>, crate::assistant::AssistantError>
        {
            self.inner.attach(guild, ports)
        }
        fn detach(&self, guild: u64) {
            self.inner.detach(guild);
        }
        fn capabilities(&self) -> crate::assistant::Capabilities {
            self.inner.capabilities()
        }
        fn status(&self) -> crate::assistant::AssistantStatus {
            let email = self.email.lock().unwrap().clone();
            // It stands in for Codex: the link records the engine it was made with.
            crate::assistant::AssistantStatus {
                engine: "codex".to_owned(),
                signed_in: email.is_some(),
                account_type: email.as_ref().map(|_| "chatgpt".to_owned()),
                email,
                ..self.inner.status()
            }
        }
    }

    fn owner_runtime(
        dir: &tempfile::TempDir,
        tokio_rt: &tokio::runtime::Runtime,
    ) -> (Runtime, Arc<AccountAssistant>) {
        let events = EventBus::new(256);
        let assistant = Arc::new(AccountAssistant::default());
        let runtime = Runtime::with_assistant(
            "0.1.0+test 2026-11-16",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            events.clone(),
            tokio_rt.handle().clone(),
            Arc::new(ConnectionErrors::new(events)),
            assistant.clone(),
        );
        runtime.start().unwrap();
        (runtime, assistant)
    }

    /// Change `owner-only-access`, scenarios "Codex not signed in", "Successful link",
    /// "Unlink" and "The code stays in the window": Link needs a signed-in assistant, the
    /// code links the account that sent it, the file is written and removed, and no line of
    /// the log holds the code.
    #[test]
    fn link_needs_a_signed_in_assistant_and_links_the_sender() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (runtime, assistant) = owner_runtime(&dir, &tokio_rt);
        let mut window = runtime.events().subscribe();
        let error = runtime.start_link().unwrap_err();
        assert!(error.contains("not signed in"), "{error}");
        assert!(runtime.status().owner.pending.is_none(), "no code");

        assistant.sign_in(Some("me@example.com"));
        let pending = runtime.start_link().unwrap();
        assert_eq!(runtime.status().owner.pending.as_ref(), Some(&pending));
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        assert_eq!(
            host.complete_link(1091401169441656933, "Alex", "WRONG-CODE"),
            CompleteOutcome::Wrong { tries_left: 4 }
        );
        assert_eq!(
            host.complete_link(1091401169441656933, "Alex", &pending.code),
            CompleteOutcome::Linked {
                name: "Alex".to_owned()
            }
        );
        let owner = runtime.status().owner;
        assert_eq!(owner.linked.unwrap().id, "1091401169441656933");
        assert!(owner.pending.is_none());
        if cfg!(windows) {
            assert!(
                crate::owner::link_path(dir.path()).exists(),
                "the file is written"
            );
        }

        assert!(runtime.unlink().unwrap());
        assert!(runtime.status().owner.linked.is_none());
        assert!(!crate::owner::link_path(dir.path()).exists());

        let bare = pending.code.replace('-', "");
        let mut lines = 0;
        while let Ok(event) = window.try_recv() {
            let shown = format!("{event:?}");
            assert!(
                !shown.contains(&bare) && !shown.contains(&pending.code),
                "{shown}"
            );
            if let AppEvent::Log { line } = event {
                assert!(
                    !line.contains("WRONG"),
                    "the typed value is not logged: {line}"
                );
                lines += 1;
            }
        }
        assert!(lines >= 4, "issued, wrong, linked, removed");
        runtime.shutdown();
    }

    /// Scenario "Mode on, nothing linked" and the filter values the receive path reads.
    #[test]
    fn the_mode_and_the_link_set_the_voice_filter() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (runtime, assistant) = owner_runtime(&dir, &tokio_rt);
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        assert_eq!(*host.voice_filter(), VoiceFilter::Everyone);
        runtime
            .save_settings(&json!({ "discord": { "access": "owner" } }))
            .unwrap();
        assert_eq!(*host.voice_filter(), only(&[]), "nobody");
        assistant.sign_in(Some("me@example.com"));
        let pending = runtime.start_link().unwrap();
        host.complete_link(42_000_000_000, "Alex", &pending.code);
        assert_eq!(*host.voice_filter(), only(&[42_000_000_000]));
        runtime
            .save_settings(&json!({ "discord": { "access": "everyone" } }))
            .unwrap();
        assert_eq!(*host.voice_filter(), VoiceFilter::Everyone);
        runtime.shutdown();

        let (again, _) = owner_runtime(&dir, &tokio_rt);
        if cfg!(windows) {
            assert_eq!(
                again.status().owner.linked.map(|owner| owner.name),
                Some("Alex".to_owned()),
                "a restart keeps the link"
            );
        }
        again.shutdown();
    }

    /// Scenarios "Check finds another account" and "Signed out": every signed-in status the
    /// assistant publishes is compared with the link's account.
    #[test]
    fn another_account_in_an_assistant_status_ends_the_link() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (runtime, assistant) = owner_runtime(&dir, &tokio_rt);
        runtime
            .save_settings(&json!({ "discord": { "access": "owner" } }))
            .unwrap();
        assistant.sign_in(Some("me@example.com"));
        let pending = runtime.start_link().unwrap();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.complete_link(111, "Alex", &pending.code);
        let publish = |status: crate::assistant::AssistantStatus| {
            runtime
                .events()
                .publish(AppEvent::Assistant(Box::new(status)));
            // The comparison runs on the runtime's event task.
            std::thread::sleep(Duration::from_millis(100));
        };

        publish(assistant.status());
        assert!(runtime.status().owner.linked.is_some(), "the same account");
        assistant.sign_in(None);
        publish(assistant.status());
        assert!(
            runtime.status().owner.linked.is_some(),
            "signed out keeps it"
        );

        assistant.sign_in(Some("you@example.com"));
        publish(assistant.status());
        let owner = runtime.status().owner;
        assert!(owner.linked.is_none());
        assert_eq!(
            owner.link_ended.as_deref(),
            Some("the Codex account changed")
        );
        assert_eq!(*host.voice_filter(), only(&[]));
        runtime.shutdown();
    }

    fn only(ids: &[u64]) -> VoiceFilter {
        VoiceFilter::Only(ids.iter().copied().collect())
    }

    /// Change `voice-consent`: the `agreed` mode lets through the owner and the recorded, an
    /// "I agree" and a Remove apply at once, and the window's status and event follow.
    #[test]
    fn the_agreed_mode_follows_the_records() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (runtime, _) = owner_runtime(&dir, &tokio_rt);
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        runtime
            .save_settings(&json!({ "discord": { "access": "agreed" } }))
            .unwrap();
        assert_eq!(host.access_mode(), AccessMode::Agreed);
        assert_eq!(
            *host.voice_filter(),
            only(&[]),
            "nobody agreed, nobody linked"
        );
        let mut events = runtime.events().subscribe();
        assert_eq!(
            host.agree(7, "Alex", crate::consent::CONSENT_TEXT_VERSION),
            AgreeOutcome::Recorded
        );
        assert_eq!(*host.voice_filter(), only(&[7]));
        assert!(std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, AppEvent::Consent(_))));
        assert_eq!(runtime.status().consent.records[0].name, "Alex");
        assert_eq!(
            host.passing_names(&[(7, "Alex".to_owned()), (8, "Sam".to_owned())]),
            ["Alex"]
        );
        // Requirement "Call context for the assistant": a joined gate gets only the names
        // the filter lets through, and again after each change of it.
        let (output, _source) = crate::audio::output_pair();
        host.on_join(42, output);
        host.set_participants(42, &[(7, "Alex".to_owned()), (8, "Sam".to_owned())]);
        let gate = runtime.host.registry.get(42).expect("a gate");
        assert_eq!(gate.participants(), ["Alex"], "Sam has no record");
        assert_eq!(
            host.agree(8, "Sam", crate::consent::CONSENT_TEXT_VERSION),
            AgreeOutcome::Recorded
        );
        assert_eq!(gate.participants(), ["Alex", "Sam"], "after Sam's /optin");
        runtime
            .save_settings(&json!({ "discord": { "access": "everyone" } }))
            .unwrap();
        assert_eq!(gate.participants(), ["Alex", "Sam"]);
        runtime
            .save_settings(&json!({ "discord": { "access": "owner" } }))
            .unwrap();
        assert!(gate.participants().is_empty(), "owner mode, nobody linked");
        runtime
            .save_settings(&json!({ "discord": { "access": "agreed" } }))
            .unwrap();
        host.withdraw(8);
        host.on_leave(42);
        let after = runtime.consent_remove("7").unwrap();
        assert!(after.records.is_empty());
        assert_eq!(*host.voice_filter(), only(&[]));
        assert!(runtime.consent_remove("not a number").is_err());
        assert_eq!(host.withdraw(7), WithdrawOutcome::NothingToRemove);
        runtime.shutdown();
    }

    /// Scenario "Codex signed in with another ChatGPT account": the join preparation reads
    /// the account; the link ends before it returns, and the join goes on.
    #[test]
    fn a_join_with_another_account_ends_the_link_and_still_joins() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (runtime, assistant) = owner_runtime(&dir, &tokio_rt);
        // Always listening: the scratch folder has no Vosk files, which would refuse a
        // wake-word join (change `models-in-settings`).
        runtime
            .save_settings(&json!({
                "discord": { "access": "owner" },
                "session": { "mode": "always" }
            }))
            .unwrap();
        assistant.sign_in(Some("me@example.com"));
        let pending = runtime.start_link().unwrap();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.complete_link(111, "Alex", &pending.code);
        assert_eq!(*host.voice_filter(), only(&[111]));

        assistant.sign_in(Some("you@example.com"));
        let prepared = tokio_rt.block_on(host.prepare_join(42));
        assert_eq!(prepared, Ok(()), "the join goes on");
        assert_eq!(*host.voice_filter(), only(&[]));
        assert_eq!(
            runtime.status().owner.link_ended.as_deref(),
            Some("the Codex account changed")
        );
        runtime.shutdown();
    }

    /// Change `post-assistant-answers`: a save of only the two answer posting fields
    /// restarts nothing. The gate stays (same gate, same settings), the wake engine is not
    /// configured again, the assistant gets no new settings, so no call is touched.
    /// Change `interrupt-on-speech`, requirement "Interrupt on speech setting": a save of
    /// only the switch keeps the gate (no rebuild) and reaches its settings for the next
    /// tick; in the Whisper mode the gate's flag is on.
    #[test]
    fn a_save_of_the_interrupt_switch_rebuilds_no_gate() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({
                "assistant": { "mode": "whisper" },
                "session": { "mode": "always", "earcons": false }
            }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output);
        let gate = runtime.host.registry.get(42).unwrap();
        assert!(
            !runtime
                .host
                .gate_settings
                .lock()
                .unwrap()
                .interrupt_on_speech
        );
        runtime
            .save_settings(&json!({ "assistant": { "interrupt_on_speech": true } }))
            .unwrap();
        assert!(
            Arc::ptr_eq(&gate, &runtime.host.registry.get(42).unwrap()),
            "the same gate"
        );
        assert!(
            runtime
                .host
                .gate_settings
                .lock()
                .unwrap()
                .interrupt_on_speech
        );
        host.on_leave(42);
        runtime.shutdown();
    }

    #[test]
    fn a_save_of_the_answer_posting_fields_restarts_nothing() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let events = EventBus::new(64);
        let assistant = Arc::new(CountingAssistant {
            inner: StubAssistant::new(Duration::from_millis(10)),
            configured: std::sync::atomic::AtomicUsize::new(0),
        });
        let runtime = Runtime::with_assistant(
            "0.1.0+test 2026-11-16",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            events.clone(),
            tokio_rt.handle().clone(),
            Arc::new(ConnectionErrors::new(events)),
            assistant.clone(),
        );
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always", "earcons": false } }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output);
        let gate = runtime.host.registry.get(42).unwrap();
        let gate_settings = runtime.host.gate_settings.lock().unwrap().clone();
        let configured = assistant
            .configured
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut window = runtime.events().subscribe();

        for patch in [
            json!({ "assistant": { "post_answers": "voice_channel" } }),
            json!({ "assistant": { "post_answers": "channel", "answers_channel": "1091401169890443327" } }),
            json!({ "assistant": { "post_answers": "off" } }),
        ] {
            runtime.save_settings(&patch).unwrap();
        }
        assert!(
            Arc::ptr_eq(&gate, &runtime.host.registry.get(42).unwrap()),
            "the gate stays registered and is not rebuilt"
        );
        assert_eq!(*runtime.host.gate_settings.lock().unwrap(), gate_settings);
        assert!(gate.status().active, "the session runs on");
        assert_eq!(
            assistant
                .configured
                .load(std::sync::atomic::Ordering::Relaxed),
            configured,
            "the assistant, and so its call, is not reconfigured"
        );
        let mut saves = 0;
        while let Ok(event) = window.try_recv() {
            match event {
                AppEvent::SettingsChanged => saves += 1,
                AppEvent::Wake(_) => panic!("the wake engine is not configured again"),
                AppEvent::Gate { .. } => panic!("no gate changed"),
                _ => {}
            }
        }
        assert_eq!(saves, 3);
        let stored = runtime.settings().assistant;
        assert_eq!(stored.post_answers, crate::settings::PostAnswers::Off);
        assert_eq!(
            stored.answers_channel, "1091401169890443327",
            "kept while off"
        );

        // An unknown destination is refused and the stored settings stay in effect.
        let error = runtime
            .save_settings(&json!({ "assistant": { "post_answers": "thread" } }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("assistant.post_answers"), "{error}");
        assert_eq!(runtime.settings().assistant, stored);
        runtime.shutdown();
    }

    /// A Discord stand-in for the answers: records each post and answers with the next
    /// outcome given (a post by default).
    #[derive(Default)]
    struct FakePoster {
        posts: Mutex<Vec<(AgentAnswer, AnswerDestination)>>,
        outcomes: Mutex<std::collections::VecDeque<PostOutcome>>,
    }

    impl AnswerPoster for FakePoster {
        fn post<'a>(
            &'a self,
            answer: &'a AgentAnswer,
            destination: AnswerDestination,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PostOutcome> + Send + 'a>> {
            self.posts
                .lock()
                .unwrap()
                .push((answer.clone(), destination));
            let outcome =
                self.outcomes
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(PostOutcome::Posted {
                        channel: "answers".to_owned(),
                        server: "Test server".to_owned(),
                        messages: 1,
                        characters: answer.text.len(),
                    });
            Box::pin(async move { outcome })
        }
    }

    fn answer(text: &str) -> AgentAnswer {
        AgentAnswer {
            guild: 7,
            task: Some("find the weather in Bangkok".to_owned()),
            text: text.to_owned(),
            engine: "Codex",
        }
    }

    /// Waits until the poster has `count` posts, or a second.
    fn wait_for_posts(poster: &FakePoster, count: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while poster.posts.lock().unwrap().len() < count && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The answer handler reads the settings stored when each answer arrives: off makes no
    /// call, and a change between two answers sends the second one to the new destination.
    #[test]
    fn each_answer_goes_where_the_settings_say_when_it_arrives() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let events = EventBus::new(16);
        let errors = Arc::new(ConnectionErrors::new(events));
        let settings = Arc::new(RwLock::new(Settings::default()));
        let poster = Arc::new(FakePoster::default());
        let (sink, queue) = AnswerSink::channel();
        let handler = spawn_answer_handler(
            tokio_rt.handle(),
            queue,
            Arc::clone(&settings),
            poster.clone(),
            Arc::clone(&errors),
        );

        // Off, the default: no call.
        assert!(sink.send(answer("first")));
        std::thread::sleep(Duration::from_millis(100));
        assert!(poster.posts.lock().unwrap().is_empty(), "off makes no call");

        settings.write().unwrap().assistant.post_answers = PostAnswers::VoiceChannel;
        assert!(sink.send(answer("second")));
        wait_for_posts(&poster, 1);
        {
            let mut stored = settings.write().unwrap();
            stored.assistant.post_answers = PostAnswers::Channel;
            stored.assistant.answers_channel = "1091401169890443327".to_owned();
        }
        assert!(sink.send(answer("third")));
        wait_for_posts(&poster, 2);
        settings.write().unwrap().assistant.post_answers = PostAnswers::CommandChannel;
        assert!(sink.send(answer("fourth")));
        wait_for_posts(&poster, 3);

        let posts = poster.posts.lock().unwrap().clone();
        assert_eq!(
            posts
                .iter()
                .map(|(answer, destination)| (answer.text.as_str(), *destination))
                .collect::<Vec<_>>(),
            vec![
                ("second", AnswerDestination::VoiceChannel),
                ("third", AnswerDestination::Channel(1091401169890443327)),
                ("fourth", AnswerDestination::CommandChannel),
            ]
        );
        assert_eq!(
            posts[0].0.task.as_deref(),
            Some("find the weather in Bangkok")
        );
        assert!(errors.snapshot().is_empty(), "posts set no error");
        handler.abort();
    }

    /// Requirement "Only the final answer of a completed agent task is posted" (change
    /// `text-answers-mode`, D3): posting off sends the text answers mode's answers to the voice
    /// channel's chat, and nothing in the other two modes; a chosen destination stays.
    #[test]
    fn the_text_answers_mode_posts_with_posting_off() {
        use crate::settings::AssistantMode;
        let with = |mode: AssistantMode, post_answers: PostAnswers| {
            answer_destination(&AssistantSettings {
                mode,
                post_answers,
                answers_channel: "13".to_owned(),
                ..AssistantSettings::default()
            })
        };
        assert_eq!(
            with(AssistantMode::Text, PostAnswers::Off),
            Some(AnswerDestination::VoiceChannel)
        );
        assert_eq!(with(AssistantMode::Whisper, PostAnswers::Off), None);
        assert_eq!(with(AssistantMode::Realtime, PostAnswers::Off), None);
        assert_eq!(
            with(AssistantMode::Text, PostAnswers::Channel),
            Some(AnswerDestination::Channel(13))
        );
        assert_eq!(
            with(AssistantMode::Text, PostAnswers::CommandChannel),
            Some(AnswerDestination::CommandChannel)
        );
    }

    /// A refusal sets the `answers` error with the channel and Discord's reason; the next
    /// post clears it. An answer not posted on purpose sets nothing.
    #[test]
    fn a_refused_post_sets_the_answers_error_and_the_next_post_clears_it() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let events = EventBus::new(16);
        let mut window = events.subscribe();
        let errors = ConnectionErrors::new(events);
        let mut settings = AssistantSettings {
            post_answers: PostAnswers::Channel,
            answers_channel: "13".to_owned(),
            ..AssistantSettings::default()
        };
        let poster = FakePoster::default();
        poster.outcomes.lock().unwrap().extend([
            PostOutcome::Refused {
                channel: "rules".to_owned(),
                server: "Test server".to_owned(),
                message:
                    "Missing Permissions (HTTP 403, code 50013); the bot lacks Send Messages here"
                        .to_owned(),
            },
            PostOutcome::NotPosted {
                reason: "the bot had left the voice channel in Test server".to_owned(),
                repeat: false,
            },
        ]);
        tokio_rt.block_on(handle_answer(answer("one"), &settings, &poster, &errors));
        let error = errors.snapshot()["answers"].clone();
        assert_eq!(
            format!("answers: {error}"),
            "answers: cannot post to #rules in Test server: Missing Permissions (HTTP 403, code 50013); the bot lacks Send Messages here",
            "the window shows the source in front of the message"
        );
        assert!(matches!(
            window.try_recv(),
            Ok(AppEvent::ConnectionError { source, message: Some(_) }) if source == "answers"
        ));

        tokio_rt.block_on(handle_answer(answer("two"), &settings, &poster, &errors));
        assert!(
            errors.snapshot().contains_key("answers"),
            "an answer not posted on purpose leaves the error"
        );

        settings.answers_channel = "14".to_owned();
        tokio_rt.block_on(handle_answer(answer("three"), &settings, &poster, &errors));
        assert!(
            !errors.snapshot().contains_key("answers"),
            "a post clears it"
        );
        assert!(matches!(
            window.try_recv(),
            Ok(AppEvent::ConnectionError { source, message: None }) if source == "answers"
        ));
        assert_eq!(poster.posts.lock().unwrap().len(), 3);
        assert_eq!(
            poster.posts.lock().unwrap()[2].1,
            AnswerDestination::Channel(14)
        );
    }

    /// A Discord stand-in: the guilds the bot is in, and the leaves asked for.
    struct FakeVoice {
        channels: Mutex<std::collections::HashMap<u64, (String, String)>>,
        left: Mutex<Vec<u64>>,
    }

    impl VoiceControl for FakeVoice {
        fn voice_channel(&self, guild: u64) -> Option<(String, String)> {
            self.channels.lock().unwrap().get(&guild).cloned()
        }

        fn leave(
            &self,
            guild: u64,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                self.channels.lock().unwrap().remove(&guild);
                self.left.lock().unwrap().push(guild);
            })
        }
    }

    #[test]
    fn a_codec_panic_makes_the_bot_leave_that_channel_and_tells_the_window() {
        use crate::audio::codec::{CodecError, CodecPath};
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let voice = FakeVoice {
            channels: Mutex::new(
                [
                    (1, ("Test server".to_owned(), "General".to_owned())),
                    (2, ("Other".to_owned(), "Lobby".to_owned())),
                ]
                .into(),
            ),
            left: Mutex::new(Vec::new()),
        };
        let events = EventBus::new(16);
        let mut window = events.subscribe();
        let (hub, mut reports) = CodecFailureHub::new();
        let panic = CodecError::Panicked {
            codec: OpusCodec::OpusRs,
            detail: "attempt to subtract with overflow".to_owned(),
        };
        // Two paths of guild 1 panic; a guild the bot is not in panics too.
        assert!(hub.for_guild(1).report(CodecPath::CallReceive, &panic));
        assert!(!hub.for_guild(1).report(CodecPath::DiscordOut, &panic));
        assert!(hub.for_guild(9).report(CodecPath::Receive, &panic));
        tokio_rt.block_on(async {
            while let Ok(failure) = reports.try_recv() {
                handle_codec_failure(failure, &voice, &events).await;
            }
        });
        assert_eq!(
            *voice.left.lock().unwrap(),
            vec![1],
            "only the failed guild is left"
        );
        assert!(voice.voice_channel(2).is_some(), "the other guild stays");
        let mut notices = Vec::new();
        while let Ok(event) = window.try_recv() {
            if let AppEvent::CodecFailure {
                guild_name,
                channel_name,
                path,
                codec,
                message,
                ..
            } = event
            {
                notices.push((guild_name, channel_name, path, codec, message));
            }
        }
        assert_eq!(
            notices,
            vec![(
                "Test server".to_owned(),
                "General".to_owned(),
                "call-receive".to_owned(),
                "opus-rs 0.1.42".to_owned(),
                "attempt to subtract with overflow".to_owned()
            )],
            "one notice, for the guild the bot left"
        );
    }

    #[test]
    fn a_codec_panic_leaves_the_settings_and_a_new_join_reports_again() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "diagnostics": { "opus_codec": "opus-rs" } }))
            .unwrap();
        let panic = crate::audio::codec::CodecError::Panicked {
            codec: OpusCodec::OpusRs,
            detail: "boom".to_owned(),
        };
        let failures = runtime.host.codec_failures(3);
        assert!(failures.report(crate::audio::codec::CodecPath::Receive, &panic));
        // The handler finds no voice channel of guild 3 (not connected) and drops it.
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            runtime.settings().diagnostics.opus_codec,
            "opus-rs",
            "not swapped"
        );
        assert!(
            runtime
                .host
                .codec_failures(3)
                .report(crate::audio::codec::CodecPath::Receive, &panic),
            "a new join of the guild reports again"
        );
        runtime.shutdown();
    }

    #[test]
    fn a_settings_file_with_a_removed_codec_starts_with_libopus_and_saves() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{ "version": 2, "diagnostics": { "opus_codec": "mousiki" } }"#,
        )
        .unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        assert_eq!(runtime.host.opus_codec(), OpusCodec::Libopus);
        runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 90 } }))
            .expect("a save of another field succeeds");
        let text = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
        assert!(text.contains("\"opus_codec\": \"libopus\""), "{text}");
        runtime.shutdown();
    }

    #[test]
    fn saving_a_patch_validates_persists_and_reports_the_changed_sections() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        let mut events = runtime.events().subscribe();
        let saved = runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 120 } }))
            .unwrap();
        assert_eq!(saved.session.idle_timeout_seconds, 120);
        assert_eq!(runtime.settings().session.idle_timeout_seconds, 120);
        let on_disk = SettingsStore::new(dir.path().join("settings.json"))
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(on_disk.session.idle_timeout_seconds, 120);
        // The session section changed, so the wake manager reports its state first. With
        // no Vosk files in the scratch folder that is the download it waits for, and its
        // log line.
        let mut saw_settings_changed = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::SettingsChanged => saw_settings_changed = true,
                AppEvent::Wake(status) => assert!(
                    matches!(
                        status.state,
                        crate::wake::WakeState::Off
                            | crate::wake::WakeState::NoPhrases
                            | crate::wake::WakeState::NeedsDownload { .. }
                    ),
                    "{:?}",
                    status.state
                ),
                AppEvent::Log { line } => assert!(line.starts_with("wake word:"), "{line}"),
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(saw_settings_changed);

        let error = runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 1 } }))
            .unwrap_err();
        assert!(error.to_string().contains("session.idle_timeout_seconds"));
        assert_eq!(
            runtime.settings().session.idle_timeout_seconds,
            120,
            "rejected save changes nothing"
        );
    }

    #[test]
    fn mute_is_one_state_with_an_event_per_change() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        let mut events = runtime.events().subscribe();
        assert!(runtime.toggle_mute());
        assert!(!runtime.toggle_mute());
        runtime.set_muted(false);
        assert!(matches!(
            events.try_recv(),
            Ok(AppEvent::MuteChanged { muted: true })
        ));
        assert!(matches!(
            events.try_recv(),
            Ok(AppEvent::MuteChanged { muted: false })
        ));
        assert!(
            events.try_recv().is_err(),
            "no event when the state did not change"
        );
    }

    /// Change `claude-and-opencode-engines`, D9: the account a link is bound to comes with
    /// its engine; OpenCode links without one, a Claude Code without a sign-in cannot link.
    #[test]
    fn the_linked_account_comes_with_its_engine() {
        use crate::assistant::AssistantStatus;
        let opencode = AssistantStatus {
            engine: "opencode".to_owned(),
            ..AssistantStatus::default()
        };
        assert_eq!(
            signed_in_account(&opencode),
            Some(("opencode".to_owned(), String::new()))
        );
        let claude_out = AssistantStatus {
            engine: "claude".to_owned(),
            ..AssistantStatus::default()
        };
        assert_eq!(signed_in_account(&claude_out), None);
        let claude_in = AssistantStatus {
            engine: "claude".to_owned(),
            signed_in: true,
            account_type: Some("claude.ai".to_owned()),
            email: Some("me@example.com".to_owned()),
            ..AssistantStatus::default()
        };
        let (engine, fingerprint) = signed_in_account(&claude_in).unwrap();
        assert_eq!(engine, "claude");
        assert_eq!(
            fingerprint,
            crate::owner::account_fingerprint(Some("claude.ai"), Some("me@example.com"))
        );
    }

    /// An engine whose preparation always fails, as a Claude Code that is not signed in.
    struct RefusingEngine(StubAssistant);

    impl Assistant for RefusingEngine {
        fn id(&self) -> &'static str {
            "refusing"
        }
        fn prepare(&self, _guild: u64) -> crate::assistant::PrepareFuture<'_> {
            Box::pin(async {
                Err(crate::assistant::AssistantError::Unavailable(
                    "not signed in".to_owned(),
                ))
            })
        }
        fn attach(
            &self,
            guild: u64,
            ports: crate::audio::AudioPorts,
        ) -> Result<Arc<dyn crate::assistant::AssistantSession>, crate::assistant::AssistantError>
        {
            self.0.attach(guild, ports)
        }
        fn detach(&self, guild: u64) {
            self.0.detach(guild)
        }
        fn capabilities(&self) -> crate::assistant::Capabilities {
            self.0.capabilities()
        }
        fn status(&self) -> crate::assistant::AssistantStatus {
            self.0.status()
        }
    }

    /// Requirement "The engine switches at once" (change `claude-and-opencode-engines`,
    /// task 4.1): the bot stays in the channel, the old engine's session is detached and
    /// the new engine's attached; an engine that cannot start leaves the channel without a
    /// session and with its error.
    #[test]
    fn switching_the_engine_in_a_channel_keeps_the_bot_and_moves_the_session() {
        use crate::assistant::switch::EngineSwitch;
        use crate::settings::AssistantEngine;
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let codex = Arc::new(StubAssistant::new(Duration::ZERO));
        let claude = Arc::new(StubAssistant::new(Duration::ZERO));
        let opencode = Arc::new(RefusingEngine(StubAssistant::new(Duration::ZERO)));
        let switch = EngineSwitch::new(
            vec![
                (
                    AssistantEngine::Codex,
                    Arc::clone(&codex) as Arc<dyn Assistant>,
                ),
                (
                    AssistantEngine::Claude,
                    Arc::clone(&claude) as Arc<dyn Assistant>,
                ),
                (
                    AssistantEngine::Opencode,
                    Arc::clone(&opencode) as Arc<dyn Assistant>,
                ),
            ],
            AssistantEngine::Codex,
        );
        let errors = Arc::new(ConnectionErrors::new(EventBus::new(16)));
        let runtime = Runtime::with_assistant(
            "0.1.0+test",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            EventBus::new(64),
            tokio_rt.handle().clone(),
            errors,
            Arc::new(switch),
        );
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always" } }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output);
        assert!(codex.session(42).is_some());

        runtime
            .save_settings(&json!({ "assistant": { "engine": "claude", "mode": "whisper" } }))
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(runtime.host.registry.get(42).is_some(), "the bot stays");
        assert!(codex.session(42).is_none(), "Codex's session ended");
        assert!(
            claude.session(42).is_some(),
            "Claude Code's session attached"
        );

        runtime
            .save_settings(&json!({ "assistant": { "engine": "opencode" } }))
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(runtime.host.registry.get(42).is_some(), "the bot stays");
        assert!(claude.session(42).is_none());
        assert!(opencode.0.session(42).is_none(), "nothing attached");
        let error = runtime
            .status()
            .connection_errors
            .get("assistant")
            .cloned()
            .unwrap_or_default();
        assert!(
            error.contains("OpenCode could not start") && error.contains("not signed in"),
            "{error}"
        );
        runtime.shutdown();
    }

    /// Change `realtime-switches-to-codex`, scenario "Claude Code in the Whisper mode back to
    /// the realtime voice in a channel": one save sets Codex and the realtime mode; the bot
    /// stays, Claude Code's session is detached and Codex's attached in the realtime mode.
    #[test]
    fn one_save_back_to_codex_realtime_keeps_the_bot_and_moves_the_session() {
        use crate::assistant::switch::EngineSwitch;
        use crate::settings::{AssistantEngine, AssistantMode};
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let codex = Arc::new(StubAssistant::new(Duration::ZERO));
        let claude = Arc::new(StubAssistant::new(Duration::ZERO));
        let opencode = Arc::new(StubAssistant::new(Duration::ZERO));
        let switch = EngineSwitch::new(
            vec![
                (
                    AssistantEngine::Codex,
                    Arc::clone(&codex) as Arc<dyn Assistant>,
                ),
                (
                    AssistantEngine::Claude,
                    Arc::clone(&claude) as Arc<dyn Assistant>,
                ),
                (
                    AssistantEngine::Opencode,
                    Arc::clone(&opencode) as Arc<dyn Assistant>,
                ),
            ],
            AssistantEngine::Codex,
        );
        let errors = Arc::new(ConnectionErrors::new(EventBus::new(16)));
        let runtime = Runtime::with_assistant(
            "0.1.0+test",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            EventBus::new(64),
            tokio_rt.handle().clone(),
            errors,
            Arc::new(switch),
        );
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({
                "session": { "mode": "always" },
                "assistant": { "engine": "claude", "mode": "whisper" }
            }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output);
        assert!(claude.session(42).is_some(), "Claude Code is attached");

        runtime
            .save_settings(&json!({ "assistant": { "engine": "codex", "mode": "realtime" } }))
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(runtime.host.registry.get(42).is_some(), "the bot stays");
        assert!(claude.session(42).is_none(), "Claude Code's session ended");
        assert!(codex.session(42).is_some(), "Codex's session attached");
        let settings = runtime.settings();
        assert_eq!(settings.assistant.engine, AssistantEngine::Codex);
        assert_eq!(settings.assistant.mode, AssistantMode::Realtime);
        runtime.shutdown();
    }

    /// Scenario "Switched to Whisper while in a channel" (change `whisper-mode`, task 8.2):
    /// the session ends, the assistant is attached again on a new gate on the same output,
    /// the bot stays, Vosk is off, and the missing Whisper files wait for the window.
    #[test]
    fn switching_the_mode_in_a_channel_replaces_the_session_and_keeps_the_bot() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always" } }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output.clone());
        let old = runtime.host.registry.get(42).unwrap();
        assert_eq!(host.assistant_mode(), crate::discord::MODE_REALTIME);

        let changed = runtime
            .save_settings(&json!({ "assistant": { "mode": "whisper" } }))
            .unwrap();
        assert_eq!(
            changed.assistant.mode,
            crate::settings::AssistantMode::Whisper
        );
        let new = runtime
            .host
            .registry
            .get(42)
            .expect("the bot stays in the channel");
        assert!(!Arc::ptr_eq(&old, &new), "a new gate in the new mode");
        assert_eq!(host.assistant_mode(), crate::discord::MODE_WHISPER);
        let status = runtime.status();
        assert_eq!(status.assistant.calls[0].guild_id, "42", "attached again");
        assert_eq!(
            status.wake.state,
            crate::wake::WakeState::Off,
            "Vosk is off"
        );
        assert!(
            matches!(
                status.whisper.state,
                crate::stt::manager::WhisperState::NeedsDownload { .. }
            ),
            "{:?}",
            status.whisper.state
        );
        assert!(matches!(
            status.tts.state,
            crate::tts::manager::TtsState::NeedsDownload { .. }
        ));

        runtime
            .save_settings(&json!({ "assistant": { "mode": "realtime" } }))
            .unwrap();
        assert_eq!(host.assistant_mode(), crate::discord::MODE_REALTIME);
        assert_eq!(
            runtime.status().whisper.state,
            crate::stt::manager::WhisperState::Off
        );
        assert_eq!(runtime.host.registry.all().len(), 1);
        host.on_leave(42);
        runtime.shutdown();
    }

    /// Requirement "Join prepares the text answers mode" (change `text-answers-mode`): the
    /// mode is named as such, a join without Whisper's files is refused with the reply that
    /// names the Whisper card only, and the voice is neither needed nor loaded.
    #[test]
    fn a_join_in_the_text_answers_mode_needs_whisper_only() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "assistant": { "mode": "text" } }))
            .unwrap();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        assert_eq!(host.assistant_mode(), crate::discord::MODE_TEXT);
        let refused = tokio_rt.block_on(host.prepare_join(42));
        assert_eq!(
            refused,
            Err(crate::stt::manager::JOIN_NEEDS_WHISPER_FILES.to_owned())
        );
        assert!(refused
            .unwrap_err()
            .starts_with("The Whisper mode needs downloads"));
        let status = runtime.status();
        assert!(matches!(
            status.whisper.state,
            crate::stt::manager::WhisperState::NeedsDownload { .. }
        ));
        assert_eq!(
            status.tts.state,
            crate::tts::manager::TtsState::Off,
            "the voice is not asked for"
        );
        assert_eq!(
            status.wake.state,
            crate::wake::WakeState::Off,
            "Vosk is off"
        );
        runtime.shutdown();
    }

    /// Scenario "Whisper files missing" (change `models-in-settings`): the join is refused
    /// with the reply that sends the user to the Settings tab, and Vosk is not asked for.
    #[test]
    fn a_join_in_the_whisper_mode_without_its_files_is_refused() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "assistant": { "mode": "whisper" } }))
            .unwrap();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        let refused = tokio_rt.block_on(host.prepare_join(42));
        assert_eq!(
            refused,
            Err(crate::stt::manager::JOIN_NEEDS_DOWNLOADS.to_owned())
        );
        assert!(
            refused.unwrap_err().contains("Settings"),
            "the reply points to the Settings tab"
        );
        assert!(
            !dir.path().join("assets").join("whisper-cpu-1.9.5").exists(),
            "nothing was downloaded"
        );
        runtime
            .save_settings(&json!({
                "assistant": { "mode": "realtime" },
                "session": { "mode": "always" }
            }))
            .unwrap();
        assert_eq!(tokio_rt.block_on(host.prepare_join(42)), Ok(()));
        runtime.shutdown();
    }

    /// Requirement "A join needs the wake word's files" (change `models-in-settings`): in
    /// the realtime mode with the wake word a scratch folder without Vosk refuses the join
    /// before the assistant starts, names the files and downloads nothing; always
    /// listening joins.
    #[test]
    fn a_wake_word_join_without_the_vosk_files_is_refused() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({
                "assistant": { "mode": "realtime" },
                "session": { "mode": "wake_word" }
            }))
            .unwrap();
        let mut events = runtime.events().subscribe();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        let reason = tokio_rt
            .block_on(host.prepare_join(42))
            .expect_err("the join is refused");
        assert!(
            reason.starts_with(crate::wake::JOIN_NEEDS_WAKE_FILES),
            "{reason}"
        );
        assert!(reason.contains("Vosk library"), "{reason}");
        assert!(reason.contains("Download under Wake model"), "{reason}");
        assert!(reason.ends_with("then run /join again"), "{reason}");
        let mut logged = false;
        while let Ok(event) = events.try_recv() {
            if let AppEvent::Log { line } = event {
                logged |= line == format!("join to guild 42 refused: {reason}");
            }
        }
        assert!(logged, "the refusal is logged for the window");
        assert!(
            !dir.path().join("assets").join("vosk-win64-0.3.45").exists(),
            "nothing was downloaded"
        );
        runtime
            .save_settings(&json!({ "session": { "mode": "always" } }))
            .unwrap();
        assert_eq!(tokio_rt.block_on(host.prepare_join(42)), Ok(()));
        runtime.shutdown();
    }

    #[test]
    fn joining_builds_a_gate_attached_to_the_assistant_and_leaving_removes_it() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always" } }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output.clone());
        let status = runtime.status();
        assert!(status.gates["42"].active, "always mode: active at once");
        assert!(status.gates["42"].session_wanted);
        assert_eq!(status.assistant.calls[0].guild_id, "42");
        assert_eq!(host.assistant_state(42), AssistantState::Listening);
        assert_eq!(host.assistant_state(43), AssistantState::Off);

        runtime.set_muted(true);
        assert_eq!(host.assistant_state(42), AssistantState::Muted);
        assert!(output.is_muted());
        runtime.set_muted(false);

        runtime
            .save_settings(&json!({ "session": { "mode": "wake_word" } }))
            .unwrap();
        assert_eq!(host.assistant_state(42), AssistantState::WaitingForWakeWord);
        assert!(
            output.is_muted(),
            "waiting for the wake word keeps the output silent"
        );

        host.on_leave(42);
        assert!(runtime.status().gates.is_empty());
        assert!(runtime.status().assistant.calls.is_empty());
        assert_eq!(host.assistant_state(42), AssistantState::Off);
        runtime.shutdown();
    }

    #[test]
    fn a_saved_audio_section_reaches_the_next_frames_without_rebuilding() {
        use crate::audio::FRAME_SAMPLES;
        use std::io::Read;
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always", "earcons": false } }))
            .unwrap();
        let (output, mut source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output.clone());
        let gate = runtime.host.registry.get(42).unwrap();

        runtime
            .save_settings(&json!({ "audio": { "input_gain": 4.0, "output_gain": 2.5 } }))
            .unwrap();
        assert!(
            Arc::ptr_eq(&gate, &runtime.host.registry.get(42).unwrap()),
            "the gate is not rebuilt"
        );

        let loud = Frame::from_slice(&[10_000; FRAME_SAMPLES]);
        host.on_tick(42, &loud, &[(7, loud.as_slice())]);
        let audio = runtime.status().audio;
        assert_eq!((audio.input.gain, audio.output.gain), (4.0, 2.5));
        assert_eq!(
            audio.input.clips, FRAME_SAMPLES as u64,
            "the next frame sent used the new input gain"
        );

        for _ in 0..3 {
            output.push(Frame::from_slice(&[1_000; FRAME_SAMPLES]));
        }
        let mut bytes = vec![0u8; FRAME_SAMPLES * 4];
        source.read_exact(&mut bytes).unwrap();
        let first = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert!(
            (first - 2_500.0 / 32_768.0).abs() < 1e-6,
            "the next frame played used the new output gain: {first}"
        );
        assert_eq!(runtime.status().audio.output.clips, 0);
        runtime.shutdown();
    }

    #[test]
    fn a_turn_is_measured_from_the_speech_sent_to_the_answer_played() {
        use crate::audio::FRAME_SAMPLES;
        use std::io::Read;
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always", "earcons": false } }))
            .unwrap();
        let (output, mut source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output.clone());
        let speech = Frame::from_slice(&[5_000; FRAME_SAMPLES]);
        for _ in 0..10 {
            host.on_tick(42, &speech, &[(7, speech.as_slice())]);
        }
        std::thread::sleep(Duration::from_millis(30));
        for _ in 0..3 {
            output.push(Frame::from_slice(&[3_000; FRAME_SAMPLES]));
        }
        let mut bytes = vec![0u8; FRAME_SAMPLES * 4];
        source.read_exact(&mut bytes).unwrap();
        let turns = runtime.status().latency["42"].clone();
        assert_eq!(turns.len(), 1, "{turns:?}");
        assert_eq!(turns[0].turn, Some(1));
        assert_eq!(turns[0].kind, "response");
        assert!(turns[0].first_speech_to_playback_ms.unwrap() >= 30);
        runtime.shutdown();
    }

    #[test]
    fn nothing_of_codex_runs_after_start() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let events = EventBus::new(64);
        let errors = Arc::new(ConnectionErrors::new(events.clone()));
        let mut settings = Settings::default();
        // A path that exists: the engine would find it if it tried to start.
        let fake = dir.path().join("codex.exe");
        std::fs::write(&fake, b"").unwrap();
        settings.codex.executable = fake.display().to_string();
        let codex = crate::assistant::codex::CodexAssistant::new(
            tokio_rt.handle().clone(),
            events.clone(),
            Arc::clone(&errors),
            dir.path(),
            &settings,
        );
        let runtime = Runtime::with_assistant(
            "0.1.0+test 2026-11-10",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            events,
            tokio_rt.handle().clone(),
            errors,
            Arc::new(codex),
        );
        runtime.start().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let assistant = runtime.status().assistant;
        assert!(!assistant.running, "{}", assistant.summary);
        assert!(assistant.summary.contains("/join"), "{}", assistant.summary);
        runtime.shutdown();
    }

    #[test]
    fn hotkey_status_is_kept_and_published() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        let mut events = runtime.events().subscribe();
        runtime.set_hotkey_status(HotkeyStatus {
            shortcut: "Ctrl+Alt+M".to_owned(),
            registered: false,
            error: Some("taken".to_owned()),
        });
        assert_eq!(runtime.status().hotkey.error.as_deref(), Some("taken"));
        let mut saw = false;
        while let Ok(event) = events.try_recv() {
            if let AppEvent::Hotkey(status) = event {
                saw = !status.registered;
            }
        }
        assert!(saw);
    }

    #[test]
    fn connect_without_a_token_is_refused() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        assert!(matches!(
            runtime.connect(),
            Err(RuntimeError::Discord(DiscordError::NoToken))
        ));
    }

    #[test]
    fn unknown_secret_names_are_rejected() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        assert!(matches!(
            runtime.set_secret("password", "x"),
            Err(RuntimeError::UnknownSecret(_))
        ));
    }
}
