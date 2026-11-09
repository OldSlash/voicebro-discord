//! The WebRTC call inside the Rust process.
//!
//! `rtc-loopback` connects two peer connections in one process and streams an Opus tone
//! from one to the other: it separates "we hold the crate wrong" from "the remote side
//! rejects us". `rtc-call` opens the same kind of connection against Codex realtime v3
//! through `thread/realtime/start` and checks that the canary instruction in the last
//! `initialItems` entry reaches the model.

use crate::codex::{self, AppServer};
use crate::opus::{self, FRAME_SAMPLES, SAMPLE_RATE};
use crate::Outcome;
use anyhow::{anyhow, bail, Context};
use bytes::Bytes;
use rtc::interceptor::Registry;
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use rtc::rtp_transceiver::PayloadType;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::rtp_transceiver::RtpSender;
use webrtc::runtime::{default_runtime, Runtime};

const FRAME: Duration = Duration::from_millis(20);
const GATHER_TIMEOUT: Duration = Duration::from_secs(2);
const SIGNAL_PEAK: i32 = 500;

// ---------------------------------------------------------------------------------------
// Peer connection plumbing shared by both probes
// ---------------------------------------------------------------------------------------

struct Handler {
    name: &'static str,
    gathered: watch::Sender<bool>,
    state: watch::Sender<String>,
    tracks: mpsc::UnboundedSender<Arc<dyn TrackRemote>>,
    channels: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
    log: mpsc::UnboundedSender<String>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        let _ = self
            .log
            .send(format!("[{}] ice gathering: {state:?}", self.name));
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathered.send(true);
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let _ = self
            .log
            .send(format!("[{}] connection: {state}", self.name));
        let _ = self.state.send(format!("{state}"));
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let _ = self.channels.send(data_channel);
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let _ = self.log.send(format!("[{}] remote track", self.name));
        let _ = self.tracks.send(track);
    }
}

struct Peer {
    pc: Arc<dyn PeerConnection>,
    gathered: watch::Receiver<bool>,
    state: watch::Receiver<String>,
    tracks: mpsc::UnboundedReceiver<Arc<dyn TrackRemote>>,
    channels: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
}

fn opus_codec() -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48_000,
            channels: 2,
            sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 111,
    }
}

fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            socket.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_owned())
}

async fn build_peer(
    name: &'static str,
    runtime: Arc<dyn Runtime>,
    stun: Option<&str>,
    log: mpsc::UnboundedSender<String>,
) -> anyhow::Result<Peer> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(opus_codec(), RtpCodecKind::Audio)
        .map_err(|error| anyhow!("register opus: {error}"))?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|error| anyhow!("interceptors: {error}"))?;
    let mut config = RTCConfigurationBuilder::new();
    if let Some(stun) = stun {
        config = config.with_ice_servers(vec![RTCIceServer {
            urls: vec![stun.to_owned()],
            ..Default::default()
        }]);
    }
    let (gathered_tx, gathered) = watch::channel(false);
    let (state_tx, state) = watch::channel(String::from("new"));
    let (tracks_tx, tracks) = mpsc::unbounded_channel();
    let (channels_tx, channels) = mpsc::unbounded_channel();
    let handler = Arc::new(Handler {
        name,
        gathered: gathered_tx,
        state: state_tx,
        tracks: tracks_tx,
        channels: channels_tx,
        log,
    });
    let pc = PeerConnectionBuilder::new()
        .with_configuration(config.build())
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{}:0", local_ip())])
        .build()
        .await
        .map_err(|error| anyhow!("build peer connection: {error}"))?;
    let pc: Arc<dyn PeerConnection> = Arc::new(pc);
    Ok(Peer {
        pc,
        gathered,
        state,
        tracks,
        channels,
    })
}

/// A send-only Opus track with its SSRC.
fn new_audio_track() -> anyhow::Result<(Arc<TrackLocalStaticSample>, u32)> {
    let ssrc = rand::random::<u32>();
    let codec = opus_codec();
    let track = TrackLocalStaticSample::new(
        Instant::now(),
        MediaStreamTrack::new(
            "voicebro-audio".to_owned(),
            "voicebro-audio-0".to_owned(),
            "audio".to_owned(),
            RtpCodecKind::Audio,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: codec.rtp_codec.clone(),
                ..Default::default()
            }],
        ),
    )
    .map_err(|error| anyhow!("audio track: {error}"))?;
    Ok((Arc::new(track), ssrc))
}

async fn negotiated_payload_type(sender: &Arc<dyn RtpSender>) -> anyhow::Result<PayloadType> {
    sender
        .get_parameters()
        .await
        .map_err(|error| anyhow!("sender parameters: {error}"))?
        .rtp_parameters
        .codecs
        .first()
        .map(|codec| codec.payload_type)
        .ok_or_else(|| anyhow!("sender has no negotiated codec"))
}

async fn wait_gathered(peer: &mut Peer) -> bool {
    if *peer.gathered.borrow() {
        return true;
    }
    timeout(GATHER_TIMEOUT, async {
        while peer.gathered.changed().await.is_ok() {
            if *peer.gathered.borrow() {
                return;
            }
        }
    })
    .await
    .is_ok()
}

async fn wait_state(peer: &mut Peer, wanted: &str, limit: Duration) -> anyhow::Result<Duration> {
    let started = Instant::now();
    if peer.state.borrow().as_str() == wanted {
        return Ok(started.elapsed());
    }
    timeout(limit, async {
        loop {
            peer.state
                .changed()
                .await
                .map_err(|_| anyhow!("peer dropped"))?;
            let state = peer.state.borrow().clone();
            if state == wanted {
                return Ok::<_, anyhow::Error>(());
            }
            if state == "failed" || state == "closed" {
                bail!("connection state {state}");
            }
        }
    })
    .await
    .map_err(|_| anyhow!("no `{wanted}` state within {limit:?}"))??;
    Ok(started.elapsed())
}

/// Decoded remote audio, shared between the track reader and the probe.
#[derive(Default)]
struct RemoteAudio {
    samples: Vec<i16>,
    packets: usize,
    decode_errors: usize,
    first_signal_at: Option<Instant>,
    last_signal_at: Option<Instant>,
}

fn spawn_track_reader(
    track: Arc<dyn TrackRemote>,
    sink: Arc<Mutex<RemoteAudio>>,
    log: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        let Ok(mut decoder) = opus2::Decoder::new(SAMPLE_RATE, opus2::Channels::Mono) else {
            let _ = log.send("remote decoder could not be created".to_owned());
            return;
        };
        let mut buffer = vec![0i16; FRAME_SAMPLES * 6];
        while let Some(event) = track.poll().await {
            match event {
                TrackRemoteEvent::OnRtpPacket(packet) => {
                    let decoded = decoder.decode(&packet.payload, &mut buffer, false);
                    let mut sink = sink.lock().expect("remote audio lock");
                    sink.packets += 1;
                    match decoded {
                        Ok(count) => {
                            let frame = &buffer[..count];
                            if opus::best_peak(frame) >= SIGNAL_PEAK {
                                let now = Instant::now();
                                sink.first_signal_at.get_or_insert(now);
                                sink.last_signal_at = Some(now);
                            }
                            sink.samples.extend_from_slice(frame);
                        }
                        Err(_) => sink.decode_errors += 1,
                    }
                }
                TrackRemoteEvent::OnEnded => {
                    let _ = log.send("remote track ended".to_owned());
                    break;
                }
                _ => {}
            }
        }
    });
}

fn spawn_channel_reader(
    channel: Arc<dyn DataChannel>,
    name: &'static str,
    log: mpsc::UnboundedSender<String>,
    messages: Arc<Mutex<Vec<String>>>,
) {
    tokio::spawn(async move {
        while let Some(event) = channel.poll().await {
            match event {
                DataChannelEvent::OnOpen => {
                    let _ = log.send(format!("[{name}] data channel open"));
                }
                DataChannelEvent::OnMessage(message) => {
                    let text = String::from_utf8_lossy(&message.data).to_string();
                    if let Ok(mut store) = messages.lock() {
                        store.push(text);
                    }
                }
                DataChannelEvent::OnClose => {
                    let _ = log.send(format!("[{name}] data channel closed"));
                    break;
                }
                _ => {}
            }
        }
    });
}

/// Encodes `pcm` (48 kHz mono) in 20 ms frames and writes them as paced samples.
async fn send_pcm(
    track: &TrackLocalStaticSample,
    ssrc: u32,
    payload_type: PayloadType,
    encoder: &mut opus2::Encoder,
    pcm: &[i16],
) -> anyhow::Result<usize> {
    let mut packet = vec![0u8; 4000];
    let mut ticker = tokio::time::interval(FRAME);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut frames = 0usize;
    let mut padded = Vec::new();
    for chunk in pcm.chunks(FRAME_SAMPLES) {
        let frame: &[i16] = if chunk.len() == FRAME_SAMPLES {
            chunk
        } else {
            padded.clear();
            padded.extend_from_slice(chunk);
            padded.resize(FRAME_SAMPLES, 0);
            &padded
        };
        let written = encoder.encode(frame, &mut packet).context("opus encode")?;
        track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: Bytes::copy_from_slice(&packet[..written]),
                duration: FRAME,
                ..Sample::new(Instant::now())
            })
            .await
            .map_err(|error| anyhow!("write_sample: {error}"))?;
        frames += 1;
        ticker.tick().await;
    }
    Ok(frames)
}

fn drain_log(log: &mut mpsc::UnboundedReceiver<String>, details: &mut Vec<String>) {
    while let Ok(line) = log.try_recv() {
        details.push(line);
    }
}

// ---------------------------------------------------------------------------------------
// Loopback: two peers in one process
// ---------------------------------------------------------------------------------------

pub fn loopback(_args: &[String]) -> anyhow::Result<Outcome> {
    let tokio_runtime = tokio::runtime::Runtime::new()?;
    tokio_runtime.block_on(async {
        let runtime = default_runtime().ok_or_else(|| anyhow!("no webrtc runtime"))?;
        let (log_tx, mut log) = mpsc::unbounded_channel();
        let mut details = Vec::new();

        let mut offerer = build_peer("A", runtime.clone(), None, log_tx.clone()).await?;
        let mut answerer = build_peer("B", runtime.clone(), None, log_tx.clone()).await?;

        let (track, ssrc) = new_audio_track()?;
        let sender = offerer
            .pc
            .add_track(track.clone() as Arc<dyn TrackLocal>)
            .await
            .map_err(|error| anyhow!("add_track: {error}"))?;
        let events = offerer
            .pc
            .create_data_channel("oai-events", None)
            .await
            .map_err(|error| anyhow!("create_data_channel: {error}"))?;
        let a_messages = Arc::new(Mutex::new(Vec::new()));
        spawn_channel_reader(events.clone(), "A", log_tx.clone(), a_messages);

        let offer = offerer
            .pc
            .create_offer(None)
            .await
            .map_err(|error| anyhow!("create_offer: {error}"))?;
        offerer
            .pc
            .set_local_description(offer)
            .await
            .map_err(|error| anyhow!("set_local_description(offer): {error}"))?;
        let gathered = wait_gathered(&mut offerer).await;
        let offer = offerer
            .pc
            .local_description()
            .await
            .ok_or_else(|| anyhow!("no local description"))?;
        details.push(format!(
            "offer: {} bytes, ice gathering complete={gathered}, m=audio {}, m=application {}",
            offer.sdp.len(),
            offer.sdp.contains("m=audio"),
            offer.sdp.contains("m=application")
        ));

        answerer
            .pc
            .set_remote_description(RTCSessionDescription::offer(offer.sdp.clone())
                .map_err(|error| anyhow!("offer sdp: {error}"))?)
            .await
            .map_err(|error| anyhow!("B set_remote_description: {error}"))?;
        let answer = answerer
            .pc
            .create_answer(None)
            .await
            .map_err(|error| anyhow!("create_answer: {error}"))?;
        answerer
            .pc
            .set_local_description(answer)
            .await
            .map_err(|error| anyhow!("B set_local_description: {error}"))?;
        let _ = wait_gathered(&mut answerer).await;
        let answer = answerer
            .pc
            .local_description()
            .await
            .ok_or_else(|| anyhow!("B has no local description"))?;
        offerer
            .pc
            .set_remote_description(RTCSessionDescription::answer(answer.sdp.clone())
                .map_err(|error| anyhow!("answer sdp: {error}"))?)
            .await
            .map_err(|error| anyhow!("A set_remote_description: {error}"))?;

        let connect_time = wait_state(&mut offerer, "connected", Duration::from_secs(10)).await?;
        details.push(format!("A connected after {connect_time:?}"));
        let _ = wait_state(&mut answerer, "connected", Duration::from_secs(10)).await?;

        // `on_track` fires with the first RTP packet, so the reader must be in place before
        // the tone is sent, not awaited first.
        let remote = Arc::new(Mutex::new(RemoteAudio::default()));
        let tracks_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        {
            let remote = Arc::clone(&remote);
            let log_tx = log_tx.clone();
            let tracks_seen = Arc::clone(&tracks_seen);
            let mut tracks = std::mem::replace(&mut answerer.tracks, mpsc::unbounded_channel().1);
            tokio::spawn(async move {
                while let Some(track) = tracks.recv().await {
                    tracks_seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    spawn_track_reader(track, Arc::clone(&remote), log_tx.clone());
                }
            });
        }

        let b_messages = Arc::new(Mutex::new(Vec::new()));
        if let Ok(Some(channel)) = timeout(Duration::from_secs(5), answerer.channels.recv()).await {
            details.push(format!("B data channel label={:?}", channel.label().await.ok()));
            spawn_channel_reader(channel, "B", log_tx.clone(), Arc::clone(&b_messages));
        } else {
            details.push("B received no data channel within 5 s".to_owned());
        }

        let payload_type = negotiated_payload_type(&sender).await?;
        let mut encoder = opus::new_encoder()?;
        let tone = opus::tone(2.0);
        let frames = send_pcm(&track, ssrc, payload_type, &mut encoder, &tone).await?;
        let _ = events.send_text("probe ping").await;
        tokio::time::sleep(Duration::from_millis(400)).await;

        let (samples, packets, errors) = {
            let remote = remote.lock().expect("remote lock");
            (remote.samples.clone(), remote.packets, remote.decode_errors)
        };
        let (corr, lag) = if samples.len() > FRAME_SAMPLES * 20 {
            opus::best_correlation(&tone[FRAME_SAMPLES * 5..], &samples[FRAME_SAMPLES * 5..], 9_600)
        } else {
            (0.0, 0)
        };
        details.push(format!(
            "sent {frames} frames (payload type {payload_type}); B saw {} remote track(s), received {packets} packets, {} samples ({:.2} s), {errors} decode errors",
            tracks_seen.load(std::sync::atomic::Ordering::Relaxed),
            samples.len(),
            samples.len() as f32 / SAMPLE_RATE as f32
        ));
        details.push(format!("tone correlation {corr:.3} at lag {lag} samples"));
        let b_text = b_messages.lock().map(|m| m.clone()).unwrap_or_default();
        details.push(format!("B data channel messages: {b_text:?}"));

        let _ = offerer.pc.close().await;
        let _ = answerer.pc.close().await;
        drain_log(&mut log, &mut details);

        let pass = corr > 0.8
            && samples.len() >= (SAMPLE_RATE as usize * 3 / 2)
            && b_text.iter().any(|m| m == "probe ping");
        details.push(
            "criterion: correlation > 0.8, at least 1.5 s decoded, data channel message delivered"
                .to_owned(),
        );
        Ok(Outcome { pass, details })
    })
}

// ---------------------------------------------------------------------------------------
// The real call to Codex realtime, with the canary instruction
// ---------------------------------------------------------------------------------------

const CANARY_WORD: &str = "банан";
const INSTRUCTIONS: &str = "Ты голосовой помощник в голосовом канале Discord. Отвечай коротко, одним-двумя предложениями, по-русски. Начинай каждый ответ со слова Банан.";

fn read_wav_48k_mono(path: &str) -> anyhow::Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("opening {path}"))?;
    let spec = reader.spec();
    if spec.sample_rate != SAMPLE_RATE {
        bail!("{path}: expected 48000 Hz, got {}", spec.sample_rate);
    }
    if spec.bits_per_sample != 16 {
        bail!(
            "{path}: expected 16-bit PCM, got {} bits",
            spec.bits_per_sample
        );
    }
    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    Ok(match spec.channels {
        1 => samples,
        2 => samples
            .chunks_exact(2)
            .map(|pair| ((pair[0] as i32 + pair[1] as i32) / 2) as i16)
            .collect(),
        other => bail!("{path}: {other} channels"),
    })
}

fn write_wav_48k_mono(path: &str, samples: &[i16]) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn normalize(text: &str) -> String {
    text.trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

pub fn call(args: &[String]) -> anyhow::Result<Outcome> {
    let input = args
        .first()
        .ok_or_else(|| anyhow!("usage: probe rtc-call <in.wav> <out.wav>"))?
        .clone();
    let output = args
        .get(1)
        .ok_or_else(|| anyhow!("usage: probe rtc-call <in.wav> <out.wav>"))?
        .clone();
    let pcm = read_wav_48k_mono(&input)?;
    let voice = std::env::var("VOICEBRO_PROBE_VOICE").unwrap_or_else(|_| "juniper".to_owned());
    let stun = std::env::var("VOICEBRO_PROBE_STUN").ok();

    let tokio_runtime = tokio::runtime::Runtime::new()?;
    tokio_runtime.block_on(async {
        let mut details = vec![format!(
            "input {input}: {:.2} s of 48 kHz mono",
            pcm.len() as f32 / SAMPLE_RATE as f32
        )];
        let executable = codex::find_codex().ok_or_else(|| anyhow!("codex not found"))?;
        let server = AppServer::spawn(&executable).await?;
        let mut notifications = server.subscribe();
        details.push(format!("codex {}", codex::codex_version(&executable)?));

        let workspace = codex::probe_workspace()?;
        let thread = server
            .request(
                "thread/start",
                codex::thread_start_params(&workspace, INSTRUCTIONS),
            )
            .await?;
        let thread_id = codex::thread_id(&thread)?;
        details.push(format!("thread {thread_id}"));

        // Peer connection, track and data channel before the offer.
        let runtime = default_runtime().ok_or_else(|| anyhow!("no webrtc runtime"))?;
        let (log_tx, mut log) = mpsc::unbounded_channel();
        let mut peer = build_peer("call", runtime, stun.as_deref(), log_tx.clone()).await?;
        let (track, ssrc) = new_audio_track()?;
        let sender = peer
            .pc
            .add_track(track.clone() as Arc<dyn TrackLocal>)
            .await
            .map_err(|error| anyhow!("add_track: {error}"))?;
        let events = peer
            .pc
            .create_data_channel("oai-events", None)
            .await
            .map_err(|error| anyhow!("create_data_channel: {error}"))?;
        let event_messages = Arc::new(Mutex::new(Vec::new()));
        spawn_channel_reader(events, "call", log_tx.clone(), Arc::clone(&event_messages));

        let offer = peer
            .pc
            .create_offer(None)
            .await
            .map_err(|error| anyhow!("create_offer: {error}"))?;
        peer.pc
            .set_local_description(offer)
            .await
            .map_err(|error| anyhow!("set_local_description: {error}"))?;
        let gathered = wait_gathered(&mut peer).await;
        let offer = peer
            .pc
            .local_description()
            .await
            .ok_or_else(|| anyhow!("no local description"))?;
        details.push(format!(
            "offer: {} bytes, gathering complete={gathered}, candidates={}",
            offer.sdp.len(),
            offer.sdp.matches("a=candidate").count()
        ));

        // Remote audio collector: tracks may arrive before or after `connected`.
        let remote = Arc::new(Mutex::new(RemoteAudio::default()));
        {
            let remote = Arc::clone(&remote);
            let log_tx = log_tx.clone();
            let mut tracks = std::mem::replace(&mut peer.tracks, mpsc::unbounded_channel().1);
            tokio::spawn(async move {
                while let Some(track) = tracks.recv().await {
                    spawn_track_reader(track, Arc::clone(&remote), log_tx.clone());
                }
            });
        }

        // Codex notifications: collect everything realtime-related.
        let transcripts: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let notes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let (sdp_tx, sdp_rx) = tokio::sync::oneshot::channel::<String>();
        let (assistant_done_tx, mut assistant_done_rx) = watch::channel(false);
        {
            let transcripts = Arc::clone(&transcripts);
            let notes = Arc::clone(&notes);
            let mut sdp_tx = Some(sdp_tx);
            tokio::spawn(async move {
                loop {
                    let (method, params) = match notifications.recv().await {
                        Ok(item) => item,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    };
                    match method.as_str() {
                        "thread/realtime/sdp" => {
                            if let (Some(tx), Some(sdp)) =
                                (sdp_tx.take(), params.get("sdp").and_then(Value::as_str))
                            {
                                let _ = tx.send(sdp.to_owned());
                            }
                        }
                        "thread/realtime/transcript/done" => {
                            let role = params
                                .get("role")
                                .and_then(Value::as_str)
                                .unwrap_or("?")
                                .to_owned();
                            let text = params
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            let is_assistant = role == "assistant";
                            if let Ok(mut store) = transcripts.lock() {
                                store.push((role, text));
                            }
                            if is_assistant {
                                let _ = assistant_done_tx.send(true);
                            }
                        }
                        "thread/realtime/transcript/delta" | "thread/realtime/outputAudio/delta" => {}
                        other => {
                            let compact = params.to_string();
                            let compact = if compact.len() > 300 {
                                format!("{}…", &compact[..300])
                            } else {
                                compact
                            };
                            if let Ok(mut store) = notes.lock() {
                                store.push(format!("{other} {compact}"));
                            }
                        }
                    }
                }
            });
        }

        let voice_session_id = format!("probe-{:x}", rand::random::<u64>());
        let start = Instant::now();
        let params = json!({
            "threadId": thread_id,
            "outputModality": "audio",
            "version": "v3",
            "voice": voice,
            "prompt": INSTRUCTIONS,
            "includeStartupContext": false,
            "voiceSessionId": voice_session_id,
            "initialItems": [
                { "role": "developer", "text": INSTRUCTIONS }
            ],
            "transport": { "type": "webrtc", "sdp": offer.sdp },
        });
        let started = server.request("thread/realtime/start", params).await;
        if let Err(error) = &started {
            details.push(format!("thread/realtime/start failed: {error:#}"));
            details.extend(server.stderr_tail(10));
            return Ok(Outcome::fail(details));
        }
        let answer = match timeout(Duration::from_secs(20), sdp_rx).await {
            Ok(Ok(sdp)) => sdp,
            _ => {
                details.push("no thread/realtime/sdp within 20 s".to_owned());
                drain_log(&mut log, &mut details);
                details.extend(notes.lock().map(|n| n.clone()).unwrap_or_default());
                details.extend(server.stderr_tail(10));
                return Ok(Outcome::fail(details));
            }
        };
        details.push(format!(
            "answer after {:?}: {} bytes, candidates: {}",
            start.elapsed(),
            answer.len(),
            answer
                .lines()
                .filter(|line| line.starts_with("a=candidate"))
                .map(|line| line.split_whitespace().take(5).collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        peer.pc
            .set_remote_description(
                RTCSessionDescription::answer(answer)
                    .map_err(|error| anyhow!("answer sdp: {error}"))?,
            )
            .await
            .map_err(|error| anyhow!("set_remote_description: {error}"))?;

        let connected = wait_state(&mut peer, "connected", Duration::from_secs(15)).await;
        drain_log(&mut log, &mut details);
        let connect_time = match connected {
            Ok(elapsed) => elapsed,
            Err(error) => {
                details.push(format!("connection failed: {error:#}"));
                details.extend(notes.lock().map(|n| n.clone()).unwrap_or_default());
                let _ = server
                    .request("thread/realtime/stop", json!({ "threadId": thread_id }))
                    .await;
                return Ok(Outcome::fail(details));
            }
        };
        details.push(format!("DTLS/ICE connected {connect_time:?} after the answer"));

        // Speak the question, then keep the stream alive with silence while listening.
        let payload_type = negotiated_payload_type(&sender).await?;
        let mut encoder = opus::new_encoder()?;
        let frames = send_pcm(&track, ssrc, payload_type, &mut encoder, &pcm).await?;
        let question_done = Instant::now();
        details.push(format!("sent the question: {frames} frames"));

        let silence = vec![0i16; FRAME_SAMPLES * 25]; // 500 ms per call
        let deadline = question_done + Duration::from_secs(45);
        let mut quiet_after_answer = false;
        loop {
            send_pcm(&track, ssrc, payload_type, &mut encoder, &silence).await?;
            let now = Instant::now();
            let (first, last) = {
                let remote = remote.lock().expect("remote lock");
                (remote.first_signal_at, remote.last_signal_at)
            };
            let assistant_done = *assistant_done_rx.borrow_and_update();
            if let (Some(_), Some(last)) = (first, last) {
                if assistant_done && now.duration_since(last) > Duration::from_millis(1_500) {
                    quiet_after_answer = true;
                    break;
                }
            }
            if now > deadline {
                break;
            }
        }

        let (samples, packets, errors, first_signal) = {
            let remote = remote.lock().expect("remote lock");
            (
                remote.samples.clone(),
                remote.packets,
                remote.decode_errors,
                remote.first_signal_at,
            )
        };
        details.push(format!(
            "remote audio: {packets} packets, {:.2} s decoded, {errors} decode errors, first voice {} after the question",
            samples.len() as f32 / SAMPLE_RATE as f32,
            first_signal
                .map(|at| format!("{:?}", at.saturating_duration_since(question_done)))
                .unwrap_or_else(|| "never".to_owned())
        ));
        write_wav_48k_mono(&output, &samples)?;
        details.push(format!("wrote {output}"));

        let _ = server
            .request("thread/realtime/stop", json!({ "threadId": thread_id }))
            .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = peer.pc.close().await;
        drain_log(&mut log, &mut details);

        let transcripts = transcripts.lock().map(|t| t.clone()).unwrap_or_default();
        for (role, text) in &transcripts {
            details.push(format!("transcript {role}: {text}"));
        }
        let assistant_text = transcripts
            .iter()
            .find(|(role, _)| role == "assistant")
            .map(|(_, text)| text.clone());
        let canary = assistant_text
            .as_deref()
            .map(|text| normalize(text).starts_with(CANARY_WORD))
            .unwrap_or(false);
        details.push(format!(
            "canary «Банан» at the start of the assistant transcript: {canary}"
        ));
        let event_count = event_messages.lock().map(|m| m.len()).unwrap_or(0);
        details.push(format!("oai-events messages received: {event_count}"));
        if let Ok(messages) = event_messages.lock() {
            for message in messages.iter().take(5) {
                let short = if message.len() > 200 { &message[..200] } else { message };
                details.push(format!("  event: {short}"));
            }
        }
        details.extend(
            notes
                .lock()
                .map(|n| n.iter().take(20).cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        );
        details.extend(server.stderr_tail(5).into_iter().map(|line| format!("codex stderr: {line}")));

        let has_voice = first_signal.is_some() && samples.len() > SAMPLE_RATE as usize / 2;
        details.push(format!(
            "criterion: connected={} remote voice={has_voice} quiet after answer={quiet_after_answer} canary={canary}",
            true
        ));
        Ok(Outcome {
            pass: has_voice && canary,
            details,
        })
    })
}
