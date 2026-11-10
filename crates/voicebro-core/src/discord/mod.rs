//! The Discord adapter: serenity for the gateway and slash commands, songbird for voice.
//!
//! One `DiscordRuntime` per app. `start` spawns the client; `/join` puts the bot into the
//! caller's voice channel with the speaking handler registered before the join (Discord
//! announces a user's SSRC only the first time they speak); `/leave` removes the call so no
//! handler or track survives. The receive path mixes the speakers and hands every tick to
//! the [`VoiceHost`] the app provides (the session gate).

mod handler;
mod panel;
mod receive;
mod session;
mod speakers;

use crate::audio::{Frame, OutputCounters, OutputHandle};
use crate::events::{AppEvent, EventBus};
use serde::Serialize;
use serenity::all::{Cache, GatewayIntents, GuildId, Http, ShardManager};
use songbird::driver::{Channels, DecodeConfig, DecodeMode, SampleRate};
use songbird::{SerenityInit, Songbird};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

pub use panel::{mute_button_id, parse_mute_button_id};
pub use speakers::{clean_display_name, SpeakerDirectory};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// What the assistant is doing in a guild, for the panel and `/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantState {
    /// No voice connection in this guild.
    Off,
    WaitingForWakeWord,
    Listening,
    Muted,
}

impl AssistantState {
    pub fn label(self) -> &'static str {
        match self {
            AssistantState::Off => "off",
            AssistantState::WaitingForWakeWord => "waiting for the wake word",
            AssistantState::Listening => "listening",
            AssistantState::Muted => "muted",
        }
    }
}

/// Counters the receive path keeps. Numbers only, read into the status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct VoiceCounters {
    pub ticks: u64,
    pub speaker_frames: u64,
    pub malformed_frames: u64,
    pub unnamed_sources: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceChannelStatus {
    pub guild_name: String,
    pub channel_name: String,
    pub participants: Vec<String>,
    pub output: OutputCounters,
}

/// What the UI shows about Discord. Measured values only.
#[derive(Debug, Clone, Serialize)]
pub struct DiscordStatus {
    pub state: ConnectionState,
    pub user_name: Option<String>,
    pub guild_count: usize,
    /// Guild id → the voice channel the bot is in.
    pub voice_channels: BTreeMap<String, VoiceChannelStatus>,
    pub last_error: Option<String>,
    pub counters: VoiceCounters,
}

impl Default for DiscordStatus {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            user_name: None,
            guild_count: 0,
            voice_channels: BTreeMap::new(),
            last_error: None,
            counters: VoiceCounters::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscordError {
    #[error("Discord is already connecting or connected")]
    AlreadyRunning,
    #[error("no Discord token is stored")]
    NoToken,
}

/// What [`VoiceHost::prepare_join`] returns: an error is the reason the join is refused.
pub type JoinPreparation<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>;

/// Everything the adapter needs from the rest of the app. The runtime implements it with
/// the session gates and the assistant.
pub trait VoiceHost: Send + Sync {
    /// Runs before the bot joins a channel of `guild` (the assistant starts its process and
    /// checks its sign-in); an error refuses the join and is shown to the caller.
    fn prepare_join(&self, _guild: u64) -> JoinPreparation<'_> {
        Box::pin(async { Ok(()) })
    }
    /// Every 20 ms while the bot is in a channel: the mix and each speaker's own frame.
    fn on_tick(&self, guild: u64, mixed: &Frame, speakers: &[(u32, &[i16])]);
    fn set_speaker_name(&self, guild: u64, ssrc: u32, name: &str);
    fn set_participants(&self, guild: u64, names: &[String]);
    /// The bot joined a channel; `output` is where the assistant's audio goes.
    fn on_join(&self, guild: u64, output: OutputHandle);
    fn on_leave(&self, guild: u64);
    fn assistant_state(&self, guild: u64) -> AssistantState;
    fn toggle_mute(&self) -> bool;
    fn is_muted(&self) -> bool;
}

/// State shared between the runtime handle, the serenity handler and the voice handlers.
pub(crate) struct Shared {
    pub(crate) events: EventBus,
    pub(crate) host: Arc<dyn VoiceHost>,
    pub(crate) status: Mutex<DiscordStatus>,
    pub(crate) guild_id: Mutex<Option<GuildId>>,
    pub(crate) http: Mutex<Option<Arc<Http>>>,
    pub(crate) cache: Mutex<Option<Arc<Cache>>>,
    pub(crate) songbird: Mutex<Option<Arc<Songbird>>>,
    pub(crate) sessions: Mutex<BTreeMap<u64, session::VoiceSession>>,
    pub(crate) join_locks: Mutex<BTreeMap<u64, Arc<tokio::sync::Mutex<()>>>>,
    pub(crate) speakers: Mutex<BTreeMap<u64, Arc<SpeakerDirectory>>>,
    pub(crate) panels_dirty: AtomicBool,
    pub(crate) ticks: AtomicU64,
    pub(crate) speaker_frames: AtomicU64,
    pub(crate) malformed_frames: AtomicU64,
    pub(crate) unnamed_sources: AtomicU64,
}

impl Shared {
    pub(crate) fn update(&self, change: impl FnOnce(&mut DiscordStatus)) {
        let snapshot = {
            let mut status = self.status.lock().expect("discord status lock");
            change(&mut status);
            status.counters = self.counters();
            status.clone()
        };
        self.events.publish(AppEvent::Discord(Box::new(snapshot)));
    }

    pub(crate) fn counters(&self) -> VoiceCounters {
        VoiceCounters {
            ticks: self.ticks.load(Ordering::Relaxed),
            speaker_frames: self.speaker_frames.load(Ordering::Relaxed),
            malformed_frames: self.malformed_frames.load(Ordering::Relaxed),
            unnamed_sources: self.unnamed_sources.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn directory(&self, guild: u64) -> Arc<SpeakerDirectory> {
        let mut all = self.speakers.lock().expect("speakers lock");
        Arc::clone(all.entry(guild).or_default())
    }

    pub(crate) fn join_lock(&self, guild: u64) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.join_locks.lock().expect("join locks");
        Arc::clone(locks.entry(guild).or_default())
    }

    pub(crate) fn http(&self) -> Option<Arc<Http>> {
        self.http.lock().expect("http lock").clone()
    }

    pub(crate) fn cache(&self) -> Option<Arc<Cache>> {
        self.cache.lock().expect("cache lock").clone()
    }

    pub(crate) fn songbird(&self) -> Option<Arc<Songbird>> {
        self.songbird.lock().expect("songbird lock").clone()
    }

    pub(crate) fn log(&self, line: impl Into<String>) {
        self.events.log(line);
    }
}

/// Only what names and voice channels need: guilds with their channels, voice states and
/// members stay (they live inside each cached guild); no message cache, no global channel
/// map (`cache.channel(id)` is never used) and no global user map (names come from guild
/// members, then HTTP). Field names checked in the serenity 0.12.5 source.
fn cache_settings() -> serenity::cache::Settings {
    let mut settings = serenity::cache::Settings::default();
    settings.max_messages = 0;
    settings.cache_guilds = true;
    settings.cache_channels = false;
    settings.cache_users = false;
    settings
}

/// Owns the serenity client task.
pub struct DiscordRuntime {
    shared: Arc<Shared>,
    task: Mutex<Option<JoinHandle<()>>>,
    shard_manager: Arc<Mutex<Option<Arc<ShardManager>>>>,
    handle: Handle,
}

impl DiscordRuntime {
    pub fn new(events: EventBus, host: Arc<dyn VoiceHost>, handle: Handle) -> Self {
        Self {
            shared: Arc::new(Shared {
                events,
                host,
                status: Mutex::new(DiscordStatus::default()),
                guild_id: Mutex::new(None),
                http: Mutex::new(None),
                cache: Mutex::new(None),
                songbird: Mutex::new(None),
                sessions: Mutex::new(BTreeMap::new()),
                join_locks: Mutex::new(BTreeMap::new()),
                speakers: Mutex::new(BTreeMap::new()),
                panels_dirty: AtomicBool::new(false),
                ticks: AtomicU64::new(0),
                speaker_frames: AtomicU64::new(0),
                malformed_frames: AtomicU64::new(0),
                unnamed_sources: AtomicU64::new(0),
            }),
            task: Mutex::new(None),
            shard_manager: Arc::new(Mutex::new(None)),
            handle,
        }
    }

    pub fn status(&self) -> DiscordStatus {
        let mut status = self
            .shared
            .status
            .lock()
            .expect("discord status lock")
            .clone();
        status.counters = self.shared.counters();
        for (guild, session) in self.shared.sessions.lock().expect("sessions lock").iter() {
            if let Some(entry) = status.voice_channels.get_mut(&guild.to_string()) {
                entry.output = session.output.counters();
                entry.participants = session.participants.clone();
            }
        }
        status
    }

    pub fn is_running(&self) -> bool {
        self.task
            .lock()
            .expect("discord task lock")
            .as_ref()
            .map(|task| !task.is_finished())
            .unwrap_or(false)
    }

    /// Connects with `token`. `guild_id` scopes the slash commands to one guild so they
    /// appear at once; without it they are registered globally.
    pub fn start(&self, token: String, guild_id: Option<GuildId>) -> Result<(), DiscordError> {
        if self.is_running() {
            return Err(DiscordError::AlreadyRunning);
        }
        if token.trim().is_empty() {
            return Err(DiscordError::NoToken);
        }
        *self.shared.guild_id.lock().expect("guild id lock") = guild_id;
        self.shared.update(|status| {
            status.state = ConnectionState::Connecting;
            status.last_error = None;
            status.voice_channels.clear();
        });
        let shared = Arc::clone(&self.shared);
        let shard_slot = Arc::clone(&self.shard_manager);
        let task = self.handle.spawn(async move {
            let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES;
            let songbird_config = songbird::Config::default().decode_mode(DecodeMode::Decode(
                DecodeConfig::new(Channels::Stereo, SampleRate::Hz48000),
            ));
            let client = serenity::Client::builder(token.trim(), intents)
                .cache_settings(cache_settings())
                .event_handler(handler::Handler {
                    shared: Arc::clone(&shared),
                })
                .register_songbird_from_config(songbird_config)
                .await;
            let mut client = match client {
                Ok(client) => client,
                Err(error) => {
                    shared.update(|status| {
                        status.state = ConnectionState::Error;
                        status.last_error = Some(format!("client: {error}"));
                    });
                    return;
                }
            };
            *shard_slot.lock().expect("shard manager lock") = Some(client.shard_manager.clone());
            shared.log("Discord: connecting");
            match client.start().await {
                Ok(()) => shared.update(|status| {
                    status.state = ConnectionState::Disconnected;
                    status.voice_channels.clear();
                }),
                Err(error) => {
                    log::error!("Discord client stopped: {error}");
                    shared.update(|status| {
                        status.state = ConnectionState::Error;
                        status.last_error = Some(error.to_string());
                        status.voice_channels.clear();
                    });
                }
            }
            shared.sessions.lock().expect("sessions lock").clear();
            *shared.songbird.lock().expect("songbird lock") = None;
        });
        *self.task.lock().expect("discord task lock") = Some(task);
        Ok(())
    }

    /// Disconnects and waits for the client task to end. Leaves every voice channel first.
    pub async fn stop(&self) {
        let guilds: Vec<u64> = self
            .shared
            .sessions
            .lock()
            .expect("sessions lock")
            .keys()
            .copied()
            .collect();
        for guild in guilds {
            session::leave_voice(&self.shared, GuildId::new(guild)).await;
        }
        let manager = self
            .shard_manager
            .lock()
            .expect("shard manager lock")
            .clone();
        if let Some(manager) = manager {
            manager.shutdown_all().await;
        }
        let task = self.task.lock().expect("discord task lock").take();
        if let Some(task) = task {
            let _ = tokio::time::timeout(Duration::from_secs(10), task).await;
        }
        *self.shard_manager.lock().expect("shard manager lock") = None;
        self.shared.update(|status| {
            status.state = ConnectionState::Disconnected;
            status.voice_channels.clear();
            status.user_name = None;
            status.guild_count = 0;
        });
        self.shared.log("Discord: disconnected");
    }

    /// Something the panel shows changed (mute, assistant state): refresh the panels soon.
    pub fn mark_panels_dirty(&self) {
        self.shared.panels_dirty.store(true, Ordering::Release);
    }

    /// Leaves the voice channel of `guild`, if any.
    pub async fn leave(&self, guild: u64) {
        session::leave_voice(&self.shared, GuildId::new(guild)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    struct NullHost(AtomicBool);

    impl VoiceHost for NullHost {
        fn on_tick(&self, _: u64, _: &Frame, _: &[(u32, &[i16])]) {}
        fn set_speaker_name(&self, _: u64, _: u32, _: &str) {}
        fn set_participants(&self, _: u64, _: &[String]) {}
        fn on_join(&self, _: u64, _: OutputHandle) {}
        fn on_leave(&self, _: u64) {}
        fn assistant_state(&self, _: u64) -> AssistantState {
            AssistantState::Off
        }
        fn toggle_mute(&self) -> bool {
            !self.0.fetch_xor(true, Ordering::AcqRel)
        }
        fn is_muted(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    #[test]
    fn start_without_a_token_is_refused_and_status_stays_disconnected() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let discord = DiscordRuntime::new(
            EventBus::new(8),
            Arc::new(NullHost(AtomicBool::new(false))),
            runtime.handle().clone(),
        );
        assert!(matches!(
            discord.start("  ".to_owned(), None),
            Err(DiscordError::NoToken)
        ));
        assert_eq!(discord.status().state, ConnectionState::Disconnected);
        assert!(!discord.is_running());
    }
}
