//! The receive path: every 20 ms songbird hands over the decoded audio of each speaker; this
//! mixes it in `i32`, clamps once, and gives the mix and the per-speaker frames to the host.

use super::{Shared, SpeakerDirectory};
use crate::audio::{frame::mix_into, Frame, FRAME_SAMPLES};
use serenity::all::GuildId;
use serenity::async_trait;
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

struct Scratch {
    sums: Vec<i32>,
    mixed: Frame,
}

/// Registered before `join`; lives as long as the call.
pub(crate) struct ReceiveHandler {
    pub(crate) shared: Arc<Shared>,
    pub(crate) guild: GuildId,
    pub(crate) directory: Arc<SpeakerDirectory>,
    scratch: Mutex<Scratch>,
    /// Audio sources the "only person in the channel" fallback was tried for.
    fallback_tried: Mutex<HashSet<u32>>,
}

impl ReceiveHandler {
    pub(crate) fn new(
        shared: Arc<Shared>,
        guild: GuildId,
        directory: Arc<SpeakerDirectory>,
    ) -> Self {
        Self {
            shared,
            guild,
            directory,
            scratch: Mutex::new(Scratch {
                sums: Vec::with_capacity(FRAME_SAMPLES),
                mixed: Frame::silence(),
            }),
            fallback_tried: Mutex::new(HashSet::new()),
        }
    }

    /// Once per audio source Discord has not named: when exactly one person is in the
    /// bot's channel, that person is taken as its speaker. A real mapping from a later
    /// speaking event replaces it.
    fn try_sole_person(&self, ssrc: u32) {
        if self.directory.name(ssrc).is_some()
            || !self
                .fallback_tried
                .lock()
                .expect("fallback lock")
                .insert(ssrc)
        {
            return;
        }
        let participants = self
            .shared
            .sessions
            .lock()
            .expect("sessions lock")
            .get(&self.guild.get())
            .map(|session| session.participants.clone())
            .unwrap_or_default();
        if let Some(name) = super::speakers::sole_person(&participants) {
            if self.directory.remember(ssrc, name.to_owned()) {
                self.shared.log(format!(
                    "Speaker: {name} (ssrc {ssrc}), assigned as the only person in the channel"
                ));
                self.shared
                    .host
                    .set_speaker_name(self.guild.get(), ssrc, name);
            }
        }
    }
}

#[async_trait]
impl VoiceEventHandler for ReceiveHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        match ctx {
            EventContext::VoiceTick(tick) => {
                self.shared.ticks.fetch_add(1, Ordering::Relaxed);
                let mut speakers: Vec<(u32, &[i16])> = Vec::with_capacity(tick.speaking.len());
                for (ssrc, data) in &tick.speaking {
                    let Some(pcm) = data.decoded_voice.as_deref() else {
                        continue;
                    };
                    if pcm.len() != FRAME_SAMPLES {
                        self.shared.malformed_frames.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    self.try_sole_person(*ssrc);
                    speakers.push((*ssrc, pcm));
                }
                self.shared
                    .speaker_frames
                    .fetch_add(speakers.len() as u64, Ordering::Relaxed);
                let mut scratch = self.scratch.lock().expect("receive scratch lock");
                let Scratch { sums, mixed } = &mut *scratch;
                let blocks: Vec<&[i16]> = speakers.iter().map(|(_, pcm)| *pcm).collect();
                mix_into(&blocks, sums, mixed);
                self.shared.host.on_tick(self.guild.get(), mixed, &speakers);
            }
            EventContext::SpeakingStateUpdate(speaking) => {
                let Some(user) = speaking.user_id else {
                    return None;
                };
                let ssrc = speaking.ssrc;
                let (Some(http), Some(cache)) = (self.shared.http(), self.shared.cache()) else {
                    return None;
                };
                let name = super::speakers::member_label(
                    &http,
                    &cache,
                    self.guild,
                    serenity::all::UserId::new(user.0),
                )
                .await;
                match name {
                    Some(name) => {
                        if self.directory.remember(ssrc, name.clone()) {
                            self.shared.log(format!("Speaker: {name} (ssrc {ssrc})"));
                            self.shared
                                .host
                                .set_speaker_name(self.guild.get(), ssrc, &name);
                        }
                    }
                    None => {
                        self.shared.unnamed_sources.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            _ => {}
        }
        None
    }
}
