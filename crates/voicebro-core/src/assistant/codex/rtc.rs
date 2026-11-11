//! The realtime call over WebRTC inside the process: one Opus track out, one remote track
//! in, a data channel for the realtime server's events. Audio toward the call comes from the
//! gate's `FrameReceiver`, every frame in order, on a dedicated 20 ms thread (see
//! [`super::pacing`]); the assistant's voice goes into a [`JitterBuffer`] that the output
//! pulls on songbird's clock.

use super::jitter::JitterBuffer;
use super::pacing::{Pick, SendPacer};
use super::playout::DepthEstimator;
use crate::audio::dsp::StereoDownsampler;
use crate::audio::{AudioPorts, Frame, FrameProvider, FrameReceiver, OutputHandle};
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
/// Encoder settings toward the call (design D4).
const ENCODER_COMPLEXITY: i32 = 10;
const ENCODER_BITRATE: i32 = 32_000;
const ENCODER_EXPECTED_LOSS: i32 = 5;

/// One call's audio counters, both directions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct CallAudioCounters {
    pub frames_sent: u64,
    /// Frames dropped before the send thread took them (more than 100 ms waiting).
    pub input_drops: u64,
    /// Ticks that sent silence because no frame was waiting, after audio had started.
    pub input_underruns: u64,
    pub packets_received: u64,
    pub late: u64,
    pub concealed: u64,
    pub concealed_fec: u64,
    pub playout_underruns: u64,
    /// Packets of the assistant's voice dropped because the buffer was full.
    pub output_drops: u64,
    pub decode_errors: u64,
    /// Time the playout added by lengthening pauses, in ms.
    pub stretched_pause_ms: u64,
    /// Time the playout added by lengthening voiced speech, in ms.
    pub stretched_speech_ms: u64,
    /// Time the playout removed from pauses and speech, in ms.
    pub removed_ms: u64,
    /// Frames bridged with concealment while the buffer was dry in speech.
    pub bridged_frames: u64,
    /// Dry spells in speech that bridging could not cover: heard as a gap.
    pub speech_gaps: u64,
    /// The playout depth now and at its highest, in ms.
    pub depth_ms: u64,
    pub depth_max_ms: u64,
}

impl CallAudioCounters {
    /// The one log line written when a call ends.
    pub fn log_line(&self, guild: u64) -> String {
        format!(
            "call audio for guild {guild}: frames sent {}, input drops {}, input underruns {}, packets received {}, late {}, concealed {} (FEC {}), playout underruns {}, output drops {}, decode errors {}, stretched pause {} ms, stretched speech {} ms, removed {} ms, bridged frames {}, speech gaps {}, depth {} ms (max {} ms)",
            self.frames_sent,
            self.input_drops,
            self.input_underruns,
            self.packets_received,
            self.late,
            self.concealed,
            self.concealed_fec,
            self.playout_underruns,
            self.output_drops,
            self.decode_errors,
            self.stretched_pause_ms,
            self.stretched_speech_ms,
            self.removed_ms,
            self.bridged_frames,
            self.speech_gaps,
            self.depth_ms,
            self.depth_max_ms
        )
    }
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

/// Where a call reports its counters when it ends: the guild for the log line, the slot the
/// session shows in its status after the call is gone, and what the call's playout learnt
/// about the server's lateness, which the next call of the guild starts from.
#[derive(Clone)]
pub struct CallReport {
    pub guild: u64,
    pub last: Arc<Mutex<Option<CallAudioCounters>>>,
    pub learned: Arc<Mutex<Option<DepthEstimator>>>,
}

impl CallReport {
    pub fn new(guild: u64) -> Self {
        Self::with_learned(guild, Arc::default())
    }

    /// A report whose learnt playout depth is kept in `learned` (the assistant keeps one per
    /// guild for the app's run, so it outlives the session).
    pub fn with_learned(guild: u64, learned: Arc<Mutex<Option<DepthEstimator>>>) -> Self {
        Self {
            guild,
            last: Arc::new(Mutex::new(None)),
            learned,
        }
    }
}

/// One call: the peer connection, its outgoing track and the data channel.
pub struct RtcCall {
    report: CallReport,
    pc: Arc<dyn PeerConnection>,
    track: Arc<TrackLocalStaticSample>,
    ssrc: u32,
    payload_type: PayloadType,
    state: watch::Receiver<String>,
    closed: Arc<AtomicBool>,
    counters: Arc<Counters>,
    jitter: Arc<JitterBuffer>,
    output: OutputHandle,
    input: FrameReceiver,
    /// The input queue's drop count when the call opened; the queue outlives calls.
    input_drops_at_open: u64,
}

#[derive(Default)]
struct Counters {
    frames_sent: AtomicU64,
    input_underruns: AtomicU64,
}

impl RtcCall {
    /// Builds the peer, the track and the `oai-events` channel, creates the offer and waits
    /// for ICE gathering (at most 2 s). Returns the call and the offer SDP. The send and
    /// receive paths start at once: audio from `ports.input` flows as soon as the peer is
    /// connected, and remote audio goes to `ports.output`.
    pub async fn open(
        handle: Handle,
        ports: AudioPorts,
        report: CallReport,
    ) -> Result<(Arc<Self>, String), String> {
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
        // The playout starts from what the guild's previous call learnt, or from the prior.
        let learned = report.learned.lock().expect("call report lock").clone();
        let (estimator, start) = match learned {
            Some(estimator) => (estimator, "learnt in the previous call"),
            None => (DepthEstimator::with_prior(), "initial"),
        };
        let jitter = Arc::new(JitterBuffer::with_estimator(true, estimator));
        crate::detail!(
            "rtc",
            "[rtc] call opens with a voice buffer of {} ms ({start})",
            jitter.depth_ms()
        );
        ports
            .output
            .attach_provider(Arc::clone(&jitter) as Arc<dyn FrameProvider>);
        let call = Arc::new(Self {
            report,
            pc,
            track,
            ssrc,
            payload_type,
            state: state.clone(),
            closed: Arc::clone(&closed),
            counters: Arc::clone(&counters),
            jitter: Arc::clone(&jitter),
            output: ports.output.clone(),
            input: ports.input.clone(),
            input_drops_at_open: ports.input.dropped(),
        });

        // Receive path: every remote RTP packet goes into the jitter buffer, which the
        // output pulls one 20 ms frame at a time on songbird's clock.
        {
            let closed = Arc::clone(&closed);
            let jitter = Arc::clone(&jitter);
            handle.spawn(async move {
                while let Some(track) = tracks.recv().await {
                    crate::detail!("rtc", "[rtc] remote track; receiving the assistant's voice");
                    let closed = Arc::clone(&closed);
                    let jitter = Arc::clone(&jitter);
                    tokio::spawn(read_remote_track(track, jitter, closed));
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

    pub fn counters(&self) -> CallAudioCounters {
        let jitter = self.jitter.counters();
        CallAudioCounters {
            frames_sent: self.counters.frames_sent.load(Ordering::Relaxed),
            input_drops: self
                .input
                .dropped()
                .saturating_sub(self.input_drops_at_open),
            input_underruns: self.counters.input_underruns.load(Ordering::Relaxed),
            packets_received: jitter.packets_received,
            late: jitter.late,
            concealed: jitter.concealed,
            concealed_fec: jitter.concealed_fec,
            playout_underruns: jitter.playout_underruns,
            output_drops: jitter.overflow_drops,
            decode_errors: jitter.decode_errors,
            stretched_pause_ms: jitter.stretched_pause_ms,
            stretched_speech_ms: jitter.stretched_speech_ms,
            removed_ms: jitter.removed_ms,
            bridged_frames: jitter.bridged_frames,
            speech_gaps: jitter.speech_gaps,
            depth_ms: jitter.depth_ms,
            depth_max_ms: jitter.depth_max_ms,
        }
    }

    /// Ends the call: the send thread and the track reader stop, the output stops pulling
    /// from this call's jitter buffer, and the call's counters go to the log (one line) and
    /// to the report slot. Closing twice reports once.
    pub async fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            let counters = self.counters();
            crate::detail!("rtc", "[rtc] {}", counters.log_line(self.report.guild));
            *self.report.last.lock().expect("call report lock") = Some(counters);
            *self.report.learned.lock().expect("call report lock") = Some(self.jitter.learned());
            self.output.detach_provider();
            self.jitter.reset();
        }
        let _ = self.pc.close().await;
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// The Opus encoder toward the call: mono VOIP with in-band FEC (design D4).
pub(crate) fn new_encoder() -> Option<opus2::Encoder> {
    let mut encoder =
        opus2::Encoder::new(48_000, opus2::Channels::Mono, opus2::Application::Voip).ok()?;
    let _ = encoder.set_complexity(ENCODER_COMPLEXITY);
    let _ = encoder.set_bitrate(opus2::Bitrate::Bits(ENCODER_BITRATE));
    let _ = encoder.set_inband_fec(true);
    let _ = encoder.set_packet_loss_perc(ENCODER_EXPECTED_LOSS);
    Some(encoder)
}

/// Sends one packet every 20 ms while connected: the oldest frame the gate passed, or
/// silence when none waits (after a 40 ms prefill, see [`SendPacer`]). Runs until the call
/// is closed.
fn send_loop(call: Arc<RtcCall>, input: FrameReceiver, packets: mpsc::UnboundedSender<Bytes>) {
    let Some(mut encoder) = new_encoder() else {
        log::error!("[rtc] could not create the Opus encoder");
        return;
    };
    let mut downsampler = StereoDownsampler::new(1);
    let mut pacer = SendPacer::new();
    let silence = Frame::silence();
    let mut packet = vec![0u8; 4000];
    let mut next = Instant::now();
    while !call.is_closed() {
        next += FRAME;
        if call.is_connected() {
            let frame = match pacer.pick(input.len()) {
                Pick::Frame => input.try_recv(),
                Pick::Silence { underrun } => {
                    if underrun {
                        call.counters
                            .input_underruns
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    None
                }
            };
            let mono = downsampler.process(frame.as_ref().unwrap_or(&silence).as_slice());
            debug_assert_eq!(mono.len(), MONO_FRAME);
            if let Ok(written) = encoder.encode(&mono, &mut packet) {
                if packets
                    .send(Bytes::copy_from_slice(&packet[..written]))
                    .is_err()
                {
                    break;
                }
            }
        } else {
            // Not connected (yet or any more): drop what the gate sent so it does not
            // pile up, and prefill again once connected.
            while input.try_recv().is_some() {}
            pacer.reset();
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
    jitter: Arc<JitterBuffer>,
    closed: Arc<AtomicBool>,
) {
    while let Some(event) = track.poll().await {
        if closed.load(Ordering::Acquire) {
            break;
        }
        match event {
            TrackRemoteEvent::OnRtpPacket(packet) => {
                jitter.insert(
                    packet.header.sequence_number,
                    packet.header.timestamp,
                    packet.payload,
                );
            }
            TrackRemoteEvent::OnEnded => break,
            _ => {}
        }
    }
}

/// Keeps a mutex-protected call slot readable without holding the lock across awaits.
pub type CallSlot = Mutex<Option<Arc<RtcCall>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{frame_channel, output_pair, INPUT_CAPACITY};
    use std::io::Read;

    #[test]
    fn the_end_of_call_line_names_every_counter() {
        let counters = CallAudioCounters {
            frames_sent: 500,
            input_drops: 1,
            input_underruns: 2,
            packets_received: 480,
            late: 3,
            concealed: 4,
            concealed_fec: 1,
            playout_underruns: 5,
            output_drops: 6,
            decode_errors: 7,
            stretched_pause_ms: 820,
            stretched_speech_ms: 41,
            removed_ms: 60,
            bridged_frames: 9,
            speech_gaps: 1,
            depth_ms: 220,
            depth_max_ms: 360,
        };
        let line = counters.log_line(42);
        for part in [
            "guild 42",
            "frames sent 500",
            "input drops 1",
            "input underruns 2",
            "packets received 480",
            "late 3",
            "concealed 4 (FEC 1)",
            "playout underruns 5",
            "output drops 6",
            "decode errors 7",
            "stretched pause 820 ms",
            "stretched speech 41 ms",
            "removed 60 ms",
            "bridged frames 9",
            "speech gaps 1",
            "depth 220 ms (max 360 ms)",
        ] {
            assert!(line.contains(part), "{part} missing in {line}");
        }
    }

    /// The far end of the loopback: a plain peer that answers, records what it hears and
    /// plays a tone back.
    struct Far {
        tracks: mpsc::UnboundedSender<Arc<dyn TrackRemote>>,
        state: watch::Sender<String>,
        gathered: watch::Sender<bool>,
    }

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for Far {
        async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
            if state == RTCIceGatheringState::Complete {
                let _ = self.gathered.send(true);
            }
        }
        async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
            let _ = self.state.send(format!("{state}"));
        }
        async fn on_track(&self, track: Arc<dyn TrackRemote>) {
            let _ = self.tracks.send(track);
        }
    }

    fn tone(samples: usize, hz: f32) -> Vec<i16> {
        (0..samples)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                ((2.0 * std::f32::consts::PI * hz * t).sin() * 12_000.0) as i16
            })
            .collect()
    }

    /// The best normalized cross-correlation of `decoded` against `reference` within
    /// `max_lag` samples.
    fn correlation(reference: &[i16], decoded: &[i16], max_lag: usize) -> f64 {
        let len = reference.len().min(decoded.len()).saturating_sub(max_lag);
        let mut best = 0f64;
        for lag in (0..max_lag).step_by(4) {
            let (mut dot, mut a2, mut b2) = (0f64, 0f64, 0f64);
            for index in 0..len {
                let a = reference[index] as f64;
                let b = decoded[index + lag] as f64;
                dot += a * b;
                a2 += a * a;
                b2 += b * b;
            }
            if a2 > 0.0 && b2 > 0.0 {
                best = best.max(dot / (a2.sqrt() * b2.sqrt()));
            }
        }
        best
    }

    /// Both directions through real peers on this machine, with a jittery producer on the
    /// send side. Gated by `VOICEBRO_RTC_LOOPBACK=1` because it runs for about 12 s.
    #[test]
    fn loopback_call_loses_nothing_when_enabled() {
        if std::env::var("VOICEBRO_RTC_LOOPBACK").ok().as_deref() != Some("1") {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let handle = runtime.handle().clone();
        let (sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, mut source) = output_pair();
        let ports = AudioPorts {
            input,
            output: output.clone(),
        };
        let report = CallReport::new(0);
        let seconds = 10usize;
        let near_tone = tone(48_000 * seconds, 440.0);
        let far_tone = tone(48_000 * seconds, 660.0);

        let (call, far_pc, far_track, far_ssrc, far_payload, mut far_tracks) =
            runtime.block_on(async {
                let (call, offer) = RtcCall::open(handle.clone(), ports, report.clone())
                    .await
                    .unwrap();
                let mut media_engine = MediaEngine::default();
                media_engine
                    .register_codec(opus_codec(), RtpCodecKind::Audio)
                    .unwrap();
                let registry =
                    register_default_interceptors(Registry::new(), &mut media_engine).unwrap();
                let (tracks_tx, tracks) = mpsc::unbounded_channel();
                let (state_tx, _state) = watch::channel(String::new());
                let (gathered_tx, mut gathered) = watch::channel(false);
                let pc = PeerConnectionBuilder::new()
                    .with_configuration(RTCConfigurationBuilder::new().build())
                    .with_media_engine(media_engine)
                    .with_interceptor_registry(registry)
                    .with_handler(Arc::new(Far {
                        tracks: tracks_tx,
                        state: state_tx,
                        gathered: gathered_tx,
                    }))
                    .with_runtime(default_runtime().unwrap())
                    .with_udp_addrs(vec![format!("{}:0", local_ip())])
                    .build()
                    .await
                    .unwrap();
                let pc: Arc<dyn PeerConnection> = Arc::new(pc);
                let ssrc = rand::random::<u32>();
                let track = Arc::new(
                    TrackLocalStaticSample::new(
                        Instant::now(),
                        MediaStreamTrack::new(
                            "far".to_owned(),
                            "far-0".to_owned(),
                            "audio".to_owned(),
                            RtpCodecKind::Audio,
                            vec![RTCRtpEncodingParameters {
                                rtp_coding_parameters: RTCRtpCodingParameters {
                                    ssrc: Some(ssrc),
                                    ..Default::default()
                                },
                                codec: opus_codec().rtp_codec,
                                ..Default::default()
                            }],
                        ),
                    )
                    .unwrap(),
                );
                let rtp_sender = pc
                    .add_track(track.clone() as Arc<dyn TrackLocal>)
                    .await
                    .unwrap();
                pc.set_remote_description(RTCSessionDescription::offer(offer).unwrap())
                    .await
                    .unwrap();
                let answer = pc.create_answer(None).await.unwrap();
                pc.set_local_description(answer).await.unwrap();
                let _ = tokio::time::timeout(GATHER_TIMEOUT, async {
                    while !*gathered.borrow() {
                        if gathered.changed().await.is_err() {
                            break;
                        }
                    }
                })
                .await;
                let answer = pc.local_description().await.unwrap();
                call.set_answer(answer.sdp).await.unwrap();
                call.wait_connected(Duration::from_secs(10)).await.unwrap();
                let payload = rtp_sender
                    .get_parameters()
                    .await
                    .unwrap()
                    .rtp_parameters
                    .codecs
                    .first()
                    .map(|codec| codec.payload_type)
                    .unwrap_or(111);
                (call, pc, track, ssrc, payload, tracks)
            });

        // The far end records our audio.
        let heard = Arc::new(Mutex::new(Vec::<i16>::new()));
        {
            let heard = Arc::clone(&heard);
            runtime.spawn(async move {
                if let Some(track) = far_tracks.recv().await {
                    let mut decoder = opus2::Decoder::new(48_000, opus2::Channels::Mono).unwrap();
                    let mut buffer = vec![0i16; 5_760];
                    while let Some(event) = track.poll().await {
                        if let TrackRemoteEvent::OnRtpPacket(packet) = event {
                            if let Ok(count) = decoder.decode(&packet.payload, &mut buffer, false) {
                                heard.lock().unwrap().extend_from_slice(&buffer[..count]);
                            }
                        }
                    }
                }
            });
        }
        // The far end plays its tone back, paced by the runtime.
        {
            let far_tone = far_tone.clone();
            runtime.spawn(async move {
                let mut encoder = new_encoder().unwrap();
                let mut packet = vec![0u8; 4000];
                let mut ticker = tokio::time::interval(FRAME);
                for chunk in far_tone.chunks(MONO_FRAME) {
                    ticker.tick().await;
                    let written = encoder.encode(chunk, &mut packet).unwrap();
                    let _ = far_track
                        .sample_writer(far_ssrc, far_payload)
                        .write_sample(&Sample {
                            data: Bytes::copy_from_slice(&packet[..written]),
                            duration: FRAME,
                            ..Sample::new(Instant::now())
                        })
                        .await;
                }
            });
        }
        // Songbird's side: read the output every 20 ms on a thread.
        let played = Arc::new(Mutex::new(Vec::<i16>::new()));
        let reader = {
            let played = Arc::clone(&played);
            std::thread::spawn(move || {
                let mut bytes = vec![0u8; FRAME_SAMPLES_BYTES];
                let mut next = Instant::now();
                for _ in 0..(seconds * 50 + 100) {
                    next += FRAME;
                    let mut filled = 0;
                    while filled < bytes.len() {
                        filled += source.read(&mut bytes[filled..]).unwrap();
                    }
                    let mut store = played.lock().unwrap();
                    for pair in bytes.as_chunks::<8>().0 {
                        let left = f32::from_le_bytes([pair[0], pair[1], pair[2], pair[3]]);
                        store.push((left * 32_767.0) as i16);
                    }
                    drop(store);
                    if let Some(rest) = next.checked_duration_since(Instant::now()) {
                        std::thread::sleep(rest);
                    }
                }
            })
        };
        // The gate's side: our tone, one frame per 20 ms with -7..7 ms of jitter.
        let mut seed = 11u32;
        let started = Instant::now();
        for (index, chunk) in near_tone.chunks(MONO_FRAME).enumerate() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let jitter = ((seed >> 16) % 15) as i64 - 7;
            let due = started + Duration::from_millis((20 * index as i64 + 10 + jitter) as u64);
            if let Some(rest) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(rest);
            }
            let mut stereo = Vec::with_capacity(MONO_FRAME * 2);
            for sample in chunk {
                stereo.push(*sample);
                stereo.push(*sample);
            }
            sender.send(Frame::from_slice(&stereo));
        }
        // Wait out the 40 ms prefill and the frames still queued, then the stream has run
        // without a hole while the producer was running.
        std::thread::sleep(Duration::from_millis(80));
        let while_producing = call.counters();
        reader.join().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let counters = call.counters();
        runtime.block_on(async {
            call.close().await;
            let _ = far_pc.close().await;
        });
        let reported = report
            .last
            .lock()
            .unwrap()
            .expect("close reported the counters");
        assert!(reported.frames_sent >= counters.frames_sent);
        assert_eq!(reported.packets_received, counters.packets_received);
        let heard = heard.lock().unwrap().clone();
        let played = played.lock().unwrap().clone();
        eprintln!("{}", counters.log_line(0));
        eprintln!(
            "far end heard {:.2} s, we played {:.2} s",
            heard.len() as f64 / 48_000.0,
            played.len() as f64 / 48_000.0
        );
        assert_eq!(counters.input_drops, 0, "no frame dropped toward the call");
        assert!(
            while_producing.input_underruns <= 2,
            "no hole while the producer ran: {}",
            while_producing.input_underruns
        );
        assert_eq!(counters.late, 0);
        assert_eq!(counters.concealed, 0);
        assert_eq!(counters.speech_gaps, 0, "no gap heard on a clean network");
        let skip = 48_000;
        let heard_corr = correlation(&near_tone[skip..skip + 96_000], &heard[skip..], 9_600);
        let played_start = played
            .iter()
            .position(|s| s.unsigned_abs() > 1_000)
            .unwrap_or(0);
        let played_corr = correlation(
            &far_tone[skip..skip + 96_000],
            &played[played_start + skip - 4_800..],
            9_600,
        );
        eprintln!("correlation: far end {heard_corr:.3}, our playback {played_corr:.3}");
        assert!(
            heard_corr > 0.9,
            "what the far end heard matches the tone: {heard_corr}"
        );
        assert!(
            played_corr > 0.9,
            "what we played matches the far tone: {played_corr}"
        );
    }

    const FRAME_SAMPLES_BYTES: usize = crate::audio::FRAME_SAMPLES * 4;
}
