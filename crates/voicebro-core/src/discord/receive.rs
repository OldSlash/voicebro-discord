//! The receive path: every 20 ms songbird hands over each speaker's audio, decoded by
//! songbird (libopus) or, with a pure-Rust codec selected at the join, as decrypted packets
//! the app decodes itself ([`super::speaker_decode`]); this mixes it in `i32`, clamps once,
//! and gives the mix and the per-speaker frames to the host.

use super::speaker_decode::{opus_payload, SpeakerDecoders};
use super::speakers::SpeakerEntry;
use super::{Shared, SpeakerDirectory};
use crate::access::VoiceFilter;
use crate::audio::codec::{CodecFailures, OpusCodec};
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

/// The sources of one tick whose voice passes the access filter (change `voice-consent`,
/// design D4), in the tick's order. A source with no known account passes only the
/// `everyone` mode.
pub(crate) fn passing_sources(
    filter: &VoiceFilter,
    directory: &SpeakerDirectory,
    sources: impl IntoIterator<Item = u32>,
) -> Vec<u32> {
    sources
        .into_iter()
        .filter(|ssrc| filter.is_everyone() || filter.passes(directory.user(*ssrc)))
        .collect()
}

/// Registered before `join`; lives as long as the call.
pub(crate) struct ReceiveHandler {
    pub(crate) shared: Arc<Shared>,
    pub(crate) guild: GuildId,
    pub(crate) directory: Arc<SpeakerDirectory>,
    scratch: Mutex<Scratch>,
    /// Audio sources the "only person in the channel" fallback was tried for.
    fallback_tried: Mutex<HashSet<u32>>,
    /// Audio sources already logged as not the owner during this join.
    ignored_logged: Mutex<HashSet<u32>>,
    /// The speakers' decoders when the app decodes them itself (songbird in `Decrypt`).
    decoders: Option<Mutex<SpeakerDecoders>>,
}

impl ReceiveHandler {
    /// `own_decoding` is the codec the app decodes the speakers with and where a panic of it
    /// is reported; `None` when songbird decodes them.
    pub(crate) fn new(
        shared: Arc<Shared>,
        guild: GuildId,
        directory: Arc<SpeakerDirectory>,
        own_decoding: Option<(OpusCodec, CodecFailures)>,
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
            ignored_logged: Mutex::new(HashSet::new()),
            decoders: own_decoding
                .map(|(codec, failures)| Mutex::new(SpeakerDecoders::new(codec, failures))),
        }
    }

    /// Once per audio source Discord never announced: when exactly one person is in the
    /// bot's channel, that person (name and account) is taken as its speaker. A source with
    /// an entry, a bot's among them, is left alone. A real mapping from a later speaking
    /// event replaces it.
    fn try_sole_person(&self, ssrc: u32) {
        if self.directory.knows(ssrc)
            || !self
                .fallback_tried
                .lock()
                .expect("fallback lock")
                .insert(ssrc)
        {
            return;
        }
        let people = self
            .shared
            .sessions
            .lock()
            .expect("sessions lock")
            .get(&self.guild.get())
            .map(|session| session.people.clone())
            .unwrap_or_default();
        if let Some((user, name)) = super::speakers::sole_person(&people) {
            let entry = SpeakerEntry {
                user: Some(*user),
                bot: false,
                name: Some(name.clone()),
            };
            if self.directory.remember(ssrc, entry) {
                crate::detail!(
                    "receive",
                    "Speaker: {name} (ssrc {ssrc}), assigned as the only person in the channel"
                );
                self.shared
                    .host
                    .set_speaker_name(self.guild.get(), ssrc, name);
            }
        }
    }

    /// Counts a dropped frame and logs its source once per join, with the reason of the
    /// access mode.
    fn ignore(&self, ssrc: u32) {
        self.shared.ignored_frames.fetch_add(1, Ordering::Relaxed);
        if self
            .ignored_logged
            .lock()
            .expect("ignored lock")
            .insert(ssrc)
        {
            let who = self
                .directory
                .name(ssrc)
                .unwrap_or_else(|| "an unknown source".to_owned());
            let reason = match self.shared.host.access_mode() {
                crate::settings::AccessMode::Agreed => "has not run /optin",
                _ => "not the owner",
            };
            log::info!(
                "receive: ignored {who} (ssrc {ssrc}) in guild {}: {reason}",
                self.guild.get()
            );
        }
    }
}

#[async_trait]
impl VoiceEventHandler for ReceiveHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        match ctx {
            EventContext::VoiceTick(tick) => {
                self.shared.ticks.fetch_add(1, Ordering::Relaxed);
                // The access filter (change `voice-consent`) is read once per tick; the "only
                // person" rule runs first, so the only person is known by their account.
                let filter = self.shared.host.voice_filter();
                if !filter.is_everyone() {
                    for ssrc in tick.speaking.keys() {
                        self.try_sole_person(*ssrc);
                    }
                }
                let passing =
                    passing_sources(&filter, &self.directory, tick.speaking.keys().copied());
                for ssrc in tick.speaking.keys() {
                    if !passing.contains(ssrc) {
                        self.ignore(*ssrc);
                    }
                }
                let mut own = self
                    .decoders
                    .as_ref()
                    .map(|decoders| decoders.lock().expect("speaker decoders lock"));
                if let Some(decoders) = own.as_mut() {
                    decoders.begin_tick();
                    for (ssrc, data) in &tick.speaking {
                        if !passing.contains(ssrc) {
                            continue;
                        }
                        let packet = data.packet.as_ref().and_then(|rtp| {
                            let sequence: u16 = rtp.rtp().get_sequence().into();
                            opus_payload(&rtp.packet, rtp.payload_offset, rtp.payload_end_pad)
                                .map(|payload| (sequence, payload))
                        });
                        if decoders.decode(*ssrc, packet).is_err() {
                            self.shared.decode_errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                let mut speakers: Vec<(u32, &[i16])> = Vec::with_capacity(tick.speaking.len());
                for (ssrc, data) in &tick.speaking {
                    if !passing.contains(ssrc) {
                        continue;
                    }
                    let pcm = match own.as_ref() {
                        Some(decoders) => decoders.frame(*ssrc),
                        None => data.decoded_voice.as_deref(),
                    };
                    let Some(pcm) = pcm else {
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
                // The account is known now; the name may take an HTTP lookup. The owner
                // filter needs only the account (change `owner-only-access`).
                self.directory.set_user(ssrc, user.0);
                let (Some(http), Some(cache)) = (self.shared.http(), self.shared.cache()) else {
                    return None;
                };
                let (name, bot) = super::speakers::member_info(
                    &http,
                    &cache,
                    self.guild,
                    serenity::all::UserId::new(user.0),
                )
                .await
                .unwrap_or((None, false));
                // Every announced source keeps its account, bots included, so the owner
                // filter knows it and the "only person" rule leaves it alone.
                let entry = SpeakerEntry {
                    user: Some(user.0),
                    bot,
                    name: name.clone(),
                };
                let renamed = self.directory.remember(ssrc, entry);
                match name {
                    Some(name) if renamed => {
                        crate::detail!("receive", "Speaker: {name} (ssrc {ssrc})");
                        self.shared
                            .host
                            .set_speaker_name(self.guild.get(), ssrc, &name);
                    }
                    Some(_) => {}
                    None => {
                        if bot {
                            crate::detail!("receive", "Speaker: a bot (ssrc {ssrc})");
                        } else {
                            self.shared.unnamed_sources.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: u64 = 1091401169441656933;

    /// The owner on source 11, another member on 22, a bot on 33; source 44 is unknown.
    fn directory() -> SpeakerDirectory {
        let directory = SpeakerDirectory::default();
        for (ssrc, user, bot, name) in [
            (11, OWNER, false, Some("Alex")),
            (22, 2, false, Some("Sam")),
            (33, 3, true, None),
        ] {
            directory.remember(
                ssrc,
                SpeakerEntry {
                    user: Some(user),
                    bot,
                    name: name.map(str::to_owned),
                },
            );
        }
        directory
    }

    fn only(ids: &[u64]) -> VoiceFilter {
        VoiceFilter::Only(ids.iter().copied().collect())
    }

    /// Changes `owner-only-access` and `voice-consent`: everyone in the `everyone` mode,
    /// nobody with an empty set, the owner alone in `owner`, the owner and the recorded in
    /// `agreed`; an unknown source and a bot pass only `everyone`.
    #[test]
    fn the_filter_passes_by_account() {
        let directory = directory();
        let all = [11, 22, 33, 44];
        assert_eq!(
            passing_sources(&VoiceFilter::Everyone, &directory, all),
            all
        );
        assert!(passing_sources(&only(&[]), &directory, all).is_empty());
        assert_eq!(passing_sources(&only(&[OWNER]), &directory, all), [11]);
        assert_eq!(
            passing_sources(&only(&[OWNER, 2]), &directory, all),
            [11, 22],
            "agreed: the owner and Sam, who has a record"
        );
        assert_eq!(
            passing_sources(&only(&[2]), &directory, all),
            [22],
            "agreed without an owner: Sam only"
        );
    }

    /// Scenario "Two people at once": the mix of the owner and another member is the
    /// owner's frame alone; in `agreed`, the agreeing member's frame alone.
    #[test]
    fn the_mix_holds_only_the_owner() {
        let directory = directory();
        let owner: Vec<i16> = (0..FRAME_SAMPLES).map(|i| (i % 2000) as i16).collect();
        let other = vec![-12_000i16; FRAME_SAMPLES];
        let tick = [(11u32, owner.as_slice()), (22u32, other.as_slice())];
        let agreed = passing_sources(&only(&[2]), &directory, tick.iter().map(|(ssrc, _)| *ssrc));
        assert_eq!(agreed, [22], "only the member with a record");
        let passing = passing_sources(
            &only(&[OWNER]),
            &directory,
            tick.iter().map(|(ssrc, _)| *ssrc),
        );
        let blocks: Vec<&[i16]> = tick
            .iter()
            .filter(|(ssrc, _)| passing.contains(ssrc))
            .map(|(_, pcm)| *pcm)
            .collect();
        let (mut sums, mut mixed) = (Vec::new(), Frame::silence());
        mix_into(&blocks, &mut sums, &mut mixed);
        assert_eq!(mixed, Frame::from_slice(&owner));

        let everyone: Vec<&[i16]> = tick.iter().map(|(_, pcm)| *pcm).collect();
        mix_into(&everyone, &mut sums, &mut mixed);
        assert_ne!(
            mixed,
            Frame::from_slice(&owner),
            "without the filter both are mixed"
        );
    }
}
