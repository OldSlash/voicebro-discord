//! `opus-compare <corpus dir> [--out <dir>] [--no-wav]` (change `opus-codec-experiment`,
//! design D6.1): every WAV under the corpus through the app's Opus codecs (libopus and
//! opus-rs), on the app's four Opus paths, with the same input for each codec. The
//! comparison of 2026-11-12 also ran ropus and mousiki; its numbers are the reference an
//! opus-rs update is checked against (`docs/OPUS-CODEC-COMPARISON.md`).
//!
//! - `call`: our speech toward Codex. Each codec encodes (`rtc::call_params`), libopus
//!   decodes, as the realtime server does.
//! - `discord`: the assistant's voice toward Discord. Each codec encodes
//!   (`opus_out::discord_params`, 10 % expected loss), libopus decodes, as a listener's
//!   client does.
//! - `receive`: the people in the channel. libopus encodes as a Discord client (stereo VOIP,
//!   64 kbit/s, FEC, 10 % expected loss); each codec decodes.
//! - `answer`: the Codex answer. libopus encodes with the call's parameters (a stand-in for
//!   the server); each codec decodes, with the jitter buffer's FEC and concealment when
//!   packets are lost.
//!
//! The loss conditions (random 5, 10 and 20 %, and a burst of 2 or 3 every 25 packets, all
//! seeded) apply to `call` (libopus at the receiving end rebuilds a lost packet from the
//! next one's FEC, else conceals) and `answer` (the codec does). The numbers are the
//! waveform SNR and the segmental SNR against the input after aligning the delay, the
//! correlation of the 20 ms loudness, the level difference, and, for decoders, the SNR
//! against libopus's own decode of the same packets. SNR is an indicator for a perceptual
//! codec, not a quality score; recognition, listening and the live calls decide.
//!
//! Writes `summary.md`, `results.csv` and, unless `--no-wav`, the decoded speech as 48 kHz
//! mono WAVs under `<out>/<path>/<codec>/<the file's path in the corpus>`, so the wake
//! corpus can run on `<out>/receive/<codec>` (`scripts\run-wake-corpus.ps1 -CorpusRoot`).
//!
//! Pass criterion: libopus decodes every packet of every encoder, and every decoder decodes
//! every packet libopus made, without an error.

use crate::Outcome;
use anyhow::{anyhow, Context};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;
use voicebro_core::assistant::codex::rtc::call_params;
use voicebro_core::audio::codec::{
    Application, Channels, CodecError, EncoderParams, OpusCodec, VoiceDecoder, VoiceEncoder,
    MAX_FRAME_SAMPLES,
};
use voicebro_core::audio::dsp::MonoToStereoUpsampler;
use voicebro_core::audio::opus_out::discord_params;

const FRAME: usize = 960;
const MAX_LAG: usize = 2_400;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Loss {
    None,
    Random(u8),
    Burst(u8),
}

impl Loss {
    const ALL: [Loss; 6] = [
        Loss::None,
        Loss::Random(5),
        Loss::Random(10),
        Loss::Random(20),
        Loss::Burst(2),
        Loss::Burst(3),
    ];

    fn name(self) -> String {
        match self {
            Loss::None => "no loss".to_owned(),
            Loss::Random(percent) => format!("{percent} % random"),
            Loss::Burst(length) => format!("burst of {length}"),
        }
    }

    /// Which of `count` packets are lost; the same pattern for every codec and file length.
    fn pattern(self, count: usize) -> Vec<bool> {
        let mut seed: u64 = 0x005e_ed0f_0b05;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        (0..count)
            .map(|index| match self {
                Loss::None => false,
                Loss::Random(percent) => random() < f64::from(percent) / 100.0,
                Loss::Burst(length) => index >= 10 && (index - 10) % 25 < usize::from(length),
            })
            .collect()
    }
}

/// How close a decoded signal is to a reference.
#[derive(Clone, Copy, Default)]
struct Metrics {
    snr: f64,
    seg_snr: f64,
    loudness_r: f64,
    level_db: f64,
}

#[derive(Default)]
struct Sum {
    count: usize,
    snr: f64,
    seg_snr: f64,
    loudness_r: f64,
    level_db: f64,
    /// SNR against libopus's decode of the same packets (decoders only).
    vs_libopus: f64,
    vs_count: usize,
}

impl Sum {
    fn add(&mut self, metrics: Metrics) {
        self.count += 1;
        self.snr += metrics.snr;
        self.seg_snr += metrics.seg_snr;
        self.loudness_r += metrics.loudness_r;
        self.level_db += metrics.level_db;
    }

    fn mean(&self, value: f64) -> f64 {
        value / self.count.max(1) as f64
    }
}

#[derive(Default)]
struct Timing {
    micros: Vec<f64>,
}

impl Timing {
    fn summary(&mut self) -> (f64, f64, f64) {
        if self.micros.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        self.micros.sort_by(f64::total_cmp);
        let len = self.micros.len();
        (
            self.micros[len / 2],
            self.micros[(len - 1) * 99 / 100],
            self.micros[len - 1],
        )
    }
}

/// Everything measured, by (path, codec, loss).
#[derive(Default)]
struct Results {
    quality: BTreeMap<(&'static str, usize, Loss), Sum>,
    encode: BTreeMap<(&'static str, usize), Timing>,
    decode: BTreeMap<(&'static str, usize), Timing>,
    errors: Vec<String>,
    csv: String,
}

pub fn compare(args: &[String]) -> anyhow::Result<Outcome> {
    let usage = || anyhow!("usage: probe opus-compare <corpus dir> [--out <dir>] [--no-wav]");
    let corpus = PathBuf::from(args.first().ok_or_else(usage)?);
    let out = args
        .iter()
        .position(|arg| arg == "--out")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("opus-compare"));
    let write_wavs = !args.iter().any(|arg| arg == "--no-wav");
    let mut files = Vec::new();
    collect_wavs(&corpus, &out, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(anyhow!("no WAV files under {}", corpus.display()));
    }
    std::fs::create_dir_all(&out)?;
    let mut results = Results {
        csv: "file,path,codec,loss,snr_db,seg_snr_db,loudness_r,level_db\n".to_owned(),
        ..Results::default()
    };
    let mut seconds = 0.0;
    for file in &files {
        let relative = file.strip_prefix(&corpus).unwrap_or(file).to_path_buf();
        let input = match read_48k_mono(file) {
            Ok(input) => input,
            Err(error) => {
                results
                    .errors
                    .push(format!("{}: {error:#}", relative.display()));
                continue;
            }
        };
        seconds += input.len() as f64 / 48_000.0;
        run_file(&relative, &input, &out, write_wavs, &mut results)?;
    }
    let summary = summary(&files, seconds, &mut results);
    std::fs::write(out.join("summary.md"), &summary)?;
    std::fs::write(out.join("results.csv"), &results.csv)?;
    let mut details: Vec<String> = summary.lines().map(str::to_owned).collect();
    details.push(format!("written to {}", out.display()));
    let pass = results.errors.is_empty();
    details.push(format!(
        "criterion: no decode error on any path ({} errors)",
        results.errors.len()
    ));
    Ok(Outcome { pass, details })
}

fn collect_wavs(dir: &Path, skip: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path != skip && !path.ends_with("assets") && !path.ends_with("archives") {
                collect_wavs(&path, skip, files)?;
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// A 16-bit WAV as 48 kHz mono: channels averaged, 16 and 24 kHz upsampled with the app's
/// FIR upsampler. Padded with silence to whole 20 ms frames.
fn read_48k_mono(path: &Path) -> anyhow::Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err(anyhow!("not 16-bit PCM"));
    }
    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let channels = usize::from(spec.channels.max(1));
    let mono: Vec<i16> = samples
        .chunks(channels)
        .map(|frame| (frame.iter().map(|s| i32::from(*s)).sum::<i32>() / frame.len() as i32) as i16)
        .collect();
    let mut out = match spec.sample_rate {
        48_000 => mono,
        rate if rate > 0 && 48_000 % rate == 0 => {
            let mut upsampler = MonoToStereoUpsampler::new((48_000 / rate) as usize);
            upsampler.process(&mono).into_iter().step_by(2).collect()
        }
        rate => return Err(anyhow!("{rate} Hz is not a divisor of 48 kHz")),
    };
    let padded = out.len().div_ceil(FRAME) * FRAME;
    out.resize(padded, 0);
    Ok(out)
}

fn stereo(mono: &[i16]) -> Vec<i16> {
    mono.iter().flat_map(|sample| [*sample, *sample]).collect()
}

fn left(stereo: &[i16]) -> Vec<i16> {
    stereo.iter().step_by(2).copied().collect()
}

/// A Discord client's encoder: stereo VOIP, 64 kbit/s, FEC, 10 % expected loss.
fn client_params() -> EncoderParams {
    EncoderParams {
        channels: Channels::Stereo,
        application: Application::Voip,
        bitrate: 64_000,
        complexity: 10,
        inband_fec: true,
        loss_percent: 10,
    }
}

/// Encodes `pcm` (interleaved for `params.channels`) frame by frame; with `timing`, the time
/// of each encode in µs is recorded.
fn encode(
    codec: OpusCodec,
    params: EncoderParams,
    pcm: &[i16],
    mut timing: Option<&mut Timing>,
) -> Result<Vec<Vec<u8>>, CodecError> {
    let mut encoder = VoiceEncoder::new(codec, params)?;
    let mut out = vec![0u8; 4_000];
    let channels = params.channels.count();
    let mut packets = Vec::with_capacity(pcm.len() / (FRAME * channels));
    for frame in pcm.chunks_exact(FRAME * channels) {
        let began = Instant::now();
        let written = encoder.encode_i16(frame, &mut out)?;
        if let Some(timing) = timing.as_deref_mut() {
            timing.micros.push(began.elapsed().as_secs_f64() * 1e6);
        }
        packets.push(out[..written].to_vec());
    }
    Ok(packets)
}

/// Decodes `packets` with `codec`; lost packets (by `loss`) are rebuilt from the next
/// packet's FEC when it arrived, else concealed, as the jitter buffer does. Returns the
/// interleaved output.
fn decode(
    codec: OpusCodec,
    channels: Channels,
    packets: &[Vec<u8>],
    loss: Loss,
    mut timing: Option<&mut Timing>,
) -> Result<Vec<i16>, CodecError> {
    let lost = loss.pattern(packets.len());
    let mut decoder = VoiceDecoder::new(codec, channels)?;
    let width = channels.count();
    let mut wide = vec![0i16; MAX_FRAME_SAMPLES * width];
    let mut output = Vec::with_capacity(packets.len() * FRAME * width);
    for (index, packet) in packets.iter().enumerate() {
        if lost[index] {
            let frame = &mut wide[..FRAME * width];
            let count = match packets.get(index + 1) {
                Some(next) if !lost[index + 1] => decoder.decode_i16(next, frame, true)?,
                _ => decoder.decode_i16(&[], frame, false)?,
            };
            output.extend_from_slice(&frame[..count * width]);
        } else {
            let began = Instant::now();
            let count = decoder.decode_i16(packet, &mut wide, false)?;
            if let Some(timing) = timing.as_deref_mut() {
                timing.micros.push(began.elapsed().as_secs_f64() * 1e6);
            }
            output.extend_from_slice(&wide[..count * width]);
        }
    }
    Ok(output)
}

fn run_file(
    relative: &Path,
    input: &[i16],
    out: &Path,
    write_wavs: bool,
    results: &mut Results,
) -> anyhow::Result<()> {
    let name = relative.display().to_string();
    let codecs = OpusCodec::ALL;

    // Encoders: `call` (mono) and `discord` (stereo), decoded by libopus.
    for (path, params, pcm) in [
        ("call", call_params(), input.to_vec()),
        ("discord", discord_params(10), stereo(input)),
    ] {
        for (index, codec) in codecs.iter().enumerate() {
            let timing = results.encode.entry((path, index)).or_default();
            let packets = match encode(*codec, params, &pcm, Some(timing)) {
                Ok(packets) => packets,
                Err(error) => {
                    results
                        .errors
                        .push(format!("{name} {path} {codec} encode: {error}"));
                    continue;
                }
            };
            let conditions: &[Loss] = if path == "call" {
                &Loss::ALL
            } else {
                &[Loss::None]
            };
            for loss in conditions {
                let decoded =
                    match decode(OpusCodec::Libopus, params.channels, &packets, *loss, None) {
                        Ok(decoded) => decoded,
                        Err(error) => {
                            results.errors.push(format!(
                                "{name} {path} {codec} ({}) decoded by libopus: {error}",
                                loss.name()
                            ));
                            continue;
                        }
                    };
                let mono = if params.channels == Channels::Stereo {
                    left(&decoded)
                } else {
                    decoded
                };
                let metrics = measure(input, &mono);
                record(results, &name, path, index, *loss, metrics, None);
                if write_wavs && (*loss == Loss::None || *loss == Loss::Random(10)) {
                    let folder = if *loss == Loss::None {
                        path.to_owned()
                    } else {
                        format!("{path}-loss10")
                    };
                    write_wav(&out.join(folder).join(codec.name()).join(relative), &mono)?;
                }
            }
        }
    }

    // Decoders: `receive` (a Discord client's stereo stream) and `answer` (the call's mono
    // stream with loss), encoded once by libopus.
    for (path, params, pcm) in [
        ("receive", client_params(), stereo(input)),
        ("answer", call_params(), input.to_vec()),
    ] {
        let packets = encode(OpusCodec::Libopus, params, &pcm, None)
            .map_err(|error| anyhow!("{name} {path}: libopus encode: {error}"))?;
        let conditions: &[Loss] = if path == "answer" {
            &Loss::ALL
        } else {
            &[Loss::None]
        };
        for loss in conditions {
            let reference = decode(OpusCodec::Libopus, params.channels, &packets, *loss, None)
                .map_err(|error| anyhow!("{name} {path}: libopus decode: {error}"))?;
            let reference = if params.channels == Channels::Stereo {
                left(&reference)
            } else {
                reference
            };
            for (index, codec) in codecs.iter().enumerate() {
                let timing =
                    (*loss == Loss::None).then(|| results.decode.entry((path, index)).or_default());
                let decoded = match decode(*codec, params.channels, &packets, *loss, timing) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        results.errors.push(format!(
                            "{name} {path} {codec} ({}) decode: {error}",
                            loss.name()
                        ));
                        continue;
                    }
                };
                let mono = if params.channels == Channels::Stereo {
                    left(&decoded)
                } else {
                    decoded
                };
                let metrics = measure(input, &mono);
                let vs = snr_aligned(&reference, &mono, 0);
                record(results, &name, path, index, *loss, metrics, Some(vs));
                if write_wavs && (*loss == Loss::None || *loss == Loss::Random(10)) {
                    let folder = if *loss == Loss::None {
                        path.to_owned()
                    } else {
                        format!("{path}-loss10")
                    };
                    write_wav(&out.join(folder).join(codec.name()).join(relative), &mono)?;
                }
            }
        }
    }
    Ok(())
}

fn record(
    results: &mut Results,
    name: &str,
    path: &'static str,
    codec: usize,
    loss: Loss,
    metrics: Metrics,
    vs_libopus: Option<f64>,
) {
    let sum = results.quality.entry((path, codec, loss)).or_default();
    sum.add(metrics);
    if let Some(vs) = vs_libopus {
        sum.vs_libopus += vs;
        sum.vs_count += 1;
    }
    let _ = writeln!(
        results.csv,
        "{name},{path},{},{},{:.2},{:.2},{:.4},{:.2}",
        OpusCodec::ALL[codec].name(),
        loss.name(),
        metrics.snr,
        metrics.seg_snr,
        metrics.loudness_r,
        metrics.level_db
    );
}

fn write_wav(path: &Path, mono: &[i16]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in mono {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}

/// The codec delay: the lag in 0..MAX_LAG where `decoded` best matches `reference`, found
/// on 8× decimated signals and refined around it.
fn delay(reference: &[i16], decoded: &[i16]) -> usize {
    let span = reference.len().min(decoded.len()).saturating_sub(MAX_LAG);
    if span < FRAME {
        return 0;
    }
    let correlation = |lag: usize, step: usize| -> f64 {
        let (mut dot, mut a2, mut b2) = (0f64, 0f64, 0f64);
        for index in (0..span).step_by(step) {
            let a = f64::from(reference[index]);
            let b = f64::from(decoded[index + lag]);
            dot += a * b;
            a2 += a * a;
            b2 += b * b;
        }
        if a2 > 0.0 && b2 > 0.0 {
            dot / (a2 * b2).sqrt()
        } else {
            0.0
        }
    };
    let coarse = (0..MAX_LAG)
        .step_by(8)
        .max_by(|a, b| correlation(*a, 8).total_cmp(&correlation(*b, 8)))
        .unwrap_or(0);
    (coarse.saturating_sub(8)..(coarse + 8).min(MAX_LAG))
        .max_by(|a, b| correlation(*a, 2).total_cmp(&correlation(*b, 2)))
        .unwrap_or(coarse)
}

fn snr_aligned(reference: &[i16], decoded: &[i16], lag: usize) -> f64 {
    let len = reference.len().min(decoded.len().saturating_sub(lag));
    let (mut signal, mut noise) = (0f64, 0f64);
    for index in 0..len {
        let a = f64::from(reference[index]);
        let b = f64::from(decoded[index + lag]);
        signal += a * a;
        noise += (a - b) * (a - b);
    }
    if noise == 0.0 {
        99.0
    } else {
        10.0 * (signal / noise).log10()
    }
}

fn measure(input: &[i16], decoded: &[i16]) -> Metrics {
    let lag = delay(input, decoded);
    let len = input.len().min(decoded.len().saturating_sub(lag));
    let a = &input[..len];
    let b = &decoded[lag..lag + len];
    let energy = |s: &[i16]| s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
    let mut segments = Vec::new();
    let mut rms_a = Vec::new();
    let mut rms_b = Vec::new();
    for (x, y) in a
        .as_chunks::<FRAME>()
        .0
        .iter()
        .zip(b.as_chunks::<FRAME>().0)
    {
        let ex = energy(x);
        let ey = energy(y);
        rms_a.push((ex / FRAME as f64).sqrt());
        rms_b.push((ey / FRAME as f64).sqrt());
        // Frames above -50 dBFS count for the segmental SNR.
        if ex / FRAME as f64 > (32_768.0f64 * 0.003_16).powi(2) {
            let noise: f64 = x
                .iter()
                .zip(y)
                .map(|(p, q)| (f64::from(*p) - f64::from(*q)).powi(2))
                .sum();
            segments.push((10.0 * (ex / noise.max(1.0)).log10()).clamp(-10.0, 35.0));
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let (ma, mb) = (mean(&rms_a), mean(&rms_b));
    let covariance: f64 = rms_a
        .iter()
        .zip(&rms_b)
        .map(|(x, y)| (x - ma) * (y - mb))
        .sum();
    let spread = |v: &[f64], m: f64| v.iter().map(|x| (x - m).powi(2)).sum::<f64>().sqrt();
    let loudness_r = covariance / (spread(&rms_a, ma) * spread(&rms_b, mb)).max(f64::MIN_POSITIVE);
    Metrics {
        snr: snr_aligned(input, decoded, lag),
        seg_snr: mean(&segments),
        loudness_r,
        level_db: 10.0 * (energy(b) / energy(a).max(1.0)).log10(),
    }
}

fn summary(files: &[PathBuf], seconds: f64, results: &mut Results) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# Opus codecs compared\n\n{} files, {:.0} s of speech at 48 kHz. SNR and segmental SNR in dB against the input after aligning the codec delay; loudness r is the correlation of the 20 ms loudness; level in dB.\n",
        files.len(),
        seconds
    );
    let labels: Vec<String> = OpusCodec::ALL.iter().map(|codec| codec.label()).collect();
    let _ = writeln!(text, "Codecs: {}.\n", labels.join(", "));
    for (path, title) in [
        (
            "call",
            "Our speech toward Codex: each codec encodes, libopus decodes",
        ),
        (
            "discord",
            "The assistant's voice toward Discord: each codec encodes, libopus decodes",
        ),
        (
            "receive",
            "The people in the channel: libopus encodes as a client, each codec decodes",
        ),
        (
            "answer",
            "The Codex answer: libopus encodes, each codec decodes with FEC and concealment",
        ),
    ] {
        let decoder = matches!(path, "receive" | "answer");
        let _ = writeln!(text, "## {title}\n");
        let _ = writeln!(
            text,
            "| codec | loss | SNR | seg. SNR | loudness r | level |{}",
            if decoder {
                " SNR vs libopus decode |"
            } else {
                ""
            }
        );
        let _ = writeln!(
            text,
            "|---|---|---|---|---|---|{}",
            if decoder { "---|" } else { "" }
        );
        for loss in Loss::ALL {
            for (index, codec) in OpusCodec::ALL.iter().enumerate() {
                let Some(sum) = results.quality.get(&(path, index, loss)) else {
                    continue;
                };
                let vs = if decoder {
                    format!(" {:.1} |", sum.vs_libopus / sum.vs_count.max(1) as f64)
                } else {
                    String::new()
                };
                let _ = writeln!(
                    text,
                    "| {} | {} | {:.1} | {:.1} | {:.3} | {:+.1} |{vs}",
                    codec.name(),
                    loss.name(),
                    sum.mean(sum.snr),
                    sum.mean(sum.seg_snr),
                    sum.mean(sum.loudness_r),
                    sum.mean(sum.level_db),
                );
            }
        }
        let timings = if decoder {
            &mut results.decode
        } else {
            &mut results.encode
        };
        let _ = writeln!(
            text,
            "\n| codec | {} per 20 ms: median µs | p99 µs | max µs |\n|---|---|---|---|",
            if decoder { "decode" } else { "encode" }
        );
        for (index, codec) in OpusCodec::ALL.iter().enumerate() {
            if let Some(timing) = timings.get_mut(&(path, index)) {
                let (median, p99, max) = timing.summary();
                let _ = writeln!(
                    text,
                    "| {} | {median:.0} | {p99:.0} | {max:.0} |",
                    codec.name()
                );
            }
        }
        let _ = writeln!(text);
    }
    if results.errors.is_empty() {
        let _ = writeln!(text, "No decode or encode error.");
    } else {
        let _ = writeln!(text, "## Errors\n");
        for error in &results.errors {
            let _ = writeln!(text, "- {error}");
        }
    }
    text
}
