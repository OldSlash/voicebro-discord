//! The assistant's voice toward Discord as Opus packets the app encodes itself (change
//! `discord-voice-delivery`).
//!
//! songbird's own encoder carries no in-band FEC, so a packet lost on the way to Discord or
//! to a listener can only be concealed by the listener's client. Here each 20 ms of the
//! output ([`PcmSource`], with playout, gain, mute and earcons already applied) is encoded
//! with FEC whose strength follows the loss Discord reports on our stream, and handed to
//! songbird as a live Opus track. songbird copies such packets into the RTP payload
//! unchanged (passthrough) when the track is the only one playing; [`DeliveryShared`]
//! counts how often it decoded them instead, which means it refused passthrough.

use crate::audio::codec::{
    Application, Channels, CodecFailures, CodecPath, EncoderParams, FailureSpell, OpusCodec,
    VoiceEncoder,
};
use crate::audio::output::PcmSource;
use crate::audio::FRAME_SAMPLES;
use songbird::input::codecs::{get_codec_registry, get_probe};
use songbird::input::{Input, LiveInput, Parsed};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use symphonia_core::audio::AudioBufferRef;
use symphonia_core::codecs::{
    CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult,
};
use symphonia_core::errors::{Error as SymphError, Result as SymphResult, SeekErrorKind};
use symphonia_core::formats::{
    Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track,
};
use symphonia_core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia_core::meta::{Metadata, MetadataLog, MetadataOptions};
use symphonia_core::probe::Hint;

/// Mono samples per 20 ms frame (one Opus packet).
pub const FRAME: usize = FRAME_SAMPLES / 2;
/// The encoder's bitrate: songbird's own, which the FEC test (task 3.1) showed still
/// carries in-band FEC in VOIP mode once a loss is expected.
pub const BITRATE: i32 = 128_000;
/// The expected loss while Discord has reported nothing on our stream (the first second of
/// a call, or if its reports stop): the worst second seen in the probe of 2026-11-10 was
/// 12.5 %, so a single lost packet in ten is covered.
pub const DEFAULT_EXPECTED_LOSS: u8 = 10;
const COMPLEXITY: i32 = 10;
/// Room for one encoded packet; at 128 kbit/s a 20 ms packet is about 320 bytes.
const MAX_PACKET_BYTES: usize = 4_000;

/// How much a loss report still counts after each packet we send: the memory of the voice
/// playout's depth estimator (about 600 packets, 12 s), so a lossy second is remembered
/// about as long as a late burst is there. Counted in our packets, not in reports, so it
/// does not depend on how often Discord reports.
const LOSS_FORGETTING_PER_PACKET: f64 = 0.9983;

/// Where the expected loss comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LossSource {
    /// Nothing reported on our stream yet, or the reports have stopped long enough ago that
    /// the default outweighs them.
    Default,
    /// Discord's reports on our stream.
    Measured,
}

/// What the Opus output and songbird's fallback share with Discord's reports and the
/// status: packets encoded, encode errors, packets songbird decoded (passthrough refused),
/// the reported loss and the expected loss in use with its source.
#[derive(Debug)]
pub struct DeliveryShared {
    pub packets_encoded: AtomicU64,
    pub encode_errors: AtomicU64,
    pub songbird_decodes: AtomicU64,
    /// The encoder's expected loss in use, in percent.
    pub expected_loss: AtomicU32,
    /// Whether the expected loss in use comes from Discord's reports (else the default).
    pub measured: AtomicBool,
    /// Report blocks about our stream so far.
    pub reports: AtomicU64,
    /// The last report's fraction lost (of 256) and interarrival jitter (48 kHz units).
    pub last_fraction_lost: AtomicU32,
    pub last_jitter: AtomicU32,
    /// The loss envelope at the last report (percent, `f64` bits) and the packets encoded
    /// then; read on the mixer thread, written only by [`DeliveryShared::on_report`].
    envelope_bits: AtomicU64,
    envelope_at: AtomicU64,
}

impl Default for DeliveryShared {
    fn default() -> Self {
        Self {
            packets_encoded: AtomicU64::new(0),
            encode_errors: AtomicU64::new(0),
            songbird_decodes: AtomicU64::new(0),
            expected_loss: AtomicU32::new(u32::from(DEFAULT_EXPECTED_LOSS)),
            measured: AtomicBool::new(false),
            reports: AtomicU64::new(0),
            last_fraction_lost: AtomicU32::new(0),
            last_jitter: AtomicU32::new(0),
            envelope_bits: AtomicU64::new(f64::from(DEFAULT_EXPECTED_LOSS).to_bits()),
            envelope_at: AtomicU64::new(0),
        }
    }
}

impl DeliveryShared {
    /// Packets songbird sent as we encoded them.
    pub fn passed_through(&self) -> u64 {
        self.packets_encoded
            .load(Ordering::Relaxed)
            .saturating_sub(self.songbird_decodes.load(Ordering::Relaxed))
    }

    /// A report block about our stream: the fraction lost since the previous report (of
    /// 256) and the interarrival jitter (48 kHz units). The loss envelope rises at once to a
    /// higher report and otherwise moves toward the report by the forgetting of the packets
    /// sent since the previous one. It starts from the default, so the first reports pull
    /// it down gradually. Called by one task at a time (the call's event handler).
    pub fn on_report(&self, fraction_lost: u8, jitter: u32) {
        let reported = f64::from(fraction_lost) * 100.0 / 256.0;
        let now = self.packets_encoded.load(Ordering::Relaxed);
        let since = now.saturating_sub(self.envelope_at.load(Ordering::Relaxed));
        let kept = forgetting(since);
        let previous = f64::from_bits(self.envelope_bits.load(Ordering::Relaxed));
        let envelope = reported.max(kept * previous + (1.0 - kept) * reported);
        self.envelope_bits
            .store(envelope.to_bits(), Ordering::Relaxed);
        self.envelope_at.store(now, Ordering::Relaxed);
        self.last_fraction_lost
            .store(u32::from(fraction_lost), Ordering::Relaxed);
        self.last_jitter.store(jitter, Ordering::Relaxed);
        self.reports.fetch_add(1, Ordering::Release);
    }

    /// The expected loss to encode with now, in whole percent, and its source. Without
    /// reports it is the default; after the last report the envelope returns toward the
    /// default by the same forgetting, and counts as measured while the report still
    /// outweighs the default.
    pub fn expected_loss_now(&self) -> (u8, LossSource) {
        if self.reports.load(Ordering::Acquire) == 0 {
            return (DEFAULT_EXPECTED_LOSS, LossSource::Default);
        }
        let since = self
            .packets_encoded
            .load(Ordering::Relaxed)
            .saturating_sub(self.envelope_at.load(Ordering::Relaxed));
        let kept = forgetting(since);
        let envelope = f64::from_bits(self.envelope_bits.load(Ordering::Relaxed));
        let loss = kept * envelope + (1.0 - kept) * f64::from(DEFAULT_EXPECTED_LOSS);
        let source = if kept >= 0.5 {
            LossSource::Measured
        } else {
            LossSource::Default
        };
        (loss.round().clamp(0.0, 100.0) as u8, source)
    }
}

/// The weight a report keeps after `packets` of ours.
fn forgetting(packets: u64) -> f64 {
    LOSS_FORGETTING_PER_PACKET.powf(packets as f64)
}

/// The encoder's parameters toward Discord: 48 kHz stereo, VOIP, 128 kbit/s, complexity
/// 10, in-band FEC sized for `expected_loss`.
pub fn discord_params(expected_loss: u8) -> EncoderParams {
    EncoderParams {
        channels: Channels::Stereo,
        application: Application::Voip,
        bitrate: BITRATE,
        complexity: COMPLEXITY,
        inband_fec: true,
        loss_percent: expected_loss,
    }
}

/// The encoder toward Discord ([`discord_params`]) of the codec selected at the join, the
/// expected loss set from the measurement.
pub struct DiscordEncoder {
    encoder: Option<VoiceEncoder>,
    out: Vec<u8>,
    expected_loss: u8,
    /// One 20 ms packet of silence, encoded at the start, sent when encoding fails.
    silence: Vec<u8>,
    spell: FailureSpell,
    /// Where a panic of the codec is reported (the join's; none in tests).
    failures: CodecFailures,
}

impl DiscordEncoder {
    pub fn new(codec: OpusCodec, expected_loss: u8) -> Self {
        let mut encoder = match VoiceEncoder::new(codec, discord_params(expected_loss)) {
            Ok(encoder) => Some(encoder),
            Err(error) => {
                log::error!("[delivery] {error}");
                None
            }
        };
        let mut out = vec![0u8; MAX_PACKET_BYTES];
        let silence = encoder
            .as_mut()
            .and_then(|encoder| {
                let zeros = [0f32; FRAME_SAMPLES];
                encoder.encode_f32(&zeros, &mut out).ok()
            })
            .and_then(|written| out.get(..written).map(<[u8]>::to_vec))
            // The Opus silence frame (TOC for 20 ms CELT, no data) if even that failed.
            .unwrap_or_else(|| vec![0xf8, 0xff, 0xfe]);
        Self {
            encoder,
            out,
            expected_loss,
            silence,
            spell: FailureSpell::default(),
            failures: CodecFailures::none(),
        }
    }

    /// Reports a panic of the codec through `failures` (design D5).
    pub fn with_failures(mut self, failures: CodecFailures) -> Self {
        self.failures = failures;
        self
    }

    /// The codec in use; `None` when its encoder could not be built.
    pub fn codec(&self) -> Option<OpusCodec> {
        self.encoder.as_ref().map(VoiceEncoder::codec)
    }

    pub fn set_expected_loss(&mut self, percent: u8) {
        if percent != self.expected_loss {
            self.expected_loss = percent;
            if let Some(encoder) = self.encoder.as_mut() {
                let _ = encoder.set_loss_percent(percent);
            }
        }
    }

    pub fn expected_loss(&self) -> u8 {
        self.expected_loss
    }

    /// Encodes one 20 ms frame of f32 stereo; `None` when the encoder fails (the failure
    /// is logged once per spell).
    pub fn encode(&mut self, pcm: &[f32]) -> Option<&[u8]> {
        let encoder = self.encoder.as_mut()?;
        match encoder.encode_f32(pcm, &mut self.out) {
            Ok(written) => {
                self.spell.succeeded("delivery");
                self.out.get(..written)
            }
            Err(error) => {
                self.spell.failed("delivery", &error);
                self.failures.report(CodecPath::DiscordOut, &error);
                None
            }
        }
    }

    /// The silence packet sent in place of a frame that could not be encoded.
    pub fn silence(&self) -> &[u8] {
        &self.silence
    }

    /// Makes the codec panic on the next frame (pure-Rust codecs only).
    #[cfg(test)]
    fn panic_next_for_test(&mut self) {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.panic_next_for_test();
        }
    }
}

/// Builds the songbird input that plays `source` as Opus packets encoded by the app with
/// `codec`. Gives `source` back when songbird cannot describe an Opus track (it always can;
/// the probe is fixed), so the caller can fall back to songbird's PCM path. A codec panic is
/// reported nowhere; a join uses [`live_input_reporting`].
pub fn live_input(
    source: PcmSource,
    shared: Arc<DeliveryShared>,
    codec: OpusCodec,
) -> Result<Input, PcmSource> {
    live_input_reporting(source, shared, codec, CodecFailures::none())
}

/// [`live_input`] whose encoder reports a codec panic through `failures`.
pub fn live_input_reporting(
    source: PcmSource,
    shared: Arc<DeliveryShared>,
    codec: OpusCodec,
    failures: CodecFailures,
) -> Result<Input, PcmSource> {
    let Some((track, metadata, decoder)) = opus_track() else {
        return Err(source);
    };
    let track_id = track.id;
    let format = OpusOutput::new(source, track, Arc::clone(&shared), codec, failures);
    Ok(Input::Live(
        LiveInput::Parsed(Parsed {
            format: Box::new(format),
            decoder: Box::new(CountingDecoder {
                inner: decoder,
                shared,
            }),
            track_id,
            meta: metadata,
            supports_backseek: false,
        }),
        None,
    ))
}

/// songbird's description of a 48 kHz stereo Opus track with 20 ms frames, its probed
/// metadata, and songbird's Opus decoder for it. The metadata type has no public
/// constructor, so a minimal DCA header (songbird's Opus container) is probed in memory.
fn opus_track() -> Option<(
    Track,
    symphonia_core::probe::ProbedMetadata,
    Box<dyn Decoder>,
)> {
    let meta = br#"{"dca":{"version":1,"tool":{"name":"VoiceBro","version":"1"}},"opus":{"mode":"voip","sample_rate":48000,"frame_size":960,"vbr":true,"channels":2}}"#;
    let mut header = b"DCA1".to_vec();
    header.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    header.extend_from_slice(meta);
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(header)),
        MediaSourceStreamOptions::default(),
    );
    let probed = get_probe()
        .format(
            &Hint::new(),
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let track = probed.format.default_track()?.clone();
    let decoder = get_codec_registry()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;
    Some((track, probed.metadata, decoder))
}

/// The output as a symphonia format: one Opus track, one packet per 20 ms, pulled by
/// songbird's mixer.
struct OpusOutput {
    source: PcmSource,
    /// A format reader must be `Sync` and the Opus encoder is only `Send`; the mutex is
    /// reached through `get_mut`, so it never locks.
    encoder: Mutex<DiscordEncoder>,
    track: Track,
    shared: Arc<DeliveryShared>,
    pcm: Vec<f32>,
    timestamp: u64,
    cues: Vec<Cue>,
    metadata: MetadataLog,
    /// songbird decodes seen at the last packet, and whether a refusal is being reported.
    decodes_seen: u64,
    refused: bool,
    packets_since_decode: u64,
}

impl OpusOutput {
    fn new(
        source: PcmSource,
        track: Track,
        shared: Arc<DeliveryShared>,
        codec: OpusCodec,
        failures: CodecFailures,
    ) -> Self {
        let (expected, _) = shared.expected_loss_now();
        Self {
            source,
            encoder: Mutex::new(DiscordEncoder::new(codec, expected).with_failures(failures)),
            track,
            shared,
            pcm: vec![0.0; FRAME_SAMPLES],
            timestamp: 0,
            cues: Vec::new(),
            metadata: MetadataLog::default(),
            decodes_seen: 0,
            refused: false,
            packets_since_decode: 0,
        }
    }

    /// Logs once when songbird starts decoding our packets (passthrough refused), and once
    /// when it has passed a second's worth through again.
    fn watch_passthrough(&mut self) {
        let decodes = self.shared.songbird_decodes.load(Ordering::Relaxed);
        if decodes > self.decodes_seen {
            self.decodes_seen = decodes;
            self.packets_since_decode = 0;
            if !self.refused {
                self.refused = true;
                log::warn!(
                    "songbird is re-encoding the assistant's voice (passthrough refused): no FEC toward Discord"
                );
            }
        } else {
            self.packets_since_decode += 1;
            if self.refused && self.packets_since_decode >= 50 {
                self.refused = false;
                log::info!("songbird passes the assistant's voice through again");
            }
        }
    }
}

impl FormatReader for OpusOutput {
    fn try_new(_: MediaSourceStream, _: &FormatOptions) -> SymphResult<Self> {
        Err(SymphError::Unsupported("the Opus output is built directly"))
    }

    fn cues(&self) -> &[Cue] {
        &self.cues
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn seek(&mut self, _: SeekMode, _: SeekTo) -> SymphResult<SeekedTo> {
        Err(SymphError::SeekError(SeekErrorKind::Unseekable))
    }

    fn tracks(&self) -> &[Track] {
        std::slice::from_ref(&self.track)
    }

    fn next_packet(&mut self) -> SymphResult<Packet> {
        if !self.source.fill_frame(&mut self.pcm) {
            return Err(SymphError::IoError(std::io::Error::from(
                std::io::ErrorKind::UnexpectedEof,
            )));
        }
        self.watch_passthrough();
        let (expected, source) = self.shared.expected_loss_now();
        self.shared
            .expected_loss
            .store(u32::from(expected), Ordering::Relaxed);
        self.shared
            .measured
            .store(source == LossSource::Measured, Ordering::Relaxed);
        let encoder = self
            .encoder
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        encoder.set_expected_loss(expected);
        let packet = encode_packet(
            encoder,
            &self.pcm,
            &self.shared,
            self.track.id,
            self.timestamp,
        );
        self.timestamp += FRAME as u64;
        Ok(packet)
    }

    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        MediaSourceStream::new(
            Box::new(Cursor::new(Vec::<u8>::new())),
            MediaSourceStreamOptions::default(),
        )
    }
}

/// One packet for songbird: `pcm` encoded, or the silence packet (counted as an encode
/// error) when encoding fails.
fn encode_packet(
    encoder: &mut DiscordEncoder,
    pcm: &[f32],
    shared: &DeliveryShared,
    track_id: u32,
    timestamp: u64,
) -> Packet {
    shared.packets_encoded.fetch_add(1, Ordering::Relaxed);
    if let Some(bytes) = encoder.encode(pcm) {
        return Packet::new_from_slice(track_id, timestamp, FRAME as u64, bytes);
    }
    shared.encode_errors.fetch_add(1, Ordering::Relaxed);
    Packet::new_from_slice(track_id, timestamp, FRAME as u64, encoder.silence())
}

/// songbird's Opus decoder, counting the packets it decodes: songbird decodes only when it
/// refuses to pass a packet through.
struct CountingDecoder {
    inner: Box<dyn Decoder>,
    shared: Arc<DeliveryShared>,
}

impl Decoder for CountingDecoder {
    fn try_new(_: &CodecParameters, _: &DecoderOptions) -> SymphResult<Self> {
        Err(SymphError::Unsupported(
            "the counting decoder is built directly",
        ))
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[]
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn codec_params(&self) -> &CodecParameters {
        self.inner.codec_params()
    }

    fn decode(&mut self, packet: &Packet) -> SymphResult<AudioBufferRef<'_>> {
        self.shared.songbird_decodes.fetch_add(1, Ordering::Relaxed);
        self.inner.decode(packet)
    }

    fn finalize(&mut self) -> FinalizeResult {
        self.inner.finalize()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.inner.last_decoded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{output_pair, Frame};
    use std::time::Instant;

    /// A speech-like stereo signal (both channels equal): a voiced tone whose pitch glides
    /// and whose loudness rises and falls like syllables, `frames` × 20 ms, in -1..1.
    fn speech_like(frames: usize) -> Vec<f32> {
        let mut phase = 0.0f32;
        let mut out = Vec::with_capacity(frames * FRAME_SAMPLES);
        for n in 0..frames * FRAME {
            let t = n as f32 / 48_000.0;
            let f0 = 120.0 + 40.0 * (2.0 * std::f32::consts::PI * 1.3 * t).sin();
            phase += f0 / 48_000.0;
            let p = 2.0 * std::f32::consts::PI * phase;
            let envelope = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin().abs();
            let value = envelope
                * (0.30 * p.sin()
                    + 0.18 * (2.0 * p).sin()
                    + 0.10 * (3.0 * p).sin()
                    + 0.06 * (5.0 * p).sin());
            out.push(value);
            out.push(value);
        }
        out
    }

    fn error_energy(a: &[i16], b: &[i16]) -> f64 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2))
            .sum()
    }

    #[test]
    fn a_lost_packet_is_rebuilt_from_the_next_ones_fec() {
        for codec in OpusCodec::ALL {
            a_lost_packet_is_rebuilt_from_the_next_ones_fec_with(codec);
        }
    }

    /// Encoded by `codec`, decoded by libopus as a listener's client does.
    fn a_lost_packet_is_rebuilt_from_the_next_ones_fec_with(codec: OpusCodec) {
        let pcm = speech_like(60);
        let mut encoder = DiscordEncoder::new(codec, DEFAULT_EXPECTED_LOSS);
        let packets: Vec<Vec<u8>> = pcm
            .as_chunks::<FRAME_SAMPLES>()
            .0
            .iter()
            .map(|frame| encoder.encode(frame).expect("encodes").to_vec())
            .collect();
        let average = packets.iter().map(Vec::len).sum::<usize>() / packets.len();
        let lost = 30;
        // The reference: every packet decoded.
        let mut decoder = opus2::Decoder::new(48_000, opus2::Channels::Stereo).unwrap();
        let mut frame = vec![0i16; FRAME_SAMPLES];
        let mut reference = Vec::new();
        for packet in &packets[..=lost] {
            decoder.decode(packet, &mut frame, false).unwrap();
            reference = frame.clone();
        }
        // Packet `lost` missing, rebuilt from the next packet's FEC, or concealed.
        let rebuilt = |fec: bool| {
            let mut decoder = opus2::Decoder::new(48_000, opus2::Channels::Stereo).unwrap();
            let mut frame = vec![0i16; FRAME_SAMPLES];
            for packet in &packets[..lost] {
                decoder.decode(packet, &mut frame, false).unwrap();
            }
            if fec {
                decoder
                    .decode(&packets[lost + 1], &mut frame, true)
                    .unwrap();
            } else {
                decoder.decode(&[], &mut frame, false).unwrap();
            }
            frame
        };
        let with_fec = error_energy(&rebuilt(true), &reference);
        let concealed = error_energy(&rebuilt(false), &reference);
        let gain_db = 10.0 * (concealed / with_fec.max(1.0)).log10();
        eprintln!(
            "{codec}, bitrate {BITRATE}: {average} bytes per packet; lost frame error: FEC {with_fec:.3e}, concealment {concealed:.3e}, FEC better by {gain_db:.1} dB"
        );
        assert!(
            gain_db >= 6.0,
            "{codec}: FEC rebuilds the frame far better than concealment"
        );
    }

    #[test]
    fn the_output_plays_as_opus_packets_songbird_can_pass_through() {
        for codec in OpusCodec::ALL {
            the_output_plays_as_opus_packets_with(codec);
        }
    }

    fn the_output_plays_as_opus_packets_with(codec: OpusCodec) {
        let (handle, source) = output_pair();
        let pcm = speech_like(40);
        let frames: Vec<Frame> = pcm
            .as_chunks::<FRAME_SAMPLES>()
            .0
            .iter()
            .map(|frame| {
                let samples: Vec<i16> = frame.iter().map(|s| (s * 32_767.0) as i16).collect();
                Frame::from_slice(&samples)
            })
            .collect();
        let shared = Arc::new(DeliveryShared::default());
        let Ok(Input::Live(LiveInput::Parsed(mut parsed), _)) =
            live_input(source, Arc::clone(&shared), codec)
        else {
            panic!("a parsed live input");
        };
        assert_eq!(
            parsed.decoder.codec_params().codec,
            symphonia_core::codecs::CODEC_TYPE_OPUS
        );
        let mut decoder = opus2::Decoder::new(48_000, opus2::Channels::Stereo).unwrap();
        let mut decoded = Vec::new();
        let mut buffer = vec![0i16; FRAME_SAMPLES];
        for frame in &frames {
            // One frame arrives per 20 ms, as from a call or the queue.
            handle.push(frame.clone());
            let packet = parsed.format.next_packet().expect("a packet");
            assert_eq!(packet.track_id(), parsed.track_id);
            let bytes = packet.buf();
            // songbird's passthrough test: one 20 ms Opus packet.
            assert_eq!(opus2::packet::get_nb_samples(bytes, 48_000).unwrap(), FRAME);
            let count = decoder.decode(bytes, &mut buffer, false).unwrap();
            decoded.extend_from_slice(&buffer[..count * 2]);
        }
        assert_eq!(shared.packets_encoded.load(Ordering::Relaxed), 40);
        assert_eq!(shared.songbird_decodes.load(Ordering::Relaxed), 0);
        assert_eq!(shared.passed_through(), 40);
        // The decoded stream is the pushed voice, delayed by the output's priming and the
        // codec's lookahead: find the lag, then compare.
        let original: Vec<i16> = pcm.iter().map(|s| (s * 32_767.0) as i16).collect();
        let window = &original[FRAME_SAMPLES * 5..FRAME_SAMPLES * 25];
        let (lag, snr) = (0..FRAME_SAMPLES * 5)
            .step_by(2)
            .map(|lag| {
                let other = &decoded[FRAME_SAMPLES * 5 + lag..FRAME_SAMPLES * 25 + lag];
                let signal: f64 = window.iter().map(|s| f64::from(*s).powi(2)).sum();
                (
                    lag,
                    10.0 * (signal / error_energy(window, other).max(1.0)).log10(),
                )
            })
            .fold(
                (0, f64::MIN),
                |best, item| if item.1 > best.1 { item } else { best },
            );
        // VOIP mode codes speech parametrically (SILK, hybrid) and does not keep the waveform,
        // so compare what it does keep: the loudness of each 20 ms, aligned by the lag.
        let rms = |samples: &[i16]| -> Vec<f64> {
            samples
                .as_chunks::<FRAME_SAMPLES>()
                .0
                .iter()
                .map(|frame| {
                    (frame.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / frame.len() as f64)
                        .sqrt()
                })
                .collect()
        };
        let source_levels = rms(window);
        let decoded_levels = rms(&decoded[FRAME_SAMPLES * 5 + lag..FRAME_SAMPLES * 25 + lag]);
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let (ms, md) = (mean(&source_levels), mean(&decoded_levels));
        let covariance: f64 = source_levels
            .iter()
            .zip(&decoded_levels)
            .map(|(a, b)| (a - ms) * (b - md))
            .sum();
        let spread = |v: &[f64], m: f64| v.iter().map(|x| (x - m).powi(2)).sum::<f64>().sqrt();
        let correlation = covariance / (spread(&source_levels, ms) * spread(&decoded_levels, md));
        let level_db = 20.0 * (md / ms).log10();
        eprintln!(
            "{codec} decoded by libopus against the source: lag {lag} samples, waveform SNR {snr:.1} dB, loudness correlation {correlation:.3}, level {level_db:+.1} dB"
        );
        assert!(
            correlation >= 0.95,
            "{codec}: the packets carry the output's sound"
        );
        assert!(level_db.abs() <= 1.5, "{codec}: at the output's level");
    }

    #[test]
    fn songbird_reads_byte_for_byte_what_the_encoder_made() {
        for codec in OpusCodec::ALL {
            songbird_reads_byte_for_byte_with(codec);
        }
    }

    fn songbird_reads_byte_for_byte_with(codec: OpusCodec) {
        // Two outputs fed the same frames: one read through songbird's parsed input, the
        // other filled and encoded by hand with an encoder of the same settings.
        let (handle, source) = output_pair();
        let (twin_handle, mut twin) = output_pair();
        let shared = Arc::new(DeliveryShared::default());
        let Ok(Input::Live(LiveInput::Parsed(mut parsed), _)) =
            live_input(source, Arc::clone(&shared), codec)
        else {
            panic!("a parsed live input");
        };
        let mut encoder = DiscordEncoder::new(codec, DEFAULT_EXPECTED_LOSS);
        let mut pcm = vec![0.0f32; FRAME_SAMPLES];
        let voice = speech_like(30);
        for chunk in voice.as_chunks::<FRAME_SAMPLES>().0 {
            let samples: Vec<i16> = chunk.iter().map(|s| (s * 32_767.0) as i16).collect();
            handle.push(Frame::from_slice(&samples));
            twin_handle.push(Frame::from_slice(&samples));
            let packet = parsed.format.next_packet().expect("a packet");
            assert!(twin.fill_frame(&mut pcm));
            assert_eq!(
                packet.buf(),
                encoder.encode(&pcm).expect("encodes"),
                "{codec}"
            );
        }
        handle.close();
        assert!(
            parsed.format.next_packet().is_err(),
            "a closed output ends the track"
        );
    }

    /// The cost of one 20 ms pull of the output with our encode, against the PCM pull alone
    /// plus the encode songbird did before (`Application::Audio`, 128 kbit/s, no FEC) on the
    /// same frames. Gated by `VOICEBRO_BENCH=1`; run in release.
    #[test]
    fn output_pull_cost_when_enabled() {
        if std::env::var("VOICEBRO_BENCH").ok().as_deref() != Some("1") {
            return;
        }
        let frames = 3_000;
        let voice: Vec<Frame> = speech_like(frames)
            .as_chunks::<FRAME_SAMPLES>()
            .0
            .iter()
            .map(|chunk| {
                let samples: Vec<i16> = chunk.iter().map(|s| (s * 32_767.0) as i16).collect();
                Frame::from_slice(&samples)
            })
            .collect();
        let stats = |costs: &mut Vec<f64>| {
            costs.sort_by(f64::total_cmp);
            let mean = costs.iter().sum::<f64>() / costs.len() as f64;
            let p99 = costs[(costs.len() - 1) * 99 / 100];
            (mean, p99, costs[costs.len() - 1])
        };

        // Ours: songbird's `next_packet` = PCM fill + Opus VOIP with FEC, by the codec of
        // `VOICEBRO_OPUS_CODEC` (libopus when unset).
        let codec = OpusCodec::from_env().expect("VOICEBRO_OPUS_CODEC");
        eprintln!("codec: {}", codec.label());
        let (handle, source) = output_pair();
        let shared = Arc::new(DeliveryShared::default());
        let Ok(Input::Live(LiveInput::Parsed(mut parsed), _)) =
            live_input(source, Arc::clone(&shared), codec)
        else {
            panic!("a parsed live input");
        };
        let mut ours = Vec::with_capacity(frames);
        for frame in &voice {
            handle.push(frame.clone());
            let began = Instant::now();
            let packet = parsed.format.next_packet().expect("a packet");
            ours.push(began.elapsed().as_secs_f64() * 1e6);
            std::hint::black_box(packet);
        }

        // Before: the PCM fill, then songbird's own encode of the mix.
        let (handle, mut source) = output_pair();
        let mut theirs_encoder =
            opus2::Encoder::new(48_000, opus2::Channels::Stereo, opus2::Application::Audio)
                .expect("encoder");
        theirs_encoder
            .set_bitrate(opus2::Bitrate::Bits(BITRATE))
            .expect("bitrate");
        let mut pcm = vec![0.0f32; FRAME_SAMPLES];
        let mut out = vec![0u8; MAX_PACKET_BYTES];
        let mut fill_only = Vec::with_capacity(frames);
        let mut before = Vec::with_capacity(frames);
        for frame in &voice {
            handle.push(frame.clone());
            let began = Instant::now();
            assert!(source.fill_frame(&mut pcm));
            let filled = began.elapsed().as_secs_f64() * 1e6;
            let written = theirs_encoder
                .encode_float(&pcm, &mut out)
                .expect("encodes");
            before.push(began.elapsed().as_secs_f64() * 1e6);
            fill_only.push(filled);
            std::hint::black_box(written);
        }

        let (ours_mean, ours_p99, ours_max) = stats(&mut ours);
        let (before_mean, before_p99, before_max) = stats(&mut before);
        let (fill_mean, fill_p99, _) = stats(&mut fill_only);
        eprintln!(
            "per 20 ms over {frames} frames: ours (fill + VOIP FEC encode) mean {ours_mean:.1} us, p99 {ours_p99:.1} us, max {ours_max:.1} us; before (fill + songbird's Audio encode) mean {before_mean:.1} us, p99 {before_p99:.1} us, max {before_max:.1} us; fill alone mean {fill_mean:.1} us, p99 {fill_p99:.1} us"
        );
        eprintln!(
            "passthrough: {} encoded, {} decoded by songbird, so songbird's own encode is not done",
            shared.packets_encoded.load(Ordering::Relaxed),
            shared.songbird_decodes.load(Ordering::Relaxed)
        );
        assert_eq!(shared.songbird_decodes.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_frame_that_cannot_be_encoded_becomes_counted_silence() {
        for codec in OpusCodec::ALL {
            let mut encoder = DiscordEncoder::new(codec, DEFAULT_EXPECTED_LOSS);
            assert_eq!(encoder.codec(), Some(codec));
            assert_eq!(
                opus2::packet::get_nb_samples(encoder.silence(), 48_000).unwrap(),
                FRAME,
                "{codec}: the silence packet is 20 ms"
            );
            let shared = DeliveryShared::default();
            // Seven samples are not a frame: the encoder refuses them.
            let packet = encode_packet(&mut encoder, &[0.0; 7], &shared, 1, 0);
            assert_eq!(packet.buf(), encoder.silence(), "{codec}");
            assert_eq!(shared.encode_errors.load(Ordering::Relaxed), 1, "{codec}");
            assert_eq!(shared.packets_encoded.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn a_codec_panic_sends_counted_silence_reports_once_and_the_next_frame_is_encoded() {
        let voice = speech_like(3);
        let frames: Vec<&[f32]> = voice.chunks(FRAME_SAMPLES).collect();
        let codec = OpusCodec::OpusRs;
        let (hub, mut reports) = crate::audio::codec::CodecFailureHub::new();
        let mut encoder =
            DiscordEncoder::new(codec, DEFAULT_EXPECTED_LOSS).with_failures(hub.for_guild(5));
        let shared = DeliveryShared::default();
        encoder.panic_next_for_test();
        let packet = encode_packet(&mut encoder, frames[0], &shared, 1, 0);
        assert_eq!(packet.buf(), encoder.silence(), "{codec}");
        assert_eq!(shared.encode_errors.load(Ordering::Relaxed), 1, "{codec}");
        let report = reports.try_recv().expect("one report");
        assert_eq!(
            (report.guild, report.path, report.codec),
            (5, CodecPath::DiscordOut, codec)
        );
        let packet = encode_packet(&mut encoder, frames[1], &shared, 1, FRAME as u64);
        assert_ne!(packet.buf(), encoder.silence(), "{codec}: a fresh encoder");
        assert_eq!(shared.encode_errors.load(Ordering::Relaxed), 1, "{codec}");
        encoder.panic_next_for_test();
        let _ = encode_packet(&mut encoder, frames[2], &shared, 1, 2 * FRAME as u64);
        assert!(
            reports.try_recv().is_err(),
            "a second panic of the join is not reported"
        );
        assert_eq!(shared.packets_encoded.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn songbird_decoding_our_packets_counts_as_passthrough_refused() {
        let (_handle, source) = output_pair();
        let shared = Arc::new(DeliveryShared::default());
        let Ok(Input::Live(LiveInput::Parsed(mut parsed), _)) =
            live_input(source, Arc::clone(&shared), OpusCodec::Libopus)
        else {
            panic!("a parsed live input");
        };
        for _ in 0..3 {
            let packet = parsed.format.next_packet().expect("a packet");
            let _ = parsed.decoder.decode(&packet);
        }
        let _ = parsed.format.next_packet().expect("a packet");
        assert_eq!(shared.songbird_decodes.load(Ordering::Relaxed), 3);
        assert_eq!(shared.passed_through(), 1);
    }
}
