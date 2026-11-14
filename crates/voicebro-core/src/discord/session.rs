//! One voice connection per guild: join in the fixed order, leave by removing everything.

use super::delivery::{self, DeliveryHandler, DeliveryLeg};
use super::panel::{self, PanelRef, PanelView};
use super::receive::ReceiveHandler;
use super::{ConnectionState, Shared, VoiceChannelStatus};
use crate::audio::opus_out::{self, DeliveryShared};
use crate::audio::{output_pair, OutputHandle, CHANNELS, SAMPLE_RATE};
use serenity::all::{ChannelId, GuildId};
use serenity::async_trait;
use songbird::driver::{DecodeConfig, DecodeMode, SampleRate};
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};
use songbird::input::RawAdapter;
use songbird::tracks::{PlayMode, TrackHandle};
use songbird::CoreEvent;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

pub(crate) struct VoiceSession {
    pub(crate) channel: ChannelId,
    pub(crate) channel_name: String,
    pub(crate) participants: Vec<String>,
    pub(crate) output: OutputHandle,
    pub(crate) track: TrackHandle,
    pub(crate) track_ended: Arc<Notify>,
    pub(crate) panel: Option<PanelRef>,
    /// The leg toward Discord: voice server, our encoder's counters, Discord's reports.
    pub(crate) delivery: Arc<DeliveryLeg>,
    /// Applies the output's sending wish to songbird's driver; aborted at the leave.
    pub(crate) sending_task: JoinHandle<()>,
}

/// Applies each change of the output's sending wish (change `transmit-only-when-speaking`)
/// until the output is gone: `apply(true)` when something is audible and Discord should get
/// the voice, `apply(false)` when nothing should be sent. Changes made faster than they are
/// applied fold into the latest one.
pub(crate) async fn apply_sending<F, Fut>(mut sending: watch::Receiver<bool>, mut apply: F)
where
    F: FnMut(bool) -> Fut,
    Fut: Future<Output = ()>,
{
    while sending.changed().await.is_ok() {
        let on = *sending.borrow_and_update();
        apply(on).await;
    }
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
    // The leg toward Discord: its connection and RTCP handlers go in before the join, so
    // the server and our SSRC from `DriverConnect` are not missed.
    let delivery = Arc::new(DeliveryLeg::new(Arc::new(DeliveryShared::default())));
    // The codec of this join (change `opus-codec-experiment`): with libopus songbird decodes
    // the speakers as before; with a pure-Rust codec it only decrypts and the receive
    // handler decodes. songbird takes a config set before `join` for that connection.
    let codec = shared.host.opus_codec();
    // Where this join's codec panics go; a new join may report again (design D5).
    let failures = shared.host.codec_failures(guild.get());
    let own_decoding = codec.is_pure_rust().then_some(codec);
    log::info!(
        "[receive] Opus codec for this join: {} (speakers decoded by {})",
        codec.label(),
        if own_decoding.is_some() {
            "the app"
        } else {
            "songbird"
        }
    );
    let call = manager.get_or_insert(guild);
    {
        let mut call = call.lock().await;
        let mut config = call.config().clone();
        config.decode_mode = if own_decoding.is_some() {
            DecodeMode::Decrypt
        } else {
            DecodeMode::Decode(DecodeConfig::new(
                songbird::driver::Channels::Stereo,
                SampleRate::Hz48000,
            ))
        };
        call.set_config(config);
        call.remove_all_global_events();
        call.add_global_event(
            CoreEvent::SpeakingStateUpdate.into(),
            ReceiveHandler::new(Arc::clone(shared), guild, Arc::clone(&directory), None),
        );
        call.add_global_event(
            CoreEvent::VoiceTick.into(),
            ReceiveHandler::new(
                Arc::clone(shared),
                guild,
                Arc::clone(&directory),
                own_decoding.map(|codec| (codec, failures.clone())),
            ),
        );
        for event in [
            CoreEvent::DriverConnect,
            CoreEvent::DriverReconnect,
            CoreEvent::RtcpPacket,
        ] {
            call.add_global_event(event.into(), DeliveryHandler(Arc::clone(&delivery)));
        }
    }
    if let Err(error) = manager.join(guild, channel).await {
        let _ = manager.remove(guild).await;
        return JoinOutcome::Failed(format!("join: {error}"));
    }

    let (output, source) = output_pair();
    // The voice goes out as Opus packets the app encodes with FEC, which songbird passes
    // through (change `discord-voice-delivery`); songbird's own PCM path if that track
    // cannot be built.
    let input: songbird::input::Input =
        match opus_out::live_input_reporting(source, Arc::clone(&delivery.shared), codec, failures)
        {
            Ok(input) => input,
            Err(source) => {
                log::warn!(
                    "the Opus output could not be built; songbird encodes the voice (no FEC)"
                );
                RawAdapter::new(source, SAMPLE_RATE, CHANNELS as u32).into()
            }
        };
    let track_ended = Arc::new(Notify::new());
    let (track, endpoint) = {
        let mut call = call.lock().await;
        // The endpoint's host only; the connection's session id and token stay out.
        let endpoint = call
            .current_connection()
            .map(|connection| delivery::host_of(&connection.endpoint).to_owned());
        // Nothing goes to Discord until something is audible (change
        // `transmit-only-when-speaking`). The driver's mute only: songbird keeps pulling the
        // track and sends nothing. `Call::mute` would also show the bot muted in Discord.
        songbird::Driver::mute(&mut call, true);
        (call.play_only_input(input), endpoint)
    };
    let sending_task = tokio::spawn(apply_sending(output.sending(), {
        let call = Arc::clone(&call);
        move |on| {
            let call = Arc::clone(&call);
            async move {
                songbird::Driver::mute(&mut *call.lock().await, !on);
            }
        }
    }));
    crate::detail!(
        "delivery",
        "{}",
        delivery::join_line(guild.get(), delivery.server().or(endpoint).as_deref())
    );
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

    let delivery_counters = delivery.counters();
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
            delivery,
            sending_task,
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
                delivery: delivery_counters,
                opus_codec: codec.label(),
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
    // It holds the call: stop it before the call is removed.
    session.sending_task.abort();
    let _ = tokio::time::timeout(Duration::from_millis(200), session.track_ended.notified()).await;
    let _ = session.track.stop();
    crate::detail!(
        "delivery",
        "{}",
        delivery::leave_line(
            guild.get(),
            &session.delivery.counters(),
            &session.output.counters()
        )
    );
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

    /// The next value the loop applied, if one comes within 100 ms.
    async fn next_applied(seen: &mut tokio::sync::mpsc::UnboundedReceiver<bool>) -> Option<bool> {
        tokio::time::timeout(Duration::from_millis(100), seen.recv())
            .await
            .ok()
            .flatten()
    }

    #[test]
    fn the_sending_loop_applies_each_change_and_ends_with_the_output() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        tokio_rt.block_on(async {
            let (wish, receiver) = watch::channel(false);
            let (applied, mut seen) = tokio::sync::mpsc::unbounded_channel();
            let task = tokio::spawn(apply_sending(receiver, move |on| {
                let applied = applied.clone();
                async move {
                    applied.send(on).unwrap();
                }
            }));
            assert_eq!(
                next_applied(&mut seen).await,
                None,
                "nothing applied before a change"
            );
            for on in [true, false, true] {
                wish.send_replace(on);
                assert_eq!(next_applied(&mut seen).await, Some(on), "applied in order");
            }
            assert_eq!(
                next_applied(&mut seen).await,
                None,
                "nothing applied without a change"
            );
            drop(wish);
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .expect("the loop ends with the output")
                .unwrap();
        });
    }
}
