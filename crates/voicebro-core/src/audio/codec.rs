//! The Opus codecs the app can encode and decode with (change `opus-codec-experiment`):
//! libopus through `opus2`, and the pure-Rust opus-rs, behind one encoder and one decoder
//! type. See `docs/ARCHITECTURE.md` 4.1. The comparison that chose opus-rs also ran ropus
//! and mousiki (`docs/OPUS-CODEC-COMPARISON.md`); they are not built in.
//!
//! Both codecs get the same parameters for the same path ([`EncoderParams`]). Calls into
//! opus-rs run under `catch_unwind`: a panic comes back as [`CodecError::Panicked`] and the
//! codec is rebuilt before its next use. libopus returns errors and C does not unwind, so its
//! calls are not wrapped. Release builds unwind (the workspace's release profile), so this
//! works there too. A panic is reported once per join ([`CodecFailures`]) and ends that
//! server's join; the desktop shell's panic hook ends the app on any panic outside a
//! guarded call ([`in_guarded_call`]).

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Every path runs Opus at 48 kHz.
pub const SAMPLE_RATE: u32 = 48_000;
/// The longest Opus packet, 120 ms at 48 kHz, in samples per channel.
pub const MAX_FRAME_SAMPLES: usize = 5_760;
/// The variable that picks the codec for the tests and probes that encode or decode.
pub const CODEC_ENV: &str = "VOICEBRO_OPUS_CODEC";

/// opus-rs's version, as pinned in `voicebro-core`'s manifest.
const OPUS_RS_VERSION: &str = "0.1.42";

/// One of the two Opus codecs built into the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OpusCodec {
    /// libopus 1.5, built from C by `libopus_sys` (the default).
    #[default]
    Libopus,
    /// opus-rs, a pure-Rust port of libopus 1.6 (float).
    OpusRs,
}

impl OpusCodec {
    pub const ALL: [Self; 2] = [Self::Libopus, Self::OpusRs];

    /// The name used in the settings, the variable and the probes.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Libopus => "libopus",
            Self::OpusRs => "opus-rs",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|codec| codec.name() == name)
    }

    /// The names, for messages: `libopus, opus-rs`.
    pub fn names() -> String {
        Self::ALL
            .iter()
            .map(|codec| codec.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Name and version, for the status and the log: `libopus 1.5`, `opus-rs 0.1.42`.
    /// `libopus_sys` 0.3.3 builds its Opus 1.5 sources without a version file, so libopus
    /// reports "libopus unknown"; the label then names the version the crate ships.
    pub fn label(self) -> String {
        match self {
            Self::Libopus => {
                let reported = opus2::version();
                if reported.contains("unknown") {
                    "libopus 1.5".to_owned()
                } else {
                    reported.to_owned()
                }
            }
            Self::OpusRs => format!("opus-rs {OPUS_RS_VERSION}"),
        }
    }

    pub const fn is_pure_rust(self) -> bool {
        !matches!(self, Self::Libopus)
    }

    /// The codec named by `VOICEBRO_OPUS_CODEC`; libopus when it is not set. An unknown
    /// name is an error that lists the two.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var(CODEC_ENV) {
            Ok(name) if !name.trim().is_empty() => Self::parse(name.trim()).ok_or_else(|| {
                format!(
                    "{CODEC_ENV}={name} is not a codec; use one of {}",
                    Self::names()
                )
            }),
            _ => Ok(Self::Libopus),
        }
    }
}

impl std::fmt::Display for OpusCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono,
    Stereo,
}

impl Channels {
    pub const fn count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Application {
    Voip,
    Audio,
}

/// What an encoder is built with. Both codecs get all of it; whatever is not here stays at
/// libopus's defaults (VBR, constrained VBR, signal auto, full band, 20 ms frames), which
/// opus-rs shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderParams {
    pub channels: Channels,
    pub application: Application,
    /// Bits per second.
    pub bitrate: i32,
    /// 0 to 10.
    pub complexity: i32,
    pub inband_fec: bool,
    /// The expected packet loss in percent, 0 to 100.
    pub loss_percent: u8,
}

/// Why a codec call failed. A panic is reported, not propagated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("{codec}: could not create the {what}: {detail}")]
    Init {
        codec: OpusCodec,
        what: &'static str,
        detail: String,
    },
    #[error("{codec}: {detail}")]
    Failed { codec: OpusCodec, detail: String },
    #[error("{codec} panicked: {detail}")]
    Panicked { codec: OpusCodec, detail: String },
}

impl CodecError {
    pub fn is_panic(&self) -> bool {
        matches!(self, Self::Panicked { .. })
    }
}

thread_local! {
    /// Set while this thread is inside a guarded opus-rs call, for the panic hook.
    static IN_GUARDED_CALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread is inside a guarded opus-rs call: a panic now unwinds to the codec's
/// `catch_unwind`. The desktop shell's panic hook lets such a panic through and ends the
/// process on any other (design D11).
pub fn in_guarded_call() -> bool {
    IN_GUARDED_CALL.with(std::cell::Cell::get)
}

/// Runs `call` on opus-rs under `catch_unwind`; libopus runs as it is.
fn guarded<T>(
    codec: OpusCodec,
    #[cfg_attr(not(test), allow(unused_variables))] inject_panic: bool,
    call: impl FnOnce() -> Result<T, String>,
) -> Result<T, CodecError> {
    let failed = |detail| CodecError::Failed { codec, detail };
    if !codec.is_pure_rust() {
        return call().map_err(failed);
    }
    let outer = IN_GUARDED_CALL.with(|flag| flag.replace(true));
    let result = catch_unwind(AssertUnwindSafe(|| {
        #[cfg(test)]
        if inject_panic {
            panic!("test-only panic injected into {codec}");
        }
        call()
    }));
    IN_GUARDED_CALL.with(|flag| flag.set(outer));
    match result {
        Ok(result) => result.map_err(failed),
        Err(payload) => Err(CodecError::Panicked {
            codec,
            detail: panic_message(payload.as_ref()),
        }),
    }
}

/// Which of the app's four Opus paths a codec ran on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecPath {
    /// The assistant's voice toward Discord (`DiscordEncoder`).
    DiscordOut,
    /// The people in the channel, decoded by the app (`SpeakerDecoders`).
    Receive,
    /// The speech toward the Codex call (the call's send loop).
    CallSend,
    /// The Codex call's answer (the call's jitter buffer).
    CallReceive,
}

impl CodecPath {
    pub const fn name(self) -> &'static str {
        match self {
            Self::DiscordOut => "discord-out",
            Self::Receive => "receive",
            Self::CallSend => "call-send",
            Self::CallReceive => "call-receive",
        }
    }

    /// What the codec was doing, for the user.
    pub const fn activity(self) -> &'static str {
        match self {
            Self::DiscordOut => "encoding the assistant's voice toward Discord",
            Self::Receive => "decoding the people in the channel",
            Self::CallSend => "encoding your speech toward Codex",
            Self::CallReceive => "decoding Codex's answer",
        }
    }
}

/// A panic of opus-rs on one of a guild's paths, for the runtime (design D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecFailure {
    pub guild: u64,
    pub path: CodecPath,
    pub codec: OpusCodec,
    pub message: String,
}

/// Where the app's codec panics go: one per runtime. Each guild reports once per join; a
/// new join resets it.
#[derive(Debug)]
pub struct CodecFailureHub {
    sender: tokio::sync::mpsc::UnboundedSender<CodecFailure>,
    reported: std::sync::Mutex<std::collections::HashSet<u64>>,
}

impl CodecFailureHub {
    pub fn new() -> (
        std::sync::Arc<Self>,
        tokio::sync::mpsc::UnboundedReceiver<CodecFailure>,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let hub = Self {
            sender,
            reported: std::sync::Mutex::new(std::collections::HashSet::new()),
        };
        (std::sync::Arc::new(hub), receiver)
    }

    /// A new join of `guild`: its next panic is reported again.
    pub fn reset(&self, guild: u64) {
        self.reported
            .lock()
            .expect("codec failures lock")
            .remove(&guild);
    }

    /// The handle a path of `guild` reports through.
    pub fn for_guild(self: &std::sync::Arc<Self>, guild: u64) -> CodecFailures {
        CodecFailures {
            guild,
            hub: Some(std::sync::Arc::clone(self)),
        }
    }
}

/// A path's handle for reporting a codec panic of its guild. `none()` reports nowhere (the
/// tests, the probes, a stub assistant).
#[derive(Debug, Clone, Default)]
pub struct CodecFailures {
    guild: u64,
    hub: Option<std::sync::Arc<CodecFailureHub>>,
}

impl CodecFailures {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn guild(&self) -> u64 {
        self.guild
    }

    /// Reports `error` when it is a panic, at most once per join of the guild; never blocks,
    /// since it runs on songbird's mixer thread, the receive task or a call's send thread.
    /// Returns whether a report was sent.
    pub fn report(&self, path: CodecPath, error: &CodecError) -> bool {
        let CodecError::Panicked { codec, detail } = error else {
            return false;
        };
        let Some(hub) = self.hub.as_ref() else {
            return false;
        };
        if !hub
            .reported
            .lock()
            .expect("codec failures lock")
            .insert(self.guild)
        {
            return false;
        }
        hub.sender
            .send(CodecFailure {
                guild: self.guild,
                path,
                codec: *codec,
                message: detail.clone(),
            })
            .is_ok()
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

enum EncoderImpl {
    Libopus(opus2::Encoder),
    OpusRs(Box<opus_rs::OpusEncoder>),
}

fn build_encoder(codec: OpusCodec, params: &EncoderParams) -> Result<EncoderImpl, String> {
    let loss = params.loss_percent.min(100);
    match codec {
        OpusCodec::Libopus => {
            let channels = match params.channels {
                Channels::Mono => opus2::Channels::Mono,
                Channels::Stereo => opus2::Channels::Stereo,
            };
            let application = match params.application {
                Application::Voip => opus2::Application::Voip,
                Application::Audio => opus2::Application::Audio,
            };
            let text = |error: opus2::Error| error.to_string();
            let mut encoder =
                opus2::Encoder::new(SAMPLE_RATE, channels, application).map_err(text)?;
            encoder
                .set_bitrate(opus2::Bitrate::Bits(params.bitrate))
                .map_err(text)?;
            encoder.set_complexity(params.complexity).map_err(text)?;
            encoder.set_inband_fec(params.inband_fec).map_err(text)?;
            encoder
                .set_packet_loss_perc(i32::from(loss))
                .map_err(text)?;
            Ok(EncoderImpl::Libopus(encoder))
        }
        OpusCodec::OpusRs => {
            let application = match params.application {
                Application::Voip => opus_rs::Application::Voip,
                Application::Audio => opus_rs::Application::Audio,
            };
            let mut encoder =
                opus_rs::OpusEncoder::new(SAMPLE_RATE as i32, params.channels.count(), application)
                    .map_err(str::to_owned)?;
            // Public fields; the defaults that differ from libopus (complexity 9, 64 kbit/s,
            // FEC off) are all set here. VBR (`use_cbr` false) and constrained VBR are the
            // defaults of both.
            encoder.bitrate_bps = params.bitrate;
            encoder.complexity = params.complexity;
            encoder.use_cbr = false;
            encoder.use_inband_fec = params.inband_fec;
            encoder.packet_loss_perc = i32::from(loss);
            Ok(EncoderImpl::OpusRs(Box::new(encoder)))
        }
    }
}

/// An Opus encoder of either codec. A failed call leaves it usable; a panic
/// drops the codec's state, which is rebuilt with the same parameters at the next call.
pub struct VoiceEncoder {
    codec: OpusCodec,
    params: EncoderParams,
    inner: Option<EncoderImpl>,
    #[cfg(test)]
    panic_next: bool,
}

impl VoiceEncoder {
    pub fn new(codec: OpusCodec, params: EncoderParams) -> Result<Self, CodecError> {
        let inner = build_encoder(codec, &params).map_err(|detail| CodecError::Init {
            codec,
            what: "encoder",
            detail,
        })?;
        Ok(Self {
            codec,
            params,
            inner: Some(inner),
            #[cfg(test)]
            panic_next: false,
        })
    }

    pub fn codec(&self) -> OpusCodec {
        self.codec
    }

    pub fn params(&self) -> EncoderParams {
        self.params
    }

    /// Makes the next call panic inside the codec (opus-rs only).
    #[cfg(test)]
    pub(crate) fn panic_next_for_test(&mut self) {
        self.panic_next = true;
    }

    fn run<T>(
        &mut self,
        call: impl FnOnce(&mut EncoderImpl) -> Result<T, String>,
    ) -> Result<T, CodecError> {
        let codec = self.codec;
        if self.inner.is_none() {
            let rebuilt =
                build_encoder(codec, &self.params).map_err(|detail| CodecError::Init {
                    codec,
                    what: "encoder",
                    detail,
                })?;
            self.inner = Some(rebuilt);
        }
        #[cfg(test)]
        let inject = std::mem::take(&mut self.panic_next);
        #[cfg(not(test))]
        let inject = false;
        let Some(inner) = self.inner.as_mut() else {
            return Err(CodecError::Failed {
                codec,
                detail: "no encoder".to_owned(),
            });
        };
        let result = guarded(codec, inject, || call(inner));
        if result.as_ref().is_err_and(CodecError::is_panic) {
            self.inner = None;
        }
        result
    }

    /// Encodes one frame of interleaved f32 (±1.0) into `out`; returns the packet's length.
    pub fn encode_f32(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, CodecError> {
        let channels = self.params.channels.count();
        self.run(|inner| match inner {
            EncoderImpl::Libopus(encoder) => {
                encoder.encode_float(pcm, out).map_err(|e| e.to_string())
            }
            EncoderImpl::OpusRs(encoder) => encoder
                .encode(pcm, pcm.len() / channels, out)
                .map_err(str::to_owned),
        })
    }

    /// Encodes one frame of interleaved i16 into `out`; returns the packet's length.
    pub fn encode_i16(&mut self, pcm: &[i16], out: &mut [u8]) -> Result<usize, CodecError> {
        let channels = self.params.channels.count();
        self.run(|inner| match inner {
            EncoderImpl::Libopus(encoder) => encoder.encode(pcm, out).map_err(|e| e.to_string()),
            EncoderImpl::OpusRs(encoder) => encoder
                .encode_i16(pcm, pcm.len() / channels, out)
                .map_err(str::to_owned),
        })
    }

    /// The expected packet loss the in-band FEC is sized for, in percent.
    pub fn set_loss_percent(&mut self, percent: u8) -> Result<(), CodecError> {
        let percent = percent.min(100);
        self.params.loss_percent = percent;
        self.run(|inner| match inner {
            EncoderImpl::Libopus(encoder) => encoder
                .set_packet_loss_perc(i32::from(percent))
                .map_err(|e| e.to_string()),
            EncoderImpl::OpusRs(encoder) => {
                encoder.packet_loss_perc = i32::from(percent);
                Ok(())
            }
        })
    }
}

enum DecoderImpl {
    Libopus(opus2::Decoder),
    OpusRs(Box<opus_rs::OpusDecoder>),
}

fn build_decoder(codec: OpusCodec, channels: Channels) -> Result<DecoderImpl, String> {
    match codec {
        OpusCodec::Libopus => {
            let channels = match channels {
                Channels::Mono => opus2::Channels::Mono,
                Channels::Stereo => opus2::Channels::Stereo,
            };
            opus2::Decoder::new(SAMPLE_RATE, channels)
                .map(DecoderImpl::Libopus)
                .map_err(|e| e.to_string())
        }
        OpusCodec::OpusRs => opus_rs::OpusDecoder::new(SAMPLE_RATE as i32, channels.count())
            .map(|decoder| DecoderImpl::OpusRs(Box::new(decoder)))
            .map_err(str::to_owned),
    }
}

/// An Opus decoder of either codec, with libopus's calling convention: an empty
/// packet conceals `out.len() / channels` samples, `fec` rebuilds that many samples from
/// the packet's in-band FEC (the packet itself is decoded by the next call), and otherwise
/// the packet is decoded into `out`, which must hold it. Each call returns samples per
/// channel. A panic drops the codec's state, which is rebuilt at the next call.
pub struct VoiceDecoder {
    codec: OpusCodec,
    channels: Channels,
    inner: Option<DecoderImpl>,
    /// opus-rs decodes f32 only: its i16 output goes through here (allocated once).
    scratch: Vec<f32>,
    #[cfg(test)]
    panic_next: bool,
}

impl VoiceDecoder {
    pub fn new(codec: OpusCodec, channels: Channels) -> Result<Self, CodecError> {
        let inner = build_decoder(codec, channels).map_err(|detail| CodecError::Init {
            codec,
            what: "decoder",
            detail,
        })?;
        let scratch = if codec == OpusCodec::OpusRs {
            vec![0.0; MAX_FRAME_SAMPLES * channels.count()]
        } else {
            Vec::new()
        };
        Ok(Self {
            codec,
            channels,
            inner: Some(inner),
            scratch,
            #[cfg(test)]
            panic_next: false,
        })
    }

    pub fn codec(&self) -> OpusCodec {
        self.codec
    }

    pub fn channels(&self) -> Channels {
        self.channels
    }

    /// Makes the next call panic inside the codec (opus-rs only).
    #[cfg(test)]
    pub(crate) fn panic_next_for_test(&mut self) {
        self.panic_next = true;
    }

    fn ensure(&mut self) -> Result<(), CodecError> {
        if self.inner.is_none() {
            let codec = self.codec;
            let rebuilt =
                build_decoder(codec, self.channels).map_err(|detail| CodecError::Init {
                    codec,
                    what: "decoder",
                    detail,
                })?;
            self.inner = Some(rebuilt);
        }
        Ok(())
    }

    fn take_inject(&mut self) -> bool {
        #[cfg(test)]
        {
            std::mem::take(&mut self.panic_next)
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    fn after<T>(&mut self, result: Result<T, CodecError>) -> Result<T, CodecError> {
        if result.as_ref().is_err_and(CodecError::is_panic) {
            self.inner = None;
        }
        result
    }

    /// Decodes into interleaved i16; see the type's documentation for the convention.
    pub fn decode_i16(
        &mut self,
        packet: &[u8],
        out: &mut [i16],
        fec: bool,
    ) -> Result<usize, CodecError> {
        self.ensure()?;
        let codec = self.codec;
        let channels = self.channels.count();
        let inject = self.take_inject();
        let Self { inner, scratch, .. } = self;
        let Some(inner) = inner.as_mut() else {
            return Err(CodecError::Failed {
                codec,
                detail: "no decoder".to_owned(),
            });
        };
        let result = guarded(codec, inject, || match inner {
            DecoderImpl::Libopus(decoder) => {
                decoder.decode(packet, out, fec).map_err(|e| e.to_string())
            }
            DecoderImpl::OpusRs(decoder) => {
                let wide = scratch
                    .get_mut(..out.len())
                    .ok_or_else(|| "output larger than 120 ms".to_owned())?;
                let count = opus_rs_decode(decoder, packet, wide, channels, fec)?;
                let used = (count * channels).min(out.len());
                for (sample, value) in out.iter_mut().zip(wide.iter()).take(used) {
                    *sample = to_i16(*value);
                }
                Ok(count)
            }
        });
        self.after(result)
    }

    /// Decodes into interleaved f32 (±1.0); see the type's documentation.
    pub fn decode_f32(
        &mut self,
        packet: &[u8],
        out: &mut [f32],
        fec: bool,
    ) -> Result<usize, CodecError> {
        self.ensure()?;
        let codec = self.codec;
        let channels = self.channels.count();
        let inject = self.take_inject();
        let Some(inner) = self.inner.as_mut() else {
            return Err(CodecError::Failed {
                codec,
                detail: "no decoder".to_owned(),
            });
        };
        let result = guarded(codec, inject, || match inner {
            DecoderImpl::Libopus(decoder) => decoder
                .decode_float(packet, out, fec)
                .map_err(|e| e.to_string()),
            DecoderImpl::OpusRs(decoder) => opus_rs_decode(decoder, packet, out, channels, fec),
        });
        self.after(result)
    }
}

fn opus_rs_decode(
    decoder: &mut opus_rs::OpusDecoder,
    packet: &[u8],
    out: &mut [f32],
    channels: usize,
    fec: bool,
) -> Result<usize, String> {
    let frame_size = out.len() / channels;
    if fec && !packet.is_empty() {
        decoder.decode_fec(packet, frame_size, out)
    } else {
        // A packet of 0 or 1 byte conceals `frame_size` samples.
        decoder.decode(packet, frame_size, out)
    }
    .map_err(str::to_owned)
}

/// libopus's float to 16-bit conversion: scale by 32768, round, clamp.
fn to_i16(value: f32) -> i16 {
    (value * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16
}

/// Counts nothing itself: tells a path when a spell of codec failures starts and how long
/// it was when it ends, so the log gets two lines per spell, not one per frame.
#[derive(Debug, Default)]
pub struct FailureSpell {
    run: u64,
}

impl FailureSpell {
    /// A failed call on `path`; the first of a spell is logged.
    pub fn failed(&mut self, path: &str, error: &CodecError) {
        if self.run == 0 {
            log::warn!("[{path}] {error}; the frame was replaced");
        }
        self.run += 1;
    }

    /// A call that worked; ends a spell with one line.
    pub fn succeeded(&mut self, path: &str) {
        if self.run > 0 {
            log::warn!(
                "[{path}] the codec works again after {} failed calls",
                self.run
            );
            self.run = 0;
        }
    }
}

// The encoders run on the call's send thread and songbird's mixer thread, the decoders on
// the mixer thread and the receive task: both codecs' state must move between threads.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<VoiceEncoder>();
    assert_send::<VoiceDecoder>();
};

#[cfg(test)]
pub(crate) mod test_signal {
    //! A deterministic speech-like test signal: a voiced harmonic sound whose pitch and
    //! loudness change every 20 ms, with breath noise, so concealment cannot simply repeat
    //! the previous frame.

    pub struct Speechlike {
        seed: u64,
        phase: f64,
    }

    impl Speechlike {
        pub fn new(seed: u64) -> Self {
            Self { seed, phase: 0.0 }
        }

        fn next_random(&mut self) -> f64 {
            self.seed = self
                .seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.seed >> 11) as f64) / ((1u64 << 53) as f64)
        }

        /// One 20 ms mono frame of 960 samples in ±1.0.
        pub fn frame(&mut self) -> Vec<f32> {
            let pitch = 90.0 + 160.0 * self.next_random();
            let loudness = 0.15 + 0.35 * self.next_random();
            let mut out = Vec::with_capacity(960);
            for _ in 0..960 {
                self.phase += pitch / 48_000.0;
                let mut value = 0.0;
                for harmonic in 1..=12 {
                    let h = f64::from(harmonic);
                    value += (std::f64::consts::TAU * self.phase * h).sin() / h;
                }
                let noise = self.next_random() - 0.5;
                out.push((loudness * (0.5 * value + 0.05 * noise)) as f32);
            }
            out
        }

        /// One 20 ms stereo frame (interleaved, the right channel a little quieter).
        pub fn stereo_frame(&mut self) -> Vec<f32> {
            self.frame()
                .into_iter()
                .flat_map(|value| [value, value * 0.8])
                .collect()
        }
    }

    /// Converts to i16 as libopus does.
    pub fn to_i16(samples: &[f32]) -> Vec<i16> {
        samples.iter().map(|value| super::to_i16(*value)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::test_signal::Speechlike;
    use super::*;

    const FRAME: usize = 960;

    fn call_params() -> EncoderParams {
        EncoderParams {
            channels: Channels::Mono,
            application: Application::Voip,
            bitrate: 32_000,
            complexity: 10,
            inband_fec: true,
            loss_percent: 5,
        }
    }

    fn discord_params() -> EncoderParams {
        EncoderParams {
            channels: Channels::Stereo,
            application: Application::Voip,
            bitrate: 128_000,
            complexity: 10,
            inband_fec: true,
            loss_percent: 10,
        }
    }

    /// The Opus mode of a packet from its TOC byte.
    fn mode(packet: &[u8]) -> &'static str {
        match packet.first().map(|toc| toc >> 3) {
            Some(0..=11) => "silk",
            Some(12..=15) => "hybrid",
            Some(_) => "celt",
            None => "empty",
        }
    }

    fn encode_stream(codec: OpusCodec, params: EncoderParams, frames: usize) -> Vec<Vec<u8>> {
        let mut encoder = VoiceEncoder::new(codec, params).expect("encoder");
        let mut signal = Speechlike::new(7);
        let mut out = vec![0u8; 4_000];
        (0..frames)
            .map(|_| {
                let pcm = match params.channels {
                    Channels::Mono => signal.frame(),
                    Channels::Stereo => signal.stereo_frame(),
                };
                let written = encoder.encode_f32(&pcm, &mut out).expect("encode");
                out[..written].to_vec()
            })
            .collect()
    }

    fn energy(samples: &[i16]) -> f64 {
        samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64
    }

    /// Normalised correlation of two frames.
    fn correlation(a: &[i16], b: &[i16]) -> f64 {
        let dot: f64 = a
            .iter()
            .zip(b)
            .map(|(x, y)| f64::from(*x) * f64::from(*y))
            .sum();
        let norm = (energy(a) * energy(b)).sqrt() * a.len() as f64;
        if norm == 0.0 {
            0.0
        } else {
            dot / norm
        }
    }

    #[test]
    fn names_parse_and_label() {
        for codec in OpusCodec::ALL {
            assert_eq!(OpusCodec::parse(codec.name()), Some(codec));
            assert!(codec.label().starts_with(codec.name()), "{}", codec.label());
        }
        assert_eq!(OpusCodec::parse("opus2"), None);
        assert_eq!(OpusCodec::names(), "libopus, opus-rs");
        for removed in ["ropus", "mousiki"] {
            assert_eq!(OpusCodec::parse(removed), None, "{removed} is not built in");
        }
        assert_eq!(OpusCodec::default(), OpusCodec::Libopus);
    }

    #[test]
    fn every_codec_round_trips_full_frames_at_both_paths_parameters() {
        for codec in OpusCodec::ALL {
            for params in [call_params(), discord_params()] {
                let packets = encode_stream(codec, params, 50);
                let mut decoder = VoiceDecoder::new(codec, params.channels).expect("decoder");
                let channels = params.channels.count();
                let mut out = vec![0i16; MAX_FRAME_SAMPLES * channels];
                let mut heard = 0.0;
                for packet in &packets {
                    let count = decoder.decode_i16(packet, &mut out, false).expect("decode");
                    assert_eq!(count, FRAME, "{codec} {params:?}");
                    heard += energy(&out[..count * channels]);
                }
                assert!(heard > 0.0, "{codec} decoded only silence");
            }
        }
    }

    #[test]
    fn libopus_decodes_every_encoders_packets() {
        let celt_params = EncoderParams {
            application: Application::Audio,
            inband_fec: false,
            loss_percent: 0,
            ..discord_params()
        };
        for codec in OpusCodec::ALL {
            let mut modes = std::collections::BTreeSet::new();
            for params in [silk_params(), call_params(), discord_params(), celt_params] {
                let packets = encode_stream(codec, params, 100);
                let mut decoder =
                    VoiceDecoder::new(OpusCodec::Libopus, params.channels).expect("decoder");
                let mut out = vec![0i16; MAX_FRAME_SAMPLES * params.channels.count()];
                for (index, packet) in packets.iter().enumerate() {
                    modes.insert(mode(packet));
                    let count = decoder
                        .decode_i16(packet, &mut out, false)
                        .unwrap_or_else(|e| panic!("{codec} packet {index}: {e}"));
                    assert_eq!(count, FRAME, "{codec} packet {index}");
                }
            }
            eprintln!("{codec}: libopus decoded every packet; modes seen: {modes:?}");
            if codec == OpusCodec::Libopus {
                assert_eq!(modes.len(), 3, "the streams cover SILK, hybrid and CELT");
            }
        }
    }

    /// Low enough for libopus to choose SILK.
    fn silk_params() -> EncoderParams {
        EncoderParams {
            bitrate: 12_000,
            ..call_params()
        }
    }

    #[test]
    fn every_decoder_decodes_libopus_packets() {
        let client = EncoderParams {
            bitrate: 64_000,
            ..discord_params()
        };
        let music = EncoderParams {
            application: Application::Audio,
            inband_fec: false,
            loss_percent: 0,
            ..discord_params()
        };
        let streams: Vec<(EncoderParams, Vec<Vec<u8>>)> =
            [silk_params(), call_params(), client, music]
                .into_iter()
                .map(|params| (params, encode_stream(OpusCodec::Libopus, params, 100)))
                .collect();
        for codec in OpusCodec::ALL {
            for (params, packets) in &streams {
                let mut decoder = VoiceDecoder::new(codec, params.channels).expect("decoder");
                let mut out = vec![0i16; MAX_FRAME_SAMPLES * params.channels.count()];
                for (index, packet) in packets.iter().enumerate() {
                    let count = decoder
                        .decode_i16(packet, &mut out, false)
                        .unwrap_or_else(|e| {
                            panic!("{codec} packet {index} ({}): {e}", mode(packet))
                        });
                    assert_eq!(count, FRAME, "{codec} packet {index}");
                }
            }
        }
    }

    /// Decodes `packets` with packets `lost` missing: a lost packet whose successor arrived
    /// is rebuilt from that packet's FEC, one without is concealed. Returns the frames.
    fn decode_with_loss(codec: OpusCodec, packets: &[Vec<u8>], lost: &[usize]) -> Vec<Vec<i16>> {
        let mut decoder = VoiceDecoder::new(codec, Channels::Mono).expect("decoder");
        let mut frames = Vec::new();
        for (index, packet) in packets.iter().enumerate() {
            let mut out = vec![0i16; FRAME];
            if lost.contains(&index) {
                let count = match packets.get(index + 1) {
                    Some(next) if !lost.contains(&(index + 1)) => {
                        decoder.decode_i16(next, &mut out, true)
                    }
                    _ => decoder.decode_i16(&[], &mut out, false),
                }
                .unwrap_or_else(|e| panic!("{codec} lost {index}: {e}"));
                assert_eq!(count, FRAME, "{codec} lost {index} gives a full frame");
            } else {
                let mut wide = vec![0i16; MAX_FRAME_SAMPLES];
                let count = decoder
                    .decode_i16(packet, &mut wide, false)
                    .unwrap_or_else(|e| panic!("{codec} packet {index}: {e}"));
                assert_eq!(count, FRAME);
                out.copy_from_slice(&wide[..FRAME]);
            }
            frames.push(out);
        }
        frames
    }

    #[test]
    fn every_codec_rebuilds_a_lost_packet_from_fec_and_conceals_without_it() {
        let params = EncoderParams {
            loss_percent: 20,
            ..call_params()
        };
        for codec in OpusCodec::ALL {
            let packets = encode_stream(codec, params, 60);
            let clean = decode_with_loss(codec, &packets, &[]);
            // One lost packet: rebuilt from the next packet's FEC.
            let one = decode_with_loss(codec, &packets, &[30]);
            // Two in a row: the first concealed, the second rebuilt from FEC.
            let two = decode_with_loss(codec, &packets, &[40, 41]);
            let fec = correlation(&one[30], &clean[30]);
            let concealed = correlation(&two[40], &clean[40]);
            let fec_second = correlation(&two[41], &clean[41]);
            eprintln!(
                "{codec}: FEC frame correlation {fec:.2}, concealed {concealed:.2}, FEC after a concealed frame {fec_second:.2}"
            );
            assert!(
                fec > 0.5 && fec > concealed + 0.1,
                "{codec}: the frame rebuilt from FEC ({fec:.2}) is not closer to the original than concealment ({concealed:.2})"
            );
        }
    }

    #[test]
    fn the_expected_loss_changes_on_a_running_encoder() {
        for codec in OpusCodec::ALL {
            let mut encoder = VoiceEncoder::new(codec, discord_params()).expect("encoder");
            let mut signal = Speechlike::new(3);
            let mut out = vec![0u8; 4_000];
            for percent in [10u8, 0, 25, 100] {
                encoder.set_loss_percent(percent).expect("set loss");
                assert_eq!(encoder.params().loss_percent, percent);
                let written = encoder
                    .encode_f32(&signal.stereo_frame(), &mut out)
                    .expect("encode");
                assert!(written > 0, "{codec} at {percent} %");
            }
        }
    }

    #[test]
    fn i16_and_f32_paths_agree_in_length() {
        for codec in OpusCodec::ALL {
            let mut encoder = VoiceEncoder::new(codec, call_params()).expect("encoder");
            let mut signal = Speechlike::new(11);
            let mut out = vec![0u8; 4_000];
            let pcm = test_signal::to_i16(&signal.frame());
            let written = encoder.encode_i16(&pcm, &mut out).expect("encode i16");
            let mut decoder = VoiceDecoder::new(codec, Channels::Mono).expect("decoder");
            let mut wide = vec![0f32; MAX_FRAME_SAMPLES];
            assert_eq!(
                decoder
                    .decode_f32(&out[..written], &mut wide, false)
                    .expect("decode f32"),
                FRAME
            );
            let mut concealed = vec![0f32; FRAME];
            assert_eq!(
                decoder
                    .decode_f32(&[], &mut concealed, false)
                    .expect("conceal f32"),
                FRAME
            );
        }
    }

    #[test]
    fn a_panic_inside_opus_rs_is_caught_and_the_codec_rebuilt() {
        {
            let codec = OpusCodec::OpusRs;
            let mut encoder = VoiceEncoder::new(codec, discord_params()).expect("encoder");
            let mut signal = Speechlike::new(5);
            let mut out = vec![0u8; 4_000];
            encoder.panic_next_for_test();
            let error = encoder
                .encode_f32(&signal.stereo_frame(), &mut out)
                .expect_err("the injected panic");
            assert!(error.is_panic(), "{error}");
            let written = encoder
                .encode_f32(&signal.stereo_frame(), &mut out)
                .expect("a fresh encoder after the panic");
            assert!(written > 0);

            let mut decoder = VoiceDecoder::new(codec, Channels::Stereo).expect("decoder");
            let mut pcm = vec![0i16; MAX_FRAME_SAMPLES * 2];
            decoder.panic_next_for_test();
            assert!(decoder
                .decode_i16(&out[..written], &mut pcm, false)
                .expect_err("the injected panic")
                .is_panic());
            assert_eq!(
                decoder
                    .decode_i16(&out[..written], &mut pcm, false)
                    .expect("a fresh decoder after the panic"),
                FRAME
            );
        }
    }

    #[test]
    fn the_guard_flag_is_set_only_inside_a_guarded_opus_rs_call() {
        assert!(!in_guarded_call());
        let inside = guarded(OpusCodec::OpusRs, false, || Ok(in_guarded_call())).unwrap();
        assert!(inside, "inside the guarded call");
        assert!(!in_guarded_call(), "reset after it");
        let _ = guarded::<()>(OpusCodec::OpusRs, true, || Ok(()));
        assert!(!in_guarded_call(), "reset after a panic too");
        let libopus = guarded(OpusCodec::Libopus, false, || Ok(in_guarded_call())).unwrap();
        assert!(!libopus, "libopus calls are not guarded");
    }

    #[test]
    fn a_panic_is_reported_once_per_join_and_an_error_never() {
        let (hub, mut reports) = CodecFailureHub::new();
        let failures = hub.for_guild(42);
        let panic = CodecError::Panicked {
            codec: OpusCodec::OpusRs,
            detail: "index out of bounds".to_owned(),
        };
        let error = CodecError::Failed {
            codec: OpusCodec::OpusRs,
            detail: "invalid packet".to_owned(),
        };
        assert!(!failures.report(CodecPath::Receive, &error), "not a panic");
        assert!(failures.report(CodecPath::DiscordOut, &panic));
        assert!(
            !hub.for_guild(42).report(CodecPath::CallSend, &panic),
            "once per join, whichever path"
        );
        assert!(
            hub.for_guild(7).report(CodecPath::CallSend, &panic),
            "another guild reports on its own"
        );
        assert_eq!(
            reports.try_recv().unwrap(),
            CodecFailure {
                guild: 42,
                path: CodecPath::DiscordOut,
                codec: OpusCodec::OpusRs,
                message: "index out of bounds".to_owned(),
            }
        );
        assert_eq!(reports.try_recv().unwrap().guild, 7);
        assert!(reports.try_recv().is_err());
        hub.reset(42);
        assert!(
            failures.report(CodecPath::Receive, &panic),
            "a new join reports again"
        );
        assert!(!CodecFailures::none().report(CodecPath::Receive, &panic));
    }

    #[test]
    fn a_failure_spell_logs_twice() {
        let mut spell = FailureSpell::default();
        let error = CodecError::Failed {
            codec: OpusCodec::OpusRs,
            detail: "bad".to_owned(),
        };
        spell.failed("test", &error);
        spell.failed("test", &error);
        assert_eq!(spell.run, 2);
        spell.succeeded("test");
        assert_eq!(spell.run, 0);
    }

    #[test]
    fn an_unknown_codec_name_lists_the_two() {
        let error = OpusCodec::parse("opus2")
            .ok_or_else(OpusCodec::names)
            .expect_err("unknown");
        assert!(error.contains("libopus") && error.contains("opus-rs"));
    }
}
