//! The realtime call over WebRTC inside the process: one Opus track out, one remote track
//! in, a data channel for the realtime server's events. Audio in comes from the gate's
//! `FrameReceiver` on a dedicated 20 ms thread; audio out goes to the `OutputHandle`.

use crate::audio::dsp::StereoDownsampler;
use crate::audio::{AudioPorts, Frame, FRAME_SAMPLES};
use bytes::Bytes;
use rtc::interceptor::Registry;
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use rtc::rtp_transceiver::PayloadType;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::default_runtime;

const FRAME: Duration = Duration::from_millis(20);
/// Mono samples per 20 ms at 48 kHz.
const MONO_FRAME: usize = 960;
const GATHER_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtcCounters {
    pub frames_sent: u64,
    pub packets_received: u64,
    pub decode_errors: u64,
}

struct Handler {
    gathered: watch::Sender<bool>,
    state: watch::Sender<String>,
    tracks: mpsc::UnboundedSender<Arc<dyn TrackRemote>>,
    channels: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathered.send(true);
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        log::info!("[rtc] connection {state}");
        let _ = self.state.send(format!("{state}"));
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let _ = self.channels.send(data_channel);
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let _ = self.tracks.send(track);
    }
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

/// One call: the peer connection, its outgoing track and the data channel.
pub struct RtcCall {
    pc: Arc<dyn PeerConnection>,
    track: Arc<TrackLocalStaticSample>,
    ssrc: u32,
    payload_type: PayloadType,
    state: watch::Receiver<String>,
    closed: Arc<AtomicBool>,
    counters: Arc<Counters>,
}

#[derive(Default)]
struct Counters {
    frames_sent: AtomicU64,
    packets_received: AtomicU64,
    decode_errors: AtomicU64,
}

impl RtcCall {
    /// Builds the peer, the track and the `oai-events` channel, creates the offer and waits
    /// for ICE gathering (at most 2 s). Returns the call and the offer SDP. The send and
    /// receive paths start at once: audio from `ports.input` flows as soon as the peer is
    /// connected, and remote audio goes to `ports.output`.
    pub async fn open(handle: Handle, ports: AudioPorts) -> Result<(Arc<Self>, String), String> {
        let runtime = default_runtime().ok_or_else(|| "no WebRTC runtime".to_owned())?;
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_codec(opus_codec(), RtpCodecKind::Audio)
            .map_err(|error| format!("register opus: {error}"))?;
        let registry = register_default_interceptors(Registry::new(), &mut media_engine)
            .map_err(|error| format!("interceptors: {error}"))?;
        let config = RTCConfigurationBuilder::new();
        let (gathered_tx, mut gathered) = watch::channel(false);
        let (state_tx, state) = watch::channel(String::from("new"));
        let (tracks_tx, mut tracks) = mpsc::unbounded_channel();
        let (channels_tx, _channels) = mpsc::unbounded_channel();
        let handler = Arc::new(Handler {
            gathered: gathered_tx,
            state: state_tx,
            tracks: tracks_tx,
            channels: channels_tx,
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
            .map_err(|error| format!("build peer connection: {error}"))?;
        let pc: Arc<dyn PeerConnection> = Arc::new(pc);

        let ssrc = rand::random::<u32>();
        let codec = opus_codec();
        let track = Arc::new(
            TrackLocalStaticSample::new(
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
            .map_err(|error| format!("audio track: {error}"))?,
        );
        let sender = pc
            .add_track(track.clone() as Arc<dyn TrackLocal>)
            .await
            .map_err(|error| format!("add_track: {error}"))?;
        let events = pc
            .create_data_channel("oai-events", None)
            .await
            .map_err(|error| format!("create_data_channel: {error}"))?;
        handle.spawn(async move {
            while let Some(event) = events.poll().await {
                match event {
                    DataChannelEvent::OnOpen => log::debug!("[rtc] oai-events open"),
                    DataChannelEvent::OnMessage(message) => {
                        let text = String::from_utf8_lossy(&message.data);
                        let kind = serde_json::from_str::<serde_json::Value>(&text)
                            .ok()
                            .and_then(|value| {
                                value
                                    .get("type")
                                    .and_then(|t| t.as_str())
                                    .map(str::to_owned)
                            })
                            .unwrap_or_else(|| "?".to_owned());
                        log::debug!("[rtc] oai-event {kind}");
                    }
                    DataChannelEvent::OnClose => {
                        log::debug!("[rtc] oai-events closed");
                        break;
                    }
                    _ => {}
                }
            }
        });

        let offer = pc
            .create_offer(None)
            .await
            .map_err(|error| format!("create_offer: {error}"))?;
        pc.set_local_description(offer)
            .await
            .map_err(|error| format!("set_local_description: {error}"))?;
        if !*gathered.borrow() {
            let _ = tokio::time::timeout(GATHER_TIMEOUT, async {
                while gathered.changed().await.is_ok() {
                    if *gathered.borrow() {
                        return;
                    }
                }
            })
            .await;
        }
        let offer = pc
            .local_description()
            .await
            .ok_or_else(|| "no local description".to_owned())?;
        let payload_type = sender
            .get_parameters()
            .await
            .map_err(|error| format!("sender parameters: {error}"))?
            .rtp_parameters
            .codecs
            .first()
            .map(|codec| codec.payload_type)
            .unwrap_or(111);

        let closed = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(Counters::default());
        let call = Arc::new(Self {
            pc,
            track,
            ssrc,
            payload_type,
            state: state.clone(),
            closed: Arc::clone(&closed),
            counters: Arc::clone(&counters),
        });

        // Receive path: decode every remote packet to 48 kHz mono, duplicate to stereo,
        // push 20 ms frames to the output queue.
        {
            let output = ports.output.clone();
            let closed = Arc::clone(&closed);
            let counters = Arc::clone(&counters);
            handle.spawn(async move {
                while let Some(track) = tracks.recv().await {
                    log::info!("[rtc] remote track; receiving the assistant's voice");
                    let output = output.clone();
                    let closed = Arc::clone(&closed);
                    let counters = Arc::clone(&counters);
                    tokio::spawn(read_remote_track(track, output, closed, counters));
                }
            });
        }

        // Send path: a dedicated thread paces the gate's frames into Opus packets.
        {
            let call = Arc::clone(&call);
            let input = ports.input.clone();
            let handle = handle.clone();
            let (packets_tx, mut packets) = mpsc::unbounded_channel::<Bytes>();
            {
                let call = Arc::clone(&call);
                handle.spawn(async move {
                    while let Some(packet) = packets.recv().await {
                        let written = call
                            .track
                            .sample_writer(call.ssrc, call.payload_type)
                            .write_sample(&Sample {
                                data: packet,
                                duration: FRAME,
                                ..Sample::new(Instant::now())
                            })
                            .await;
                        if written.is_err() {
                            break;
                        }
                        call.counters.frames_sent.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
            std::thread::Builder::new()
                .name("voicebro-rtc-send".to_owned())
                .spawn(move || send_loop(call, input, packets_tx))
                .map_err(|error| format!("could not start the send thread: {error}"))?;
        }

        Ok((call, offer.sdp))
    }

    pub async fn set_answer(&self, sdp: String) -> Result<(), String> {
        let answer =
            RTCSessionDescription::answer(sdp).map_err(|error| format!("answer sdp: {error}"))?;
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(|error| format!("set_remote_description: {error}"))
    }

    pub fn is_connected(&self) -> bool {
        self.state.borrow().as_str() == "connected"
    }

    pub fn state(&self) -> String {
        self.state.borrow().clone()
    }

    /// Resolves when the connection reaches `connected`, or fails/closes, or `limit`
    /// passes.
    pub async fn wait_connected(&self, limit: Duration) -> Result<Duration, String> {
        let started = Instant::now();
        let mut state = self.state.clone();
        if state.borrow().as_str() == "connected" {
            return Ok(Duration::ZERO);
        }
        tokio::time::timeout(limit, async {
            loop {
                state
                    .changed()
                    .await
                    .map_err(|_| "peer dropped".to_owned())?;
                let current = state.borrow().clone();
                if current == "connected" {
                    return Ok::<_, String>(());
                }
                if current == "failed" || current == "closed" {
                    return Err(format!("connection state {current}"));
                }
            }
        })
        .await
        .map_err(|_| format!("no connection within {limit:?}"))??;
        Ok(started.elapsed())
    }

    /// A receiver of the connection state, for the session's watcher.
    pub fn state_watch(&self) -> watch::Receiver<String> {
        self.state.clone()
    }

    pub fn counters(&self) -> RtcCounters {
        RtcCounters {
            frames_sent: self.counters.frames_sent.load(Ordering::Relaxed),
            packets_received: self.counters.packets_received.load(Ordering::Relaxed),
            decode_errors: self.counters.decode_errors.load(Ordering::Relaxed),
        }
    }

    pub async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.pc.close().await;
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// Reads the gate's frames every 20 ms, downmixes to mono, encodes Opus and hands the
/// packets to the async writer. Runs until the call is closed.
fn send_loop(
    call: Arc<RtcCall>,
    input: crate::audio::FrameReceiver,
    packets: mpsc::UnboundedSender<Bytes>,
) {
    let Ok(mut encoder) =
        opus2::Encoder::new(48_000, opus2::Channels::Mono, opus2::Application::Voip)
    else {
        log::error!("[rtc] could not create the Opus encoder");
        return;
    };
    let _ = encoder.set_inband_fec(true);
    let _ = encoder.set_complexity(5);
    let mut downsampler = StereoDownsampler::new(1);
    let mut packet = vec![0u8; 4000];
    let mut next = Instant::now();
    while !call.is_closed() {
        next += FRAME;
        if call.is_connected() {
            // Send the newest frame; older queued frames are stale.
            let mut frame = None;
            while let Some(next_frame) = input.try_recv() {
                frame = Some(next_frame);
            }
            if let Some(frame) = frame {
                let mono = downsampler.process(frame.as_slice());
                debug_assert_eq!(mono.len(), MONO_FRAME);
                if let Ok(written) = encoder.encode(&mono, &mut packet) {
                    if packets
                        .send(Bytes::copy_from_slice(&packet[..written]))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        } else {
            // Not connected yet: drop what the gate sent so it does not pile up.
            while input.try_recv().is_some() {}
        }
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(200) {
            next = now;
        }
    }
}

async fn read_remote_track(
    track: Arc<dyn TrackRemote>,
    output: crate::audio::OutputHandle,
    closed: Arc<AtomicBool>,
    counters: Arc<Counters>,
) {
    let Ok(mut decoder) = opus2::Decoder::new(48_000, opus2::Channels::Mono) else {
        log::error!("[rtc] could not create the Opus decoder");
        return;
    };
    let mut buffer = vec![0i16; MONO_FRAME * 6];
    let mut pending: Vec<i16> = Vec::with_capacity(FRAME_SAMPLES * 2);
    while let Some(event) = track.poll().await {
        if closed.load(Ordering::Acquire) {
            break;
        }
        match event {
            TrackRemoteEvent::OnRtpPacket(packet) => {
                counters.packets_received.fetch_add(1, Ordering::Relaxed);
                match decoder.decode(&packet.payload, &mut buffer, false) {
                    Ok(count) => {
                        for sample in &buffer[..count] {
                            pending.push(*sample);
                            pending.push(*sample);
                        }
                        while pending.len() >= FRAME_SAMPLES {
                            let frame = Frame::from_slice(&pending[..FRAME_SAMPLES]);
                            pending.drain(..FRAME_SAMPLES);
                            output.push(frame);
                        }
                    }
                    Err(_) => {
                        counters.decode_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            TrackRemoteEvent::OnEnded => break,
            _ => {}
        }
    }
}

/// Keeps a mutex-protected call slot readable without holding the lock across awaits.
pub type CallSlot = Mutex<Option<Arc<RtcCall>>>;
