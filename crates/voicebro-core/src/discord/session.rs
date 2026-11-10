//! One voice connection per guild: join in the fixed order, leave by removing everything.

use super::panel::{self, PanelRef, PanelView};
use super::receive::ReceiveHandler;
use super::{ConnectionState, Shared, VoiceChannelStatus};
use crate::audio::{output_pair, OutputHandle, CHANNELS, SAMPLE_RATE};
use serenity::all::{ChannelId, GuildId};
use serenity::async_trait;
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};
use songbird::input::RawAdapter;
use songbird::tracks::{PlayMode, TrackHandle};
use songbird::CoreEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

pub(crate) struct VoiceSession {
    pub(crate) channel: ChannelId,
    pub(crate) channel_name: String,
    pub(crate) participants: Vec<String>,
    pub(crate) output: OutputHandle,
    pub(crate) track: TrackHandle,
    pub(crate) track_ended: Arc<Notify>,
    pub(crate) panel: Option<PanelRef>,
}

struct TrackLogger {
    shared: Arc<Shared>,
    guild: GuildId,
    ended: Arc<Notify>,
}

#[async_trait]
impl VoiceEventHandler for TrackLogger {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(tracks) = ctx {
            for (state, _) in tracks.iter() {
                match &state.playing {
                    PlayMode::End => {
                        self.shared
                            .log(format!("guild {}: output track ended", self.guild.get()));
                        self.ended.notify_waiters();
                    }
                    PlayMode::Errored(error) => {
                        self.shared.log(format!(
                            "guild {}: output track error: {error}",
                            self.guild.get()
                        ));
                        self.ended.notify_waiters();
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JoinOutcome {
    Joined {
        channel_name: String,
    },
    NotReady,
    /// The host's preparation failed (Codex missing or signed out); the bot did not join.
    Refused(String),
    Failed(String),
}

impl JoinOutcome {
    /// The reply for every outcome but a successful join (that one carries the panel).
    pub(crate) fn reply_text(&self) -> String {
        match self {
            JoinOutcome::Joined { channel_name } => format!("Joined {channel_name}."),
            JoinOutcome::NotReady => "Voice is not ready yet; try again in a moment.".to_owned(),
            JoinOutcome::Refused(reason) => format!("Not joined: {reason}"),
            JoinOutcome::Failed(error) => format!("Could not join: {error}"),
        }
    }
}

/// The `/join` flow: the host prepares first (the assistant starts its process and checks
/// the sign-in), then the bot joins. A refused preparation leaves the bot out of the channel
/// and the reason goes back to the caller.
pub(crate) async fn prepare_and_join(
    shared: &Arc<Shared>,
    guild: GuildId,
    channel: ChannelId,
) -> JoinOutcome {
    if let Err(reason) = shared.host.prepare_join(guild.get()).await {
        return JoinOutcome::Refused(reason);
    }
    join_voice(shared, guild, channel).await
}

/// Joins `channel` for `guild` with the fixed order: get or create the call, register the
/// speaking and tick handlers, join, attach the output track. The panel is posted by the
/// command handler afterwards. Serialized per guild.
pub(crate) async fn join_voice(
    shared: &Arc<Shared>,
    guild: GuildId,
    channel: ChannelId,
) -> JoinOutcome {
    let lock = shared.join_lock(guild.get());
    let _guard = lock.lock().await;
    let Some(manager) = shared.songbird() else {
        return JoinOutcome::NotReady;
    };
    // A rejoin inside the same guild starts clean.
    if shared
        .sessions
        .lock()
        .expect("sessions lock")
        .contains_key(&guild.get())
    {
        leave_voice_locked(shared, guild).await;
    }

    let channel_name = shared
        .cache()
        .and_then(|cache| {
            cache
                .guild(guild)
                .and_then(|g| g.channels.get(&channel).map(|c| c.name.clone()))
        })
        .unwrap_or_else(|| channel.get().to_string());

    let directory = shared.directory(guild.get());
    let call = manager.get_or_insert(guild);
    {
        let mut call = call.lock().await;
        call.remove_all_global_events();
        call.add_global_event(
            CoreEvent::SpeakingStateUpdate.into(),
            ReceiveHandler::new(Arc::clone(shared), guild, Arc::clone(&directory)),
        );
        call.add_global_event(
            CoreEvent::VoiceTick.into(),
            ReceiveHandler::new(Arc::clone(shared), guild, Arc::clone(&directory)),
        );
    }
    if let Err(error) = manager.join(guild, channel).await {
        let _ = manager.remove(guild).await;
        return JoinOutcome::Failed(format!("join: {error}"));
    }

    let (output, source) = output_pair();
    let input: songbird::input::Input =
        RawAdapter::new(source, SAMPLE_RATE, CHANNELS as u32).into();
    let track_ended = Arc::new(Notify::new());
    let track = {
        let mut call = call.lock().await;
        call.play_only_input(input)
    };
    for event in [TrackEvent::End, TrackEvent::Error] {
        let _ = track.add_event(
            Event::Track(event),
            TrackLogger {
                shared: Arc::clone(shared),
                guild,
                ended: Arc::clone(&track_ended),
            },
        );
    }

    // Preload the names learned in an earlier connection.
    for (ssrc, name) in directory.snapshot() {
        shared.host.set_speaker_name(guild.get(), ssrc, &name);
    }
    shared.host.on_join(guild.get(), output.clone());

    shared.sessions.lock().expect("sessions lock").insert(
        guild.get(),
        VoiceSession {
            channel,
            channel_name: channel_name.clone(),
            participants: Vec::new(),
            output,
            track,
            track_ended,
            panel: None,
        },
    );
    let guild_name = shared
        .cache()
        .and_then(|cache| cache.guild(guild).map(|g| g.name.clone()))
        .unwrap_or_else(|| guild.get().to_string());
    shared.update(|status| {
        status.voice_channels.insert(
            guild.get().to_string(),
            VoiceChannelStatus {
                guild_name,
                channel_name: channel_name.clone(),
                participants: Vec::new(),
                output: Default::default(),
            },
        );
    });
    shared.log(format!("joined voice channel {channel_name}"));
    refresh_participants(shared, guild).await;
    JoinOutcome::Joined { channel_name }
}

/// Leaves `guild`'s voice channel: closes the source, waits for the track to end, removes
/// the call so no handler survives, deletes the panel. Serialized per guild.
pub(crate) async fn leave_voice(shared: &Arc<Shared>, guild: GuildId) -> bool {
    let lock = shared.join_lock(guild.get());
    let _guard = lock.lock().await;
    leave_voice_locked(shared, guild).await
}

async fn leave_voice_locked(shared: &Arc<Shared>, guild: GuildId) -> bool {
    let session = shared
        .sessions
        .lock()
        .expect("sessions lock")
        .remove(&guild.get());
    let Some(session) = session else {
        return false;
    };
    session.output.close();
    let _ = tokio::time::timeout(Duration::from_millis(200), session.track_ended.notified()).await;
    let _ = session.track.stop();
    if let Some(manager) = shared.songbird() {
        if let Err(error) = manager.remove(guild).await {
            log::debug!("remove call for guild {}: {error}", guild.get());
        }
    }
    if let (Some(panel), Some(http)) = (session.panel, shared.http()) {
        let _ = panel.channel.delete_message(&http, panel.message).await;
    }
    shared.host.on_leave(guild.get());
    shared.update(|status| {
        status.voice_channels.remove(&guild.get().to_string());
    });
    shared.log(format!("left voice channel {}", session.channel_name));
    true
}

/// Recomputes who is in the bot's channel and tells the host.
pub(crate) async fn refresh_participants(shared: &Arc<Shared>, guild: GuildId) {
    let channel = shared
        .sessions
        .lock()
        .expect("sessions lock")
        .get(&guild.get())
        .map(|session| session.channel);
    let Some(channel) = channel else {
        return;
    };
    let (Some(http), Some(cache)) = (shared.http(), shared.cache()) else {
        return;
    };
    let names = super::speakers::channel_people(&http, &cache, guild, channel).await;
    {
        let mut sessions = shared.sessions.lock().expect("sessions lock");
        if let Some(session) = sessions.get_mut(&guild.get()) {
            if session.participants == names {
                return;
            }
            session.participants = names.clone();
        } else {
            return;
        }
    }
    shared.host.set_participants(guild.get(), &names);
    shared
        .panels_dirty
        .store(true, std::sync::atomic::Ordering::Release);
    shared.update(|status| {
        if let Some(entry) = status.voice_channels.get_mut(&guild.get().to_string()) {
            entry.participants = names.clone();
        }
    });
}

/// The panel's embed and components for the current state of `guild`.
pub(crate) fn panel_message(
    shared: &Shared,
    guild: GuildId,
) -> Option<(
    serenity::all::CreateEmbed,
    Vec<serenity::all::CreateActionRow>,
)> {
    let sessions = shared.sessions.lock().expect("sessions lock");
    let session = sessions.get(&guild.get())?;
    let view = PanelView {
        channel_name: &session.channel_name,
        participants: &session.participants,
        assistant: shared.host.assistant_state(guild.get()),
        muted: shared.host.is_muted(),
    };
    Some((panel::embed(&view), panel::components(guild, view.muted)))
}

pub(crate) fn remember_panel(shared: &Shared, guild: GuildId, panel: PanelRef) {
    if let Some(session) = shared
        .sessions
        .lock()
        .expect("sessions lock")
        .get_mut(&guild.get())
    {
        session.panel = Some(panel);
    }
}

pub(crate) fn is_connected(shared: &Shared) -> bool {
    shared.status.lock().expect("discord status lock").state == ConnectionState::Connected
}

pub(crate) fn session_channel_name(shared: &Shared, guild: GuildId) -> Option<String> {
    shared
        .sessions
        .lock()
        .expect("sessions lock")
        .get(&guild.get())
        .map(|session| session.channel_name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Frame;
    use crate::discord::{AssistantState, DiscordRuntime, JoinPreparation, VoiceHost};
    use crate::events::EventBus;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A host whose preparation fails or passes, counting the joins it is told about.
    struct PreparingHost {
        refuse: Option<String>,
        joins: AtomicUsize,
    }

    impl VoiceHost for PreparingHost {
        fn prepare_join(&self, _: u64) -> JoinPreparation<'_> {
            let result = match &self.refuse {
                Some(reason) => Err(reason.clone()),
                None => Ok(()),
            };
            Box::pin(async move { result })
        }
        fn on_tick(&self, _: u64, _: &Frame, _: &[(u32, &[i16])]) {}
        fn set_speaker_name(&self, _: u64, _: u32, _: &str) {}
        fn set_participants(&self, _: u64, _: &[String]) {}
        fn on_join(&self, _: u64, _: OutputHandle) {
            self.joins.fetch_add(1, Ordering::Relaxed);
        }
        fn on_leave(&self, _: u64) {}
        fn assistant_state(&self, _: u64) -> AssistantState {
            AssistantState::Off
        }
        fn toggle_mute(&self) -> bool {
            false
        }
        fn is_muted(&self) -> bool {
            false
        }
    }

    fn join_with(refuse: Option<&str>) -> (JoinOutcome, Arc<PreparingHost>, usize) {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let host = Arc::new(PreparingHost {
            refuse: refuse.map(str::to_owned),
            joins: AtomicUsize::new(0),
        });
        let discord = DiscordRuntime::new(
            EventBus::new(16),
            host.clone() as Arc<dyn VoiceHost>,
            tokio_rt.handle().clone(),
        );
        let outcome = tokio_rt.block_on(prepare_and_join(
            &discord.shared,
            GuildId::new(1),
            ChannelId::new(2),
        ));
        let sessions = discord.shared.sessions.lock().unwrap().len();
        (outcome, host, sessions)
    }

    #[test]
    fn a_refused_preparation_keeps_the_bot_out_and_tells_the_caller_why() {
        let reason = "Codex is not signed in: use Sign in on the VoiceBro Status tab";
        let (outcome, host, sessions) = join_with(Some(reason));
        assert_eq!(outcome, JoinOutcome::Refused(reason.to_owned()));
        assert!(outcome.reply_text().contains(reason));
        assert_eq!(host.joins.load(Ordering::Relaxed), 0, "no join happened");
        assert_eq!(sessions, 0);

        // A passing preparation goes on to the join (here voice is not connected yet).
        let (outcome, _, _) = join_with(None);
        assert_eq!(outcome, JoinOutcome::NotReady);
    }
}
