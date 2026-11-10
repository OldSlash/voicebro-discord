//! P0.7 and P0.8: songbird lifecycle and receive-path timing.
//!
//! Both probes join a real voice channel (token, guild and channel from the environment).
//! `discord-lifecycle` registers a counting `VoiceTick` handler and a silent output track,
//! closes the track's source, removes the call, rejoins with a second handler, and checks
//! that the first handler is silent and the track reported its end. `receive-timing` sits in
//! the channel for a minute, does the real handler's work (mix every speaker into `i32`,
//! clamp) and reports tick-interval and handler-duration percentiles.

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
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use symphonia_core::io::MediaSource;
use tokio::sync::oneshot;

const FRAME_SAMPLES: usize = 1920; // 20 ms, 48 kHz stereo

struct Env {
    token: String,
    guild: GuildId,
    channel: ChannelId,
}

fn env() -> anyhow::Result<Env> {
    let var = |name: &str| std::env::var(name).map_err(|_| anyhow!("{name} is not set"));
    Ok(Env {
        token: var("VOICEBRO_DISCORD_TOKEN")?,
        guild: GuildId::new(
            var("VOICEBRO_TEST_GUILD")?
                .trim()
                .parse()
                .context("guild id")?,
        ),
        channel: ChannelId::new(
            var("VOICEBRO_TEST_CHANNEL")?
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
    Timing { seconds: u64 },
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

pub fn receive_timing(args: &[String]) -> anyhow::Result<Outcome> {
    let seconds = args.first().and_then(|s| s.parse().ok()).unwrap_or(60);
    run(
        Scenario::Timing { seconds },
        Duration::from_secs(seconds + 40),
    )
}
