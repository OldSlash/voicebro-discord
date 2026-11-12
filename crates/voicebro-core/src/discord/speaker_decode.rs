//! The app's own decoding of the people in the channel, used when a pure-Rust Opus codec was
//! selected at the join (change `opus-codec-experiment`, design D3). songbird then only
//! removes the transport encryption and DAVE and keeps its playout buffer
//! (`DecodeMode::Decrypt`); this decodes each speaker as songbird's `Decode` mode does: one
//! 48 kHz stereo decoder per SSRC, the payload songbird would decode with the RTP header
//! extension skipped, a concealed frame for each sequence number that never came and for
//! each tick that reports a speaker without a packet, no FEC.

use crate::audio::codec::{
    Channels, CodecError, CodecFailures, CodecPath, FailureSpell, OpusCodec, VoiceDecoder,
    MAX_FRAME_SAMPLES,
};
use crate::audio::FRAME_SAMPLES;
use songbird::packet::rtp::{RtpExtensionPacket, RtpPacket};
use songbird::packet::{Packet, PacketSize};
use std::collections::HashMap;

/// Samples per channel of one concealed 20 ms frame.
const FRAME: usize = FRAME_SAMPLES / 2;
/// Sequence numbers concealed at most before a packet after a gap: the decoder only needs
/// its state to follow, and a long gap would cost a tick many decodes.
const MAX_GAP_CONCEALED: u16 = 10;
/// songbird drops a speaker's decode state after 60 s without a packet (its default
/// `decode_state_timeout`); so does this, counted in 20 ms ticks.
const STALE_TICKS: u64 = 3_000;
const PRUNE_EVERY: u64 = 50;

/// The Opus payload of an RTP packet as songbird decodes it: the packet's payload between
/// `payload_offset` and `payload_end_pad`, after the header extension when the extension bit
/// is set. `None` for a packet songbird would refuse.
pub(crate) fn opus_payload(
    packet: &[u8],
    payload_offset: usize,
    payload_end_pad: usize,
) -> Option<&[u8]> {
    let rtp = RtpPacket::new(packet)?;
    let extension = rtp.get_extension() != 0;
    let header = packet.len().checked_sub(rtp.payload().len())?;
    let body = packet.get(header + payload_offset..header + payload_end_pad)?;
    let start = if extension {
        RtpExtensionPacket::new(body)?.packet_size()
    } else {
        0
    };
    body.get(start..)
}

struct Speaker {
    decoder: VoiceDecoder,
    /// The sequence number the next packet should carry.
    next_seq: Option<u16>,
    /// Room for the longest packet; `len` samples of it (interleaved) are this tick's.
    pcm: Vec<i16>,
    len: usize,
    last_tick: u64,
}

impl Speaker {
    fn conceal(&mut self) -> Result<usize, CodecError> {
        let out = &mut self.pcm[..FRAME * 2];
        self.decoder.decode_i16(&[], out, false)
    }
}

/// Every speaker's decoder for one join.
pub(crate) struct SpeakerDecoders {
    codec: OpusCodec,
    speakers: HashMap<u32, Speaker>,
    tick: u64,
    spell: FailureSpell,
    failures: CodecFailures,
}

impl SpeakerDecoders {
    /// `failures` is where a panic of the codec is reported (the join's).
    pub(crate) fn new(codec: OpusCodec, failures: CodecFailures) -> Self {
        Self {
            codec,
            speakers: HashMap::new(),
            tick: 0,
            spell: FailureSpell::default(),
            failures,
        }
    }

    /// Starts a tick: speakers silent for longer than songbird keeps them are dropped.
    pub(crate) fn begin_tick(&mut self) {
        self.tick += 1;
        if self.tick.is_multiple_of(PRUNE_EVERY) {
            let now = self.tick;
            self.speakers
                .retain(|_, speaker| now.saturating_sub(speaker.last_tick) < STALE_TICKS);
        }
    }

    /// Decodes `ssrc`'s packet of this tick (`Some((sequence, opus payload))`) or conceals
    /// its frame (`None`). On a decoder failure the frame is concealed and the error is
    /// returned for counting; the speaker's frame is ready either way.
    pub(crate) fn decode(
        &mut self,
        ssrc: u32,
        packet: Option<(u16, &[u8])>,
    ) -> Result<(), CodecError> {
        let tick = self.tick;
        let codec = self.codec;
        let speaker = match self.speakers.entry(ssrc) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let decoder = VoiceDecoder::new(codec, Channels::Stereo)?;
                entry.insert(Speaker {
                    decoder,
                    next_seq: None,
                    pcm: vec![0; MAX_FRAME_SAMPLES * 2],
                    len: 0,
                    last_tick: tick,
                })
            }
        };
        speaker.last_tick = tick;
        let result = match packet {
            Some((sequence, payload)) => {
                let missed = speaker
                    .next_seq
                    .map(|next| sequence.wrapping_sub(next))
                    .filter(|gap| *gap < 0x8000)
                    .unwrap_or(0);
                for _ in 0..missed.min(MAX_GAP_CONCEALED) {
                    let _ = speaker.conceal();
                }
                speaker.next_seq = Some(sequence.wrapping_add(1));
                speaker.decoder.decode_i16(payload, &mut speaker.pcm, false)
            }
            None => {
                speaker.next_seq = speaker.next_seq.map(|next| next.wrapping_add(1));
                speaker.conceal()
            }
        };
        match result {
            Ok(count) => {
                speaker.len = (count * 2).min(speaker.pcm.len());
                self.spell.succeeded("receive");
                Ok(())
            }
            Err(error) => {
                self.spell.failed("receive", &error);
                self.failures.report(CodecPath::Receive, &error);
                speaker.len = match speaker.conceal() {
                    Ok(count) => count * 2,
                    Err(_) => {
                        speaker.pcm[..FRAME * 2].fill(0);
                        FRAME * 2
                    }
                };
                Err(error)
            }
        }
    }

    /// `ssrc`'s frame of this tick, interleaved stereo.
    pub(crate) fn frame(&self, ssrc: u32) -> Option<&[i16]> {
        self.speakers
            .get(&ssrc)
            .filter(|speaker| speaker.last_tick == self.tick)
            .and_then(|speaker| speaker.pcm.get(..speaker.len))
    }

    /// Makes `ssrc`'s decoder panic on the next call (pure-Rust codecs only).
    #[cfg(test)]
    pub(crate) fn panic_next_for_test(&mut self, ssrc: u32) {
        if let Some(speaker) = self.speakers.get_mut(&ssrc) {
            speaker.decoder.panic_next_for_test();
        }
    }

    #[cfg(test)]
    pub(crate) fn speaker_count(&self) -> usize {
        self.speakers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::test_signal::Speechlike;
    use crate::audio::codec::{Application, EncoderParams, VoiceEncoder};

    /// A Discord client's stream: stereo VOIP at 64 kbit/s with FEC.
    fn client_packets(seed: u64, frames: usize) -> Vec<Vec<u8>> {
        let mut encoder = VoiceEncoder::new(
            OpusCodec::Libopus,
            EncoderParams {
                channels: Channels::Stereo,
                application: Application::Voip,
                bitrate: 64_000,
                complexity: 10,
                inband_fec: true,
                loss_percent: 10,
            },
        )
        .unwrap();
        let mut signal = Speechlike::new(seed);
        let mut out = vec![0u8; 4_000];
        (0..frames)
            .map(|_| {
                let written = encoder
                    .encode_f32(&signal.stereo_frame(), &mut out)
                    .unwrap();
                out[..written].to_vec()
            })
            .collect()
    }

    /// An RTP packet (version 2, payload type 120) around `body`; with `extension`, a
    /// one-byte-header extension of one 32-bit word inside the body, as Discord sends it.
    fn rtp(sequence: u16, body: &[u8], extension: bool) -> Vec<u8> {
        let mut packet = vec![
            0x80 | if extension { 0x10 } else { 0 },
            120,
            (sequence >> 8) as u8,
            sequence as u8,
            0,
            0,
            0,
            0,
            0,
            0,
            0x30,
            0x39,
        ];
        if extension {
            packet.extend_from_slice(&[0xbe, 0xde, 0x00, 0x01, 0x10, 0xff, 0x00, 0x00]);
        }
        packet.extend_from_slice(body);
        packet
    }

    #[test]
    fn the_payload_is_taken_as_songbird_takes_it() {
        let body = [0xf8, 1, 2, 3];
        let plain = rtp(7, &body, false);
        assert_eq!(opus_payload(&plain, 0, body.len()), Some(&body[..]));
        let extended = rtp(8, &body, true);
        assert_eq!(opus_payload(&extended, 0, body.len() + 8), Some(&body[..]));
        // A crypto suffix (here 4 bytes of tag) is cut off by `payload_end_pad`.
        let mut tagged = rtp(9, &body, false);
        tagged.extend_from_slice(&[9, 9, 9, 9]);
        assert_eq!(opus_payload(&tagged, 0, body.len()), Some(&body[..]));
        assert_eq!(opus_payload(&[0x80, 1], 0, 0), None, "too short for RTP");
    }

    /// The parity test of design D3: the packets of one speaker, with a sequence number
    /// that never came, a tick reported without a packet and header extensions, through
    /// this path with libopus and through `opus2::Decoder` used the way songbird uses it.
    #[test]
    fn libopus_through_this_path_gives_songbirds_samples() {
        let packets = client_packets(21, 30);
        // Ticks: packet index or a missed tick; index 12 never arrives at all.
        let mut ticks: Vec<Option<usize>> = (0..30).map(Some).collect();
        ticks.retain(|tick| *tick != Some(12));
        ticks[20] = None; // a tick reporting the speaker without a packet (index 21 lost)

        let mut ours = SpeakerDecoders::new(OpusCodec::Libopus, CodecFailures::none());
        let mut ours_out = Vec::new();
        let mut reference = opus2::Decoder::new(48_000, opus2::Channels::Stereo).unwrap();
        let mut reference_out = Vec::new();
        let mut reference_next: Option<u16> = None;
        for tick in &ticks {
            ours.begin_tick();
            match tick {
                Some(index) => {
                    let sequence = 1_000 + *index as u16;
                    let wire = rtp(sequence, &packets[*index], index % 3 == 0);
                    let end = wire.len() - 12;
                    let payload = opus_payload(&wire, 0, end).unwrap();
                    ours.decode(77, Some((sequence, payload))).unwrap();
                    // songbird: conceal each missed packet into its buffer, then decode.
                    let missed = reference_next.map_or(0, |next| sequence.wrapping_sub(next));
                    let mut buffer = vec![0i16; 1_920];
                    for _ in 0..missed {
                        reference.decode(&[], &mut buffer, false).unwrap();
                    }
                    let count = reference.decode(payload, &mut buffer, false).unwrap();
                    buffer.truncate(count * 2);
                    reference_out.extend_from_slice(&buffer);
                    reference_next = Some(sequence.wrapping_add(1));
                }
                None => {
                    ours.decode(77, None).unwrap();
                    let mut buffer = vec![0i16; 1_920];
                    let count = reference.decode(&[], &mut buffer, false).unwrap();
                    buffer.truncate(count * 2);
                    reference_out.extend_from_slice(&buffer);
                    reference_next = reference_next.map(|next| next.wrapping_add(1));
                }
            }
            ours_out.extend_from_slice(ours.frame(77).expect("a frame this tick"));
        }
        assert_eq!(ours_out.len(), ticks.len() * FRAME_SAMPLES);
        assert!(ours_out == reference_out, "identical samples");
    }

    #[test]
    fn two_speakers_are_decoded_apart_by_every_codec() {
        let first = client_packets(1, 10);
        let second = client_packets(2, 10);
        for codec in OpusCodec::ALL {
            let mut decoders = SpeakerDecoders::new(codec, CodecFailures::none());
            let mut alone = SpeakerDecoders::new(codec, CodecFailures::none());
            for index in 0..10 {
                decoders.begin_tick();
                alone.begin_tick();
                let sequence = index as u16;
                decoders.decode(1, Some((sequence, &first[index]))).unwrap();
                decoders
                    .decode(2, Some((sequence, &second[index])))
                    .unwrap();
                alone.decode(1, Some((sequence, &first[index]))).unwrap();
                assert_eq!(decoders.frame(1).unwrap().len(), FRAME_SAMPLES, "{codec}");
                assert_eq!(decoders.frame(2).unwrap().len(), FRAME_SAMPLES, "{codec}");
                assert_eq!(
                    decoders.frame(1),
                    alone.frame(1),
                    "{codec}: a second speaker does not touch the first one's decoder"
                );
            }
            assert_eq!(decoders.speaker_count(), 2);
        }
    }

    #[test]
    fn a_missed_tick_gives_a_concealed_frame_and_the_next_packet_decodes() {
        let packets = client_packets(4, 6);
        for codec in OpusCodec::ALL {
            let mut decoders = SpeakerDecoders::new(codec, CodecFailures::none());
            for (index, packet) in packets.iter().enumerate() {
                decoders.begin_tick();
                if index == 3 {
                    decoders.decode(5, None).unwrap();
                } else {
                    decoders.decode(5, Some((index as u16, packet))).unwrap();
                }
                assert_eq!(
                    decoders.frame(5).unwrap().len(),
                    FRAME_SAMPLES,
                    "{codec} tick {index}"
                );
            }
            decoders.begin_tick();
            assert_eq!(
                decoders.frame(5),
                None,
                "no frame on a tick it was not decoded"
            );
        }
    }

    #[test]
    fn a_decoder_failure_conceals_one_speaker_and_is_returned() {
        let packets = client_packets(6, 4);
        let (hub, mut reports) = crate::audio::codec::CodecFailureHub::new();
        {
            let codec = OpusCodec::OpusRs;
            let mut decoders = SpeakerDecoders::new(codec, hub.for_guild(9));
            decoders.begin_tick();
            decoders.decode(1, Some((0, &packets[0]))).unwrap();
            decoders.decode(2, Some((0, &packets[0]))).unwrap();
            decoders.begin_tick();
            decoders.panic_next_for_test(1);
            let error = decoders
                .decode(1, Some((1, &packets[1])))
                .expect_err("the injected panic");
            assert!(error.is_panic(), "{codec}: {error}");
            decoders.decode(2, Some((1, &packets[1]))).unwrap();
            assert_eq!(
                decoders.frame(1).unwrap().len(),
                FRAME_SAMPLES,
                "{codec}: concealed"
            );
            assert_eq!(decoders.frame(2).unwrap().len(), FRAME_SAMPLES);
            decoders.begin_tick();
            decoders
                .decode(1, Some((2, &packets[2])))
                .expect("a fresh decoder after the panic");
            let report = reports.try_recv().expect("the panic is reported");
            assert_eq!((report.guild, report.path), (9, CodecPath::Receive));
            assert!(reports.try_recv().is_err(), "once");
        }
    }

    #[test]
    fn a_decode_error_that_is_not_a_panic_is_not_reported() {
        let (hub, mut reports) = crate::audio::codec::CodecFailureHub::new();
        let mut decoders = SpeakerDecoders::new(OpusCodec::OpusRs, hub.for_guild(3));
        decoders.begin_tick();
        let error = decoders
            .decode(4, Some((0, &[0xff; 3])))
            .expect_err("not Opus");
        assert!(!error.is_panic());
        assert!(reports.try_recv().is_err());
    }

    #[test]
    fn a_silent_speaker_is_dropped_after_songbirds_timeout() {
        let packets = client_packets(8, 1);
        let mut decoders = SpeakerDecoders::new(OpusCodec::Libopus, CodecFailures::none());
        decoders.begin_tick();
        decoders.decode(9, Some((0, &packets[0]))).unwrap();
        for _ in 0..STALE_TICKS + PRUNE_EVERY {
            decoders.begin_tick();
        }
        assert_eq!(decoders.speaker_count(), 0);
    }

    #[test]
    fn a_packet_that_is_not_opus_is_concealed_and_counted() {
        let mut decoders = SpeakerDecoders::new(OpusCodec::Libopus, CodecFailures::none());
        decoders.begin_tick();
        let error = decoders
            .decode(3, Some((0, &[0xff; 3])))
            .expect_err("not Opus");
        assert!(!error.is_panic());
        assert_eq!(decoders.frame(3).unwrap().len(), FRAME_SAMPLES);
    }
}
