//! songbird lifecycle, receive-path timing, and what Discord reports about our stream.
//!
//! All probes join a real voice channel (token, guild and channel from the environment).
//! `discord-lifecycle` registers a counting `VoiceTick` handler and a silent output track,
//! closes the track's source, removes the call, rejoins with a second handler, and checks
//! that the first handler is silent and the track reported its end. `receive-timing` sits in
//! the channel for a minute, does the real handler's work (mix every speaker into `i32`,
//! clamp) and reports tick-interval and handler-duration percentiles. `discord-delivery`
//! plays a tone and records the voice server, our SSRC and every RTCP packet Discord sends,
//! with the report blocks it carries (change `discord-voice-delivery`, task 2.1).

use crate::Outcome;
use anyhow::{anyhow, Context as _};
use serenity::all::{ChannelId, GuildId};
use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::gateway::Ready;
use serenity::prelude::GatewayIntents;
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};
use songbird::input::RawAdapter;
use songbird::{CoreEvent, SerenityInit};
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use symphonia_core::io::MediaSource;
use tokio::sync::oneshot;
use voicebro_core::audio::opus_out::{self, DeliveryShared, LossSource};
use voicebro_core::audio::{output_pair, Frame, OutputHandle};
use voicebro_core::discord::for_each_report_block;

const FRAME_SAMPLES: usize = 1920; // 20 ms, 48 kHz stereo

struct Env {
    token: String,
    guild: GuildId,
    channel: ChannelId,
}

/// Test inputs come from the environment, or from `probe.local.json` in the workspace root
/// (gitignored): `{ "discord_token", "guild_id", "channel_id" }`.
fn env() -> anyhow::Result<Env> {
    let local: serde_json::Value = std::fs::read_to_string("probe.local.json")
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null);
    let value = |env_name: &str, local_key: &str| -> anyhow::Result<String> {
        if let Ok(value) = std::env::var(env_name) {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
        local
            .get(local_key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("{env_name} is not set and probe.local.json has no {local_key}"))
    };
    Ok(Env {
        token: value("VOICEBRO_DISCORD_TOKEN", "discord_token")?,
        guild: GuildId::new(
            value("VOICEBRO_TEST_GUILD", "guild_id")?
                .trim()
                .parse()
                .context("guild id")?,
        ),
        channel: ChannelId::new(
            value("VOICEBRO_TEST_CHANNEL", "channel_id")?
                .trim()
                .parse()
                .context("channel id")?,
        ),
    })
}

/// A 48 kHz stereo silence source that ends when `closed` is set.
struct SilenceSource {
    closed: Arc<AtomicBool>,
    reads: Arc<AtomicUsize>,
}

impl Read for SilenceSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(0);
        }
        self.reads.fetch_add(1, Ordering::Relaxed);
        let bytes = buf.len().min(FRAME_SAMPLES * 4); // one packet of f32 stereo at most
        buf[..bytes].fill(0);
        Ok(bytes)
    }
}

impl Seek for SilenceSource {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other("silence source is not seekable"))
    }
}

impl MediaSource for SilenceSource {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// A 440 Hz tone at 48 kHz stereo f32 that ends when `closed` is set.
struct ToneSource {
    closed: Arc<AtomicBool>,
    phase: f32,
}

impl Read for ToneSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(0);
        }
        let frames = (buf.len() / 8).min(FRAME_SAMPLES / 2);
        for frame in buf.as_chunks_mut::<8>().0.iter_mut().take(frames) {
            let value = (self.phase * std::f32::consts::TAU).sin() * 0.2;
            self.phase = (self.phase + 440.0 / 48_000.0).fract();
            frame[..4].copy_from_slice(&value.to_le_bytes());
            frame[4..].copy_from_slice(&value.to_le_bytes());
        }
        Ok(frames * 8)
    }
}

impl Seek for ToneSource {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other("tone source is not seekable"))
    }
}

impl MediaSource for ToneSource {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// What a report block said about one SSRC.
#[derive(Default)]
struct BlockSummary {
    reports: u32,
    last_fraction_lost: u8,
    max_fraction_lost: u8,
    last_jitter: u32,
}

/// The voice server, our SSRC, and Discord's RTCP: packets by type, senders, report blocks.
#[derive(Default)]
struct DeliveryLog {
    server: Option<String>,
    ssrc: Option<u32>,
    packets_by_type: BTreeMap<u8, u32>,
    senders: BTreeMap<u32, u32>,
    blocks: BTreeMap<u32, BlockSummary>,
    first_report_at: Option<Duration>,
    started: Option<Instant>,
    /// The Opus output's expected loss after each RTCP packet.
    expected: Vec<(u8, LossSource)>,
}

impl DeliveryLog {
    /// Walks a compound RTCP packet: each part's type, its sender, and the report blocks of
    /// sender (200) and receiver (201) reports (RFC 3550 6.4).
    fn note_rtcp(&mut self, bytes: &[u8]) {
        let mut at = 0;
        while let Some(header) = bytes.get(at..at + 4) {
            let count = usize::from(header[0] & 0x1f);
            let kind = header[1];
            // Version 2 and an RTCP type (192-223); anything else is past the payload.
            if header[0] >> 6 != 2 || !(192..=223).contains(&kind) {
                break;
            }
            let words = usize::from(u16::from_be_bytes([header[2], header[3]]));
            let end = (at + (words + 1) * 4).min(bytes.len());
            *self.packets_by_type.entry(kind).or_default() += 1;
            if let Some(sender) = bytes.get(at + 4..at + 8) {
                let sender = u32::from_be_bytes([sender[0], sender[1], sender[2], sender[3]]);
                *self.senders.entry(sender).or_default() += 1;
            }
            let blocks_at = match kind {
                200 => Some(at + 28),
                201 => Some(at + 8),
                _ => None,
            };
            if let Some(first) = blocks_at {
                for index in 0..count {
                    let Some(block) = bytes.get(first + index * 24..first + index * 24 + 24) else {
                        break;
                    };
                    if block.len() > 24 || first + index * 24 + 24 > end {
                        break;
                    }
                    let ssrc = u32::from_be_bytes([block[0], block[1], block[2], block[3]]);
                    let jitter = u32::from_be_bytes([block[12], block[13], block[14], block[15]]);
                    let summary = self.blocks.entry(ssrc).or_default();
                    summary.reports += 1;
                    summary.last_fraction_lost = block[4];
                    summary.max_fraction_lost = summary.max_fraction_lost.max(block[4]);
                    summary.last_jitter = jitter;
                    if Some(ssrc) == self.ssrc && self.first_report_at.is_none() {
                        self.first_report_at = self.started.map(|s| s.elapsed());
                    }
                }
            }
            at = end.max(at + 4);
        }
    }
}

/// Records the connection and every RTCP packet into a [`DeliveryLog`], and feeds report
/// blocks about our stream to the Opus output's expected loss, as the app does.
struct DeliveryHandler {
    log: Arc<Mutex<DeliveryLog>>,
    shared: Arc<DeliveryShared>,
}

#[async_trait]
impl VoiceEventHandler for DeliveryHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let Ok(mut log) = self.log.lock() else {
            return None;
        };
        match ctx {
            EventContext::DriverConnect(data) | EventContext::DriverReconnect(data) => {
                // The server's domain and our SSRC only: never the session id.
                log.server = Some(data.server.split(':').next().unwrap_or("").to_owned());
                log.ssrc = Some(data.ssrc);
            }
            EventContext::RtcpPacket(data) => {
                // The decrypted packet without the trailing pad songbird reports.
                let end = data.packet.len().saturating_sub(data.payload_end_pad);
                let bytes = data.packet.get(..end).unwrap_or(&[]);
                log.note_rtcp(bytes);
                if let Some(ours) = log.ssrc {
                    for_each_report_block(bytes, |block| {
                        if block.ssrc == ours {
                            self.shared.on_report(block.fraction_lost, block.jitter);
                        }
                    });
                    let (loss, source) = self.shared.expected_loss_now();
                    log.expected.push((loss, source));
                }
            }
            _ => {}
        }
        None
    }
}

/// Counts voice ticks and remembers speakers; logs SSRC announcements.
struct TickCounter {
    label: &'static str,
    ticks: Arc<AtomicU64>,
    log: Arc<Mutex<Vec<String>>>,
    speakers: Arc<Mutex<HashSet<u32>>>,
}

#[async_trait]
impl VoiceEventHandler for TickCounter {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        match ctx {
            EventContext::VoiceTick(tick) => {
                self.ticks.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut set) = self.speakers.lock() {
                    set.extend(tick.speaking.keys().copied());
                }
            }
            EventContext::SpeakingStateUpdate(speaking) => {
                if let Ok(mut log) = self.log.lock() {
                    log.push(format!(
                        "[{}] speaking update: ssrc={} user={:?}",
                        self.label, speaking.ssrc, speaking.user_id
                    ));
                }
            }
            _ => {}
        }
        None
    }
}

struct TrackLogger {
    log: Arc<Mutex<Vec<String>>>,
    ends: Arc<AtomicUsize>,
}

#[async_trait]
impl VoiceEventHandler for TrackLogger {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(tracks) = ctx {
            for (state, _handle) in tracks.iter() {
                if let Ok(mut log) = self.log.lock() {
                    log.push(format!(
                        "track event: playing={:?} position={:?}",
                        state.playing, state.position
                    ));
                }
                if matches!(state.playing, songbird::tracks::PlayMode::End) {
                    self.ends.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        None
    }
}

/// Records tick intervals and the handler's own duration while doing the real mix.
struct TimingHandler {
    last_tick: Mutex<Option<Instant>>,
    intervals: Mutex<Vec<Duration>>,
    durations: Mutex<Vec<Duration>>,
    speaker_counts: Mutex<Vec<usize>>,
    scratch: Mutex<(Vec<i32>, Vec<i16>)>,
    log: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl VoiceEventHandler for TimingHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        match ctx {
            EventContext::VoiceTick(tick) => {
                let started = Instant::now();
                if let Ok(mut last) = self.last_tick.lock() {
                    if let Some(previous) = *last {
                        if let Ok(mut intervals) = self.intervals.lock() {
                            intervals.push(started.duration_since(previous));
                        }
                    }
                    *last = Some(started);
                }
                // The real handler's work: sum every speaker into i32, clamp into i16.
                let mut speakers = 0usize;
                if let Ok(mut scratch) = self.scratch.lock() {
                    let (acc, mixed) = &mut *scratch;
                    acc.clear();
                    acc.resize(FRAME_SAMPLES, 0);
                    for data in tick.speaking.values() {
                        let Some(pcm) = data.decoded_voice.as_deref() else {
                            continue;
                        };
                        if pcm.len() != FRAME_SAMPLES {
                            continue;
                        }
                        speakers += 1;
                        for (sum, sample) in acc.iter_mut().zip(pcm) {
                            *sum += *sample as i32;
                        }
                    }
                    mixed.clear();
                    mixed.extend(acc.iter().map(|sum| (*sum).clamp(-32768, 32767) as i16));
                }
                if let Ok(mut counts) = self.speaker_counts.lock() {
                    counts.push(speakers);
                }
                if let Ok(mut durations) = self.durations.lock() {
                    durations.push(started.elapsed());
                }
            }
            EventContext::SpeakingStateUpdate(speaking) => {
                if let Ok(mut log) = self.log.lock() {
                    log.push(format!(
                        "speaking update: ssrc={} user={:?}",
                        speaking.ssrc, speaking.user_id
                    ));
                }
            }
            _ => {}
        }
        None
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

enum Scenario {
    Lifecycle,
    Timing {
        seconds: u64,
    },
    /// `opus`: the tone goes through the app's output and its own Opus encoder (songbird
    /// passes the packets through) instead of songbird's PCM path.
    Delivery {
        seconds: u64,
        opus: bool,
        codec: voicebro_core::audio::codec::OpusCodec,
    },
}

struct Handler {
    env_guild: GuildId,
    env_channel: ChannelId,
    scenario: Scenario,
    result: Mutex<Option<oneshot::Sender<anyhow::Result<Vec<String>>>>>,
}

impl Handler {
    async fn run_lifecycle(&self, ctx: &Context) -> anyhow::Result<Vec<String>> {
        let mut details = Vec::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let manager = songbird::get(ctx)
            .await
            .ok_or_else(|| anyhow!("songbird is not registered"))?;

        // First join: handler 1 registered before join, silent track attached after.
        let ticks_1 = Arc::new(AtomicU64::new(0));
        let speakers = Arc::new(Mutex::new(HashSet::new()));
        {
            let call = manager.get_or_insert(self.env_guild);
            let mut call = call.lock().await;
            call.add_global_event(
                CoreEvent::SpeakingStateUpdate.into(),
                TickCounter {
                    label: "h1",
                    ticks: Arc::clone(&ticks_1),
                    log: Arc::clone(&log),
                    speakers: Arc::clone(&speakers),
                },
            );
            call.add_global_event(
                CoreEvent::VoiceTick.into(),
                TickCounter {
                    label: "h1",
                    ticks: Arc::clone(&ticks_1),
                    log: Arc::clone(&log),
                    speakers: Arc::clone(&speakers),
                },
            );
        }
        let joined_at = Instant::now();
        let call = manager
            .join(self.env_guild, self.env_channel)
            .await
            .map_err(|error| anyhow!("join: {error}"))?;
        details.push(format!("joined in {:?}", joined_at.elapsed()));

        let closed = Arc::new(AtomicBool::new(false));
        let reads = Arc::new(AtomicUsize::new(0));
        let ends = Arc::new(AtomicUsize::new(0));
        let track = {
            let source = SilenceSource {
                closed: Arc::clone(&closed),
                reads: Arc::clone(&reads),
            };
            let input: songbird::input::Input = RawAdapter::new(source, 48_000, 2).into();
            let mut call = call.lock().await;
            call.play_only_input(input)
        };
        track
            .add_event(
                Event::Track(TrackEvent::End),
                TrackLogger {
                    log: Arc::clone(&log),
                    ends: Arc::clone(&ends),
                },
            )
            .map_err(|error| anyhow!("add track event: {error}"))?;
        track
            .add_event(
                Event::Track(TrackEvent::Error),
                TrackLogger {
                    log: Arc::clone(&log),
                    ends: Arc::clone(&ends),
                },
            )
            .map_err(|error| anyhow!("add track event: {error}"))?;

        tokio::time::sleep(Duration::from_secs(5)).await;
        let ticks_a = ticks_1.load(Ordering::Relaxed);
        details.push(format!(
            "after 5 s: handler 1 ticks={ticks_a}, source reads={}, speakers seen={}",
            reads.load(Ordering::Relaxed),
            speakers.lock().map(|s| s.len()).unwrap_or(0)
        ));

        // Close the source: the track must end by itself.
        closed.store(true, Ordering::Release);
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        let ends_after_close = ends.load(Ordering::Relaxed);
        details.push(format!(
            "1.5 s after closing the source: track End events={ends_after_close}"
        ));

        // Remove the call entirely.
        manager
            .remove(self.env_guild)
            .await
            .map_err(|error| anyhow!("remove: {error}"))?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let ticks_b = ticks_1.load(Ordering::Relaxed);
        details.push(format!(
            "2 s after remove: handler 1 ticks={ticks_b} (was {ticks_a})"
        ));

        // Rejoin with handler 2; handler 1 must stay silent.
        let ticks_2 = Arc::new(AtomicU64::new(0));
        {
            let call = manager.get_or_insert(self.env_guild);
            let mut call = call.lock().await;
            call.add_global_event(
                CoreEvent::VoiceTick.into(),
                TickCounter {
                    label: "h2",
                    ticks: Arc::clone(&ticks_2),
                    log: Arc::clone(&log),
                    speakers: Arc::clone(&speakers),
                },
            );
        }
        manager
            .join(self.env_guild, self.env_channel)
            .await
            .map_err(|error| anyhow!("rejoin: {error}"))?;
        tokio::time::sleep(Duration::from_secs(5)).await;
        let ticks_c = ticks_1.load(Ordering::Relaxed);
        let ticks_2_value = ticks_2.load(Ordering::Relaxed);
        details.push(format!(
            "5 s after rejoin: handler 1 ticks={ticks_c} (unchanged={}), handler 2 ticks={ticks_2_value}",
            ticks_c == ticks_b
        ));
        manager
            .remove(self.env_guild)
            .await
            .map_err(|error| anyhow!("final remove: {error}"))?;

        details.extend(log.lock().map(|l| l.clone()).unwrap_or_default());
        let pass =
            ticks_a > 100 && ticks_c == ticks_b && ticks_2_value > 100 && ends_after_close == 1;
        details.push(format!(
            "criterion: handler 1 silent after remove ({}), handler 2 ticking ({}), exactly one End after close ({})",
            ticks_c == ticks_b,
            ticks_2_value > 100,
            ends_after_close == 1
        ));
        details.push(format!("VERDICT {}", if pass { "PASS" } else { "FAIL" }));
        Ok(details)
    }

    async fn run_timing(&self, ctx: &Context, seconds: u64) -> anyhow::Result<Vec<String>> {
        let mut details = Vec::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let manager = songbird::get(ctx)
            .await
            .ok_or_else(|| anyhow!("songbird is not registered"))?;
        let handler = Arc::new(TimingHandler {
            last_tick: Mutex::new(None),
            intervals: Mutex::new(Vec::with_capacity(4000)),
            durations: Mutex::new(Vec::with_capacity(4000)),
            speaker_counts: Mutex::new(Vec::with_capacity(4000)),
            scratch: Mutex::new((Vec::new(), Vec::new())),
            log: Arc::clone(&log),
        });
        {
            let call = manager.get_or_insert(self.env_guild);
            let mut call = call.lock().await;
            call.add_global_event(
                CoreEvent::SpeakingStateUpdate.into(),
                ArcHandler(Arc::clone(&handler)),
            );
            call.add_global_event(
                CoreEvent::VoiceTick.into(),
                ArcHandler(Arc::clone(&handler)),
            );
        }
        manager
            .join(self.env_guild, self.env_channel)
            .await
            .map_err(|error| anyhow!("join: {error}"))?;
        details.push(format!(
            "joined; measuring for {seconds} s, talk with two people now"
        ));
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        manager
            .remove(self.env_guild)
            .await
            .map_err(|error| anyhow!("remove: {error}"))?;

        let mut intervals = handler
            .intervals
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        let mut durations = handler
            .durations
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        let counts = handler
            .speaker_counts
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        intervals.sort();
        durations.sort();
        let ticks = counts.len();
        let with_two = counts.iter().filter(|c| **c >= 2).count();
        let with_one = counts.iter().filter(|c| **c == 1).count();
        details.push(format!(
            "ticks: {ticks} ({with_one} with one speaker, {with_two} with two or more)"
        ));
        details.push(format!(
            "tick interval: p50 {:?}, p99 {:?}, max {:?}",
            percentile(&intervals, 0.5),
            percentile(&intervals, 0.99),
            intervals.last().copied().unwrap_or_default()
        ));
        let p99 = percentile(&durations, 0.99);
        details.push(format!(
            "handler duration: p50 {:?}, p99 {p99:?}, max {:?}",
            percentile(&durations, 0.5),
            durations.last().copied().unwrap_or_default()
        ));
        details.extend(log.lock().map(|l| l.clone()).unwrap_or_default());
        let pass = ticks > 100 && p99 < Duration::from_millis(2);
        details.push(format!(
            "criterion: p99 handler time < 2 ms ({}); two-speaker ticks seen: {}",
            p99 < Duration::from_millis(2),
            with_two > 0
        ));
        if with_two == 0 {
            details.push("NOT TESTED with two simultaneous speakers".to_owned());
        }
        details.push(format!("VERDICT {}", if pass { "PASS" } else { "FAIL" }));
        Ok(details)
    }
}

impl Handler {
    async fn run_delivery(
        &self,
        ctx: &Context,
        seconds: u64,
        opus: bool,
        codec: voicebro_core::audio::codec::OpusCodec,
    ) -> anyhow::Result<Vec<String>> {
        let mut details = Vec::new();
        if opus {
            details.push(format!("codec: {}", codec.label()));
        }
        let manager = songbird::get(ctx)
            .await
            .ok_or_else(|| anyhow!("songbird is not registered"))?;
        let log = Arc::new(Mutex::new(DeliveryLog::default()));
        let delivery = Arc::new(DeliveryShared::default());
        {
            let call = manager.get_or_insert(self.env_guild);
            let mut call = call.lock().await;
            for event in [
                CoreEvent::DriverConnect,
                CoreEvent::DriverReconnect,
                CoreEvent::RtcpPacket,
            ] {
                call.add_global_event(
                    event.into(),
                    DeliveryHandler {
                        log: Arc::clone(&log),
                        shared: Arc::clone(&delivery),
                    },
                );
            }
        }
        let call = manager
            .join(self.env_guild, self.env_channel)
            .await
            .map_err(|error| anyhow!("join: {error}"))?;
        let closed = Arc::new(AtomicBool::new(false));
        let mut pusher = None;
        let mut output_handle = None;
        {
            let mut call = call.lock().await;
            if let Some(connection) = call.current_connection() {
                // The endpoint only (host and port); the token and session id stay out.
                details.push(format!("voice endpoint: {}", connection.endpoint));
            }
            let input: songbird::input::Input = if opus {
                let (output, source) = output_pair();
                let input = opus_out::live_input(source, Arc::clone(&delivery), codec)
                    .map_err(|_| anyhow!("the Opus output could not be built"))?;
                pusher = Some(push_tone(output.clone(), Arc::clone(&closed)));
                output_handle = Some(output);
                input
            } else {
                let source = ToneSource {
                    closed: Arc::clone(&closed),
                    phase: 0.0,
                };
                RawAdapter::new(source, 48_000, 2).into()
            };
            call.play_only_input(input);
        }
        if let Ok(mut log) = log.lock() {
            log.started = Some(Instant::now());
        }
        details.push(format!(
            "joined; playing a tone for {seconds} s through {}",
            if opus {
                "the app's output and Opus encoder"
            } else {
                "songbird's PCM path"
            }
        ));
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        closed.store(true, Ordering::Release);
        if let Some(output) = output_handle {
            output.close();
        }
        if let Some(pusher) = pusher {
            let _ = pusher.join();
        }
        manager
            .remove(self.env_guild)
            .await
            .map_err(|error| anyhow!("remove: {error}"))?;
        if opus {
            let encoded = delivery.packets_encoded.load(Ordering::Relaxed);
            let decodes = delivery.songbird_decodes.load(Ordering::Relaxed);
            details.push(format!(
                "Opus output: {encoded} packets encoded, {} passed through, {decodes} decoded by songbird, {} encode errors",
                delivery.passed_through(),
                delivery.encode_errors.load(Ordering::Relaxed)
            ));
            if encoded == 0 || decodes > 0 {
                details.push("passthrough: FAIL".to_owned());
            }
            if let Ok(log) = log.lock() {
                let trace: Vec<String> = log
                    .expected
                    .iter()
                    .map(|(loss, source)| {
                        format!(
                            "{loss}{}",
                            if *source == LossSource::Measured {
                                ""
                            } else {
                                "d"
                            }
                        )
                    })
                    .collect();
                details.push(format!(
                    "expected loss after each RTCP packet, % (d = default): {}",
                    trace.join(" ")
                ));
            }
        }

        let log = log.lock().map_err(|_| anyhow!("delivery log poisoned"))?;
        details.push(format!(
            "voice server: {}; our SSRC: {}",
            log.server.as_deref().unwrap_or("?"),
            log.ssrc.map_or("?".to_owned(), |s| s.to_string())
        ));
        details.push(format!(
            "RTCP packets by type: {:?} (200 sender report, 201 receiver report, 202 SDES, 203 BYE, 205/206 feedback)",
            log.packets_by_type
        ));
        details.push(format!("RTCP senders (SSRC: packets): {:?}", log.senders));
        for (ssrc, block) in &log.blocks {
            let ours = Some(*ssrc) == log.ssrc;
            details.push(format!(
                "report blocks about SSRC {ssrc}{}: {} reports, fraction lost last {:.1} % max {:.1} %, jitter last {:.1} ms",
                if ours { " (ours)" } else { "" },
                block.reports,
                f64::from(block.last_fraction_lost) / 2.56,
                f64::from(block.max_fraction_lost) / 2.56,
                f64::from(block.last_jitter) / 48.0
            ));
        }
        let about_us = log
            .ssrc
            .and_then(|ssrc| log.blocks.get(&ssrc))
            .map_or(0, |block| block.reports);
        details.push(format!(
            "Discord reports on our stream: {} ({about_us} report blocks in {seconds} s{})",
            if about_us > 0 { "yes" } else { "no" },
            log.first_report_at
                .map(|at| format!(", the first after {at:.1?}"))
                .unwrap_or_default()
        ));
        let passthrough_ok = !details.iter().any(|line| line == "passthrough: FAIL");
        details.push(format!(
            "VERDICT {}",
            if log.ssrc.is_some() && passthrough_ok {
                "PASS"
            } else {
                "FAIL"
            }
        ));
        Ok(details)
    }
}

/// Lets one `Arc` handler be registered for several event kinds.
struct ArcHandler(Arc<TimingHandler>);

#[async_trait]
impl VoiceEventHandler for ArcHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        self.0.act(ctx).await
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        let mut details = vec![format!(
            "connected as {} ({})",
            ready.user.name, ready.user.id
        )];
        let outcome = match self.scenario {
            Scenario::Lifecycle => self.run_lifecycle(&ctx).await,
            Scenario::Timing { seconds } => self.run_timing(&ctx, seconds).await,
            Scenario::Delivery {
                seconds,
                opus,
                codec,
            } => self.run_delivery(&ctx, seconds, opus, codec).await,
        };
        let result = outcome.map(|lines| {
            details.extend(lines);
            details
        });
        if let Some(sender) = self.result.lock().ok().and_then(|mut slot| slot.take()) {
            let _ = sender.send(result);
        }
        ctx.shard.shutdown_clean();
    }
}

fn run(scenario: Scenario, limit: Duration) -> anyhow::Result<Outcome> {
    let env = env()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (sender, receiver) = oneshot::channel();
        let handler = Handler {
            env_guild: env.guild,
            env_channel: env.channel,
            scenario,
            result: Mutex::new(Some(sender)),
        };
        let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES;
        // Decode to 48 kHz stereo, the only mode the architecture allows (songbird 0.6's
        // packet-loss recovery for mono or 24 kHz is unproven).
        let songbird_config = songbird::Config::default().decode_mode(
            songbird::driver::DecodeMode::Decode(songbird::driver::DecodeConfig::new(
                songbird::driver::Channels::Stereo,
                songbird::driver::SampleRate::Hz48000,
            )),
        );
        let mut client = serenity::Client::builder(&env.token, intents)
            .event_handler(handler)
            .register_songbird_from_config(songbird_config)
            .await
            .context("building the Discord client")?;
        let client_task = tokio::spawn(async move { client.start().await });
        let lines = match tokio::time::timeout(limit, receiver).await {
            Ok(Ok(Ok(lines))) => lines,
            Ok(Ok(Err(error))) => {
                client_task.abort();
                return Ok(Outcome::fail(vec![format!("scenario failed: {error:#}")]));
            }
            Ok(Err(_)) => {
                client_task.abort();
                return Ok(Outcome::fail(vec![
                    "client stopped before the scenario finished".to_owned(),
                ]));
            }
            Err(_) => {
                client_task.abort();
                return Ok(Outcome::fail(vec![format!(
                    "no result within {limit:?} (gateway or voice never connected?)"
                )]));
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(5), client_task).await;
        let pass = lines.iter().any(|line| line == "VERDICT PASS");
        Ok(Outcome {
            pass,
            details: lines,
        })
    })
}

pub fn lifecycle(_args: &[String]) -> anyhow::Result<Outcome> {
    run(Scenario::Lifecycle, Duration::from_secs(60))
}

/// `discord-delivery [seconds] [--opus] [--codec <name>]`: a tone into the test channel, and
/// what Discord reports. `--opus` plays it through the app's output and Opus encoder, of the
/// codec `--codec` names (libopus without it).
pub fn delivery(args: &[String]) -> anyhow::Result<Outcome> {
    let codec = crate::codec_arg(args)?;
    let args = crate::without_codec_arg(args);
    let seconds = args.iter().find_map(|s| s.parse().ok()).unwrap_or(30);
    let opus = args.iter().any(|a| a == "--opus");
    run(
        Scenario::Delivery {
            seconds,
            opus,
            codec,
        },
        Duration::from_secs(seconds + 40),
    )
}

/// Pushes a 440 Hz tone into `output` one 20 ms frame at a time, on an absolute schedule
/// (a dedicated thread with `std::thread::sleep`, as the app's pacing loops), until
/// `closed` is set.
fn push_tone(output: OutputHandle, closed: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut phase = 0.0f32;
        let mut samples = vec![0i16; FRAME_SAMPLES];
        let start = Instant::now();
        let mut next = Duration::ZERO;
        while !closed.load(Ordering::Acquire) {
            for pair in samples.as_chunks_mut::<2>().0 {
                let value = ((phase * std::f32::consts::TAU).sin() * 0.2 * 32_767.0) as i16;
                phase = (phase + 440.0 / 48_000.0).fract();
                pair[0] = value;
                pair[1] = value;
            }
            output.push(Frame::from_slice(&samples));
            next += Duration::from_millis(20);
            if let Some(wait) = next.checked_sub(start.elapsed()) {
                std::thread::sleep(wait);
            }
        }
    })
}

pub fn receive_timing(args: &[String]) -> anyhow::Result<Outcome> {
    let seconds = args.first().and_then(|s| s.parse().ok()).unwrap_or(60);
    run(
        Scenario::Timing { seconds },
        Duration::from_secs(seconds + 40),
    )
}
