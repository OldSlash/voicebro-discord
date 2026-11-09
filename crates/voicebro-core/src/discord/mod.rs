//! The Discord adapter: serenity for the gateway and slash commands, songbird for voice.
//!
//! One `DiscordRuntime` per app. `start` spawns the client; `/join` puts the bot into the
//! caller's voice channel with the speaking handler registered before the join (Discord
//! announces a user's SSRC only the first time they speak), `/leave` removes the call so
//! no handler or track survives. The output is one silent track per join until the audio
//! core supplies real frames.

mod output;
mod speakers;

use crate::events::{AppEvent, EventBus};
use serde::Serialize;
use serenity::all::{
    ChannelId, Command, CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage, EventHandler, GatewayIntents, GuildId, Interaction, Ready,
    ShardManager, UserId, VoiceState,
};
use serenity::async_trait;
use songbird::driver::{Channels, DecodeConfig, DecodeMode, SampleRate};
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};
use songbird::input::RawAdapter;
use songbird::{CoreEvent, SerenityInit, Songbird};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

pub use output::{OutputSource, SilenceSource};
pub use speakers::{clean_display_name, SpeakerDirectory};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// What the UI shows about Discord. Measured values only.
#[derive(Debug, Clone, Serialize)]
pub struct DiscordStatus {
    pub state: ConnectionState,
    pub user_name: Option<String>,
    pub guild_count: usize,
    /// Guild id → voice channel name the bot is in.
    pub voice_channels: BTreeMap<String, String>,
    pub last_error: Option<String>,
    /// Voice ticks received since the last join, over all guilds.
    pub voice_ticks: u64,
}

impl Default for DiscordStatus {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            user_name: None,
            guild_count: 0,
            voice_channels: BTreeMap::new(),
            last_error: None,
            voice_ticks: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscordError {
    #[error("Discord is already connecting or connected")]
    AlreadyRunning,
    #[error("no Discord token is stored")]
    NoToken,
    #[error("building the Discord client: {0}")]
    Client(String),
}

struct Shared {
    status: Mutex<DiscordStatus>,
    events: EventBus,
    ticks: AtomicU64,
    speakers: Mutex<BTreeMap<u64, Arc<SpeakerDirectory>>>,
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut DiscordStatus)) {
        let snapshot = {
            let mut status = self.status.lock().expect("discord status lock");
            change(&mut status);
            status.voice_ticks = self.ticks.load(Ordering::Relaxed);
            status.clone()
        };
        self.events.publish(AppEvent::Discord(snapshot));
    }

    fn directory(&self, guild: GuildId) -> Arc<SpeakerDirectory> {
        let mut all = self.speakers.lock().expect("speakers lock");
        Arc::clone(all.entry(guild.get()).or_default())
    }
}

/// Owns the serenity client task. Clone-cheap through `Arc`.
pub struct DiscordRuntime {
    shared: Arc<Shared>,
    task: Mutex<Option<JoinHandle<()>>>,
    shard_manager: Mutex<Option<Arc<ShardManager>>>,
    handle: Handle,
}

impl DiscordRuntime {
    pub fn new(events: EventBus, handle: Handle) -> Self {
        Self {
            shared: Arc::new(Shared {
                status: Mutex::new(DiscordStatus::default()),
                events,
                ticks: AtomicU64::new(0),
                speakers: Mutex::new(BTreeMap::new()),
            }),
            task: Mutex::new(None),
            shard_manager: Mutex::new(None),
            handle,
        }
    }

    pub fn status(&self) -> DiscordStatus {
        let mut status = self.shared.status.lock().expect("discord status lock").clone();
        status.voice_ticks = self.shared.ticks.load(Ordering::Relaxed);
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
    /// appear at once; without it they are registered globally (Discord may take up to an
    /// hour to show those).
    pub fn start(&self, token: String, guild_id: Option<GuildId>) -> Result<(), DiscordError> {
        if self.is_running() {
            return Err(DiscordError::AlreadyRunning);
        }
        if token.trim().is_empty() {
            return Err(DiscordError::NoToken);
        }
        self.shared.update(|status| {
            status.state = ConnectionState::Connecting;
            status.last_error = None;
            status.voice_channels.clear();
        });
        let shared = Arc::clone(&self.shared);
        let shard_slot = Arc::new(Mutex::new(None::<Arc<ShardManager>>));
        {
            let mut slot = self.shard_manager.lock().expect("shard manager lock");
            *slot = None;
        }
        let shard_manager_out = Arc::clone(&shard_slot);
        let events = self.shared.events.clone();
        let task = self.handle.spawn(async move {
            let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES;
            let songbird_config = songbird::Config::default().decode_mode(DecodeMode::Decode(
                DecodeConfig::new(Channels::Stereo, SampleRate::Hz48000),
            ));
            let handler = Handler {
                shared: Arc::clone(&shared),
                guild_id,
            };
            let client = serenity::Client::builder(&token, intents)
                .event_handler(handler)
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
            *shard_manager_out.lock().expect("shard slot lock") = Some(client.shard_manager.clone());
            events.log("Discord: connecting");
            if let Err(error) = client.start().await {
                shared.update(|status| {
                    status.state = ConnectionState::Error;
                    status.last_error = Some(error.to_string());
                    status.voice_channels.clear();
                });
                log::error!("Discord client stopped: {error}");
            } else {
                shared.update(|status| {
                    status.state = ConnectionState::Disconnected;
                    status.voice_channels.clear();
                });
            }
        });
        *self.task.lock().expect("discord task lock") = Some(task);
        // The shard manager becomes available once the client is built; keep the slot.
        *self.shard_manager.lock().expect("shard manager lock") = None;
        self.pending_shard_slot(shard_slot);
        Ok(())
    }

    fn pending_shard_slot(&self, slot: Arc<Mutex<Option<Arc<ShardManager>>>>) {
        // Poll the slot from a small task so `stop` can find the shard manager later.
        let shared = Arc::clone(&self.shared);
        let target = Arc::new(Mutex::new(None));
        let _ = shared;
        let target_clone = Arc::clone(&target);
        self.handle.spawn(async move {
            for _ in 0..600 {
                if let Some(manager) = slot.lock().expect("shard slot lock").clone() {
                    *target_clone.lock().expect("target lock") = Some(manager);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
        *self.shard_manager.lock().expect("shard manager lock") = None;
        // Store the slot itself so `stop` can read whatever has arrived by then.
        *self.shard_slot.lock().expect("slot lock") = Some(target);
    }

    /// Disconnects and waits for the client task to end.
    pub async fn stop(&self) {
        let manager = self
            .shard_slot
            .lock()
            .expect("slot lock")
            .as_ref()
            .and_then(|slot| slot.lock().expect("target lock").clone());
        if let Some(manager) = manager {
            manager.shutdown_all().await;
        }
        let task = self.task.lock().expect("discord task lock").take();
        if let Some(task) = task {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(10), task).await;
        }
        self.shared.update(|status| {
            status.state = ConnectionState::Disconnected;
            status.voice_channels.clear();
        });
        self.shared.events.log("Discord: disconnected");
    }
}
