//! The Discord leg of the assistant's voice (change `discord-voice-delivery`): the voice
//! server and its region, our SSRC, and what Discord's RTCP reports say about our stream.
//!
//! Discord sends a receiver report about the bot's stream about once a second (probe of
//! 2026-11-10). Each report block about our SSRC feeds the expected loss of the Opus
//! encoder toward Discord ([`DeliveryShared::on_report`]). The connection's session id and
//! token are never kept or logged: only the server's host and our SSRC are.

use crate::audio::opus_out::{DeliveryShared, LossSource};
use crate::audio::{OutputCounters, FRAME_DURATION};
use serde::Serialize;
use serenity::async_trait;
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// No SSRC announced yet.
const NO_SSRC: u64 = u64::MAX;

/// What the status shows of the Discord leg.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DeliveryCounters {
    /// The voice server's host (no port, nothing else of the connection).
    pub voice_server: Option<String>,
    /// The region read from the host: the letters its first label starts with.
    pub region: Option<String>,
    pub packets_encoded: u64,
    /// Packets songbird sent as we encoded them (with our FEC).
    pub passed_through: u64,
    pub encode_errors: u64,
    /// The encoder's expected loss in use, in percent, and where it comes from.
    pub expected_loss_percent: u32,
    pub expected_loss_source: Option<LossSource>,
    /// Report blocks about our stream, and the last one's loss and jitter.
    pub reports: u64,
    pub last_reported_loss_percent: Option<f64>,
    pub last_reported_jitter_ms: Option<f64>,
}

/// One report block of an RTCP sender or receiver report (RFC 3550 6.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportBlock {
    pub ssrc: u32,
    /// Fraction lost since the previous report, of 256.
    pub fraction_lost: u8,
    /// Interarrival jitter in RTP timestamp units (48 kHz).
    pub jitter: u32,
}

/// Calls `each` for every report block in a compound RTCP packet: those of sender (200) and
/// receiver (201) reports. Stops at the first part that is not version 2 RTCP (192-223),
/// which is where the payload ends; a block cut short is skipped.
pub fn for_each_report_block(bytes: &[u8], mut each: impl FnMut(ReportBlock)) {
    let mut at = 0;
    while let Some(header) = bytes.get(at..at + 4) {
        let count = usize::from(header[0] & 0x1f);
        let kind = header[1];
        if header[0] >> 6 != 2 || !(192..=223).contains(&kind) {
            break;
        }
        let words = usize::from(u16::from_be_bytes([header[2], header[3]]));
        let end = (at + (words + 1) * 4).min(bytes.len());
        let first = match kind {
            200 => Some(at + 28),
            201 => Some(at + 8),
            _ => None,
        };
        if let Some(first) = first {
            for index in 0..count {
                let start = first + index * 24;
                let Some(block) = bytes.get(start..start + 24).filter(|_| start + 24 <= end) else {
                    break;
                };
                each(ReportBlock {
                    ssrc: u32::from_be_bytes([block[0], block[1], block[2], block[3]]),
                    fraction_lost: block[4],
                    jitter: u32::from_be_bytes([block[12], block[13], block[14], block[15]]),
                });
            }
        }
        at = end.max(at + 4);
    }
}

/// The host of a voice server address: everything before the port, a path or a query.
pub fn host_of(server: &str) -> &str {
    let server = server
        .trim()
        .trim_start_matches("wss://")
        .trim_start_matches("https://");
    server.split([':', '/', '?']).next().unwrap_or_default()
}

/// The region a voice server's host names. Discord uses two forms: a region name and a
/// number (`finland14009.discord.media` → `finland`), and dash-separated parts with a short
/// code and a number (`c-sin19-5f0d7996.discord.media` → `sin`). The region is the letters
/// of the first part of the first label that is letters followed by a digit; failing that,
/// the letters the label starts with. `None` when there are none.
pub fn region_of(host: &str) -> Option<String> {
    let label = host.split('.').next().unwrap_or_default();
    let leading_letters =
        |part: &str| -> String { part.chars().take_while(char::is_ascii_alphabetic).collect() };
    let coded = label.split('-').find_map(|part| {
        let letters = leading_letters(part);
        let followed_by_digit = part[letters.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit());
        (!letters.is_empty() && followed_by_digit).then_some(letters)
    });
    let letters = coded.unwrap_or_else(|| leading_letters(label));
    (!letters.is_empty()).then(|| letters.to_ascii_lowercase())
}

/// One guild's Discord leg: the server and our SSRC from the connection events, and the
/// shared counters the Opus output encodes with.
pub(crate) struct DeliveryLeg {
    pub(crate) shared: Arc<DeliveryShared>,
    server: Mutex<Option<String>>,
    ssrc: AtomicU64,
}

impl DeliveryLeg {
    pub(crate) fn new(shared: Arc<DeliveryShared>) -> Self {
        Self {
            shared,
            server: Mutex::new(None),
            ssrc: AtomicU64::new(NO_SSRC),
        }
    }

    /// The driver (re)connected: keep the server's host and our SSRC, nothing else.
    pub(crate) fn on_connect(&self, server: &str, ssrc: u32) {
        let host = host_of(server).to_owned();
        if let Ok(mut slot) = self.server.lock() {
            *slot = Some(host);
        }
        self.ssrc.store(u64::from(ssrc), Ordering::Relaxed);
    }

    /// One RTCP packet from Discord: report blocks about our SSRC feed the expected loss;
    /// blocks about other streams, and everything before our SSRC is known, are ignored.
    pub(crate) fn on_rtcp(&self, bytes: &[u8]) {
        let ours = self.ssrc.load(Ordering::Relaxed);
        if ours == NO_SSRC {
            return;
        }
        for_each_report_block(bytes, |block| {
            if u64::from(block.ssrc) == ours {
                self.shared.on_report(block.fraction_lost, block.jitter);
            }
        });
    }

    pub(crate) fn server(&self) -> Option<String> {
        self.server.lock().ok().and_then(|slot| slot.clone())
    }

    pub(crate) fn counters(&self) -> DeliveryCounters {
        let shared = &self.shared;
        let voice_server = self.server();
        let region = voice_server.as_deref().and_then(region_of);
        let reports = shared.reports.load(Ordering::Acquire);
        let measured = shared.measured.load(Ordering::Relaxed);
        DeliveryCounters {
            voice_server,
            region,
            packets_encoded: shared.packets_encoded.load(Ordering::Relaxed),
            passed_through: shared.passed_through(),
            encode_errors: shared.encode_errors.load(Ordering::Relaxed),
            expected_loss_percent: shared.expected_loss.load(Ordering::Relaxed),
            expected_loss_source: Some(if measured {
                LossSource::Measured
            } else {
                LossSource::Default
            }),
            reports,
            last_reported_loss_percent: (reports > 0).then(|| {
                f64::from(shared.last_fraction_lost.load(Ordering::Relaxed)) * 100.0 / 256.0
            }),
            last_reported_jitter_ms: (reports > 0)
                .then(|| f64::from(shared.last_jitter.load(Ordering::Relaxed)) / 48.0),
        }
    }
}

/// The line logged when the bot joins: the voice server and its region.
pub(crate) fn join_line(guild: u64, host: Option<&str>) -> String {
    match host {
        Some(host) => format!(
            "[delivery:{guild}] voice server {host} (region {})",
            region_of(host).as_deref().unwrap_or("unknown")
        ),
        None => format!("[delivery:{guild}] voice server unknown"),
    }
}

/// The line logged when the bot leaves: the leg's counters.
pub(crate) fn leave_line(
    guild: u64,
    counters: &DeliveryCounters,
    output: &OutputCounters,
) -> String {
    let source = match counters.expected_loss_source {
        Some(LossSource::Measured) => "measured",
        _ => "default",
    };
    let reported = match (
        counters.last_reported_loss_percent,
        counters.last_reported_jitter_ms,
    ) {
        (Some(loss), Some(jitter)) => format!(", last loss {loss:.1} %, jitter {jitter:.1} ms"),
        _ => String::new(),
    };
    // Change `transmit-only-when-speaking`: the voice goes out only while it is audible.
    let not_sent = FRAME_DURATION.as_secs_f64() * output.frames_not_sent as f64;
    format!(
        "[delivery:{guild}] voice server {} (region {}): {} packets encoded, {} passed through, {} encode errors; expected loss {} % ({source}); {} reports{reported}; sending turned on {} times, nothing sent for {not_sent:.1} s, {} cut starts",
        counters.voice_server.as_deref().unwrap_or("unknown"),
        counters.region.as_deref().unwrap_or("unknown"),
        counters.packets_encoded,
        counters.passed_through,
        counters.encode_errors,
        counters.expected_loss_percent,
        counters.reports,
        output.sends_started,
        output.cut_starts,
    )
}

/// songbird's connection and RTCP events for one guild's [`DeliveryLeg`].
pub(crate) struct DeliveryHandler(pub(crate) Arc<DeliveryLeg>);

#[async_trait]
impl VoiceEventHandler for DeliveryHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        match ctx {
            EventContext::DriverConnect(data) | EventContext::DriverReconnect(data) => {
                // The server and our SSRC only: the session id stays out.
                self.0.on_connect(data.server, data.ssrc);
            }
            EventContext::RtcpPacket(data) => {
                // The decrypted packet without the trailing pad songbird reports; read to
                // the end, the pad parses as more RTCP.
                let end = data.packet.len().saturating_sub(data.payload_end_pad);
                self.0.on_rtcp(data.packet.get(..end).unwrap_or(&[]));
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::opus_out::DEFAULT_EXPECTED_LOSS;

    const OURS: u32 = 2161;
    const OTHER: u32 = 77;

    /// A receiver report from `sender` with one block per `(ssrc, fraction lost, jitter)`.
    fn receiver_report(sender: u32, blocks: &[(u32, u8, u32)]) -> Vec<u8> {
        let words = 1 + blocks.len() * 6;
        let mut packet = vec![0x80 | blocks.len() as u8, 201];
        packet.extend_from_slice(&(words as u16).to_be_bytes());
        packet.extend_from_slice(&sender.to_be_bytes());
        for (ssrc, lost, jitter) in blocks {
            packet.extend_from_slice(&ssrc.to_be_bytes());
            packet.push(*lost);
            packet.extend_from_slice(&[0, 0, 3]); // cumulative lost
            packet.extend_from_slice(&1000u32.to_be_bytes()); // highest sequence
            packet.extend_from_slice(&jitter.to_be_bytes());
            packet.extend_from_slice(&[0; 8]); // LSR, DLSR
        }
        packet
    }

    /// Percent → fraction lost of 256.
    fn fraction(percent: f64) -> u8 {
        (percent * 256.0 / 100.0).round() as u8
    }

    fn leg() -> DeliveryLeg {
        let leg = DeliveryLeg::new(Arc::new(DeliveryShared::default()));
        leg.on_connect("finland14009.discord.media:443", OURS);
        leg
    }

    /// `packets` sent, then one report about our stream.
    fn report_after(leg: &DeliveryLeg, packets: u64, percent: f64) {
        leg.shared
            .packets_encoded
            .fetch_add(packets, Ordering::Relaxed);
        leg.on_rtcp(&receiver_report(1, &[(OURS, fraction(percent), 48)]));
    }

    #[test]
    fn report_blocks_are_read_from_sender_and_receiver_reports() {
        let mut compound = receiver_report(1, &[(OTHER, 10, 96), (OURS, 25, 48)]);
        // A sender report: header, sender SSRC, 20 bytes of sender info, one block.
        let mut sender = vec![0x81, 200, 0, 12];
        sender.extend_from_slice(&5u32.to_be_bytes());
        sender.extend_from_slice(&[0; 20]);
        sender.extend_from_slice(&OURS.to_be_bytes());
        sender.extend_from_slice(&[
            7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x60, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        compound.extend_from_slice(&sender);
        // Trailing padding that is not RTCP.
        compound.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        let mut blocks = Vec::new();
        for_each_report_block(&compound, |block| blocks.push(block));
        assert_eq!(
            blocks,
            vec![
                ReportBlock {
                    ssrc: OTHER,
                    fraction_lost: 10,
                    jitter: 96
                },
                ReportBlock {
                    ssrc: OURS,
                    fraction_lost: 25,
                    jitter: 48
                },
                ReportBlock {
                    ssrc: OURS,
                    fraction_lost: 7,
                    jitter: 0x60
                },
            ]
        );
        // Cut short: the partial block is skipped, nothing panics.
        let mut cut = Vec::new();
        for_each_report_block(&compound[..20], |block| cut.push(block));
        assert!(cut.is_empty());
    }

    #[test]
    fn no_reports_keep_the_default() {
        let leg = leg();
        leg.shared.packets_encoded.store(5_000, Ordering::Relaxed);
        assert_eq!(
            leg.shared.expected_loss_now(),
            (DEFAULT_EXPECTED_LOSS, LossSource::Default)
        );
        // Reports before our SSRC is known are not about us.
        let unknown = DeliveryLeg::new(Arc::new(DeliveryShared::default()));
        unknown.on_rtcp(&receiver_report(1, &[(OURS, fraction(40.0), 48)]));
        assert_eq!(unknown.shared.reports.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn reports_about_other_streams_are_ignored() {
        let leg = leg();
        leg.shared.packets_encoded.store(50, Ordering::Relaxed);
        leg.on_rtcp(&receiver_report(1, &[(OTHER, fraction(60.0), 4_800)]));
        assert_eq!(leg.shared.reports.load(Ordering::Relaxed), 0);
        assert_eq!(
            leg.shared.expected_loss_now(),
            (DEFAULT_EXPECTED_LOSS, LossSource::Default)
        );
    }

    #[test]
    fn rising_loss_raises_the_setting_at_once() {
        let leg = leg();
        for _ in 0..30 {
            report_after(&leg, 50, 2.0);
        }
        let (quiet, source) = leg.shared.expected_loss_now();
        assert_eq!(source, LossSource::Measured);
        assert!(
            quiet < DEFAULT_EXPECTED_LOSS,
            "settled below the default: {quiet}"
        );
        report_after(&leg, 50, 25.0);
        assert_eq!(leg.shared.expected_loss_now(), (25, LossSource::Measured));
    }

    #[test]
    fn falling_loss_lowers_the_setting_gradually() {
        let leg = leg();
        report_after(&leg, 50, 30.0);
        let mut previous = leg.shared.expected_loss_now().0;
        assert_eq!(previous, 30);
        let mut steps = Vec::new();
        for _ in 0..40 {
            report_after(&leg, 50, 0.0);
            let (now, source) = leg.shared.expected_loss_now();
            assert_eq!(source, LossSource::Measured);
            assert!(now <= previous, "never rises on a lower report");
            steps.push(now);
            previous = now;
        }
        assert!(
            steps[0] >= 25,
            "one quiet report does not drop it: {steps:?}"
        );
        assert!(
            *steps.last().unwrap() <= 2,
            "it follows the quiet reports: {steps:?}"
        );
    }

    #[test]
    fn without_new_reports_the_setting_returns_to_the_default() {
        let leg = leg();
        for _ in 0..30 {
            report_after(&leg, 50, 0.0);
        }
        assert_eq!(leg.shared.expected_loss_now().1, LossSource::Measured);
        // Discord stops reporting while we keep sending.
        leg.shared
            .packets_encoded
            .fetch_add(3_000, Ordering::Relaxed);
        assert_eq!(
            leg.shared.expected_loss_now(),
            (DEFAULT_EXPECTED_LOSS, LossSource::Default)
        );
    }

    #[test]
    fn the_region_is_read_from_the_hosts_first_label() {
        for (server, host, region) in [
            (
                "finland14009.discord.media:443",
                "finland14009.discord.media",
                Some("finland"),
            ),
            (
                "singapore10123.discord.media",
                "singapore10123.discord.media",
                Some("singapore"),
            ),
            (
                "wss://india10045.discord.media:443/?v=8",
                "india10045.discord.media",
                Some("india"),
            ),
            (
                "Rotterdam2.discord.media",
                "Rotterdam2.discord.media",
                Some("rotterdam"),
            ),
            // The form seen for Singapore on 2026-11-10.
            (
                "c-sin19-5f0d7996.discord.media:2096",
                "c-sin19-5f0d7996.discord.media",
                Some("sin"),
            ),
            (
                "c-fra12-ab12.discord.media",
                "c-fra12-ab12.discord.media",
                Some("fra"),
            ),
            (
                "voice-only.example.com",
                "voice-only.example.com",
                Some("voice"),
            ),
            ("1234.example.com:80", "1234.example.com", None),
            ("", "", None),
        ] {
            assert_eq!(host_of(server), host);
            assert_eq!(region_of(host).as_deref(), region, "{server}");
        }
    }

    #[test]
    fn the_join_and_leave_lines_name_the_server_and_the_counters() {
        let leg = leg();
        report_after(&leg, 1_500, 3.9);
        leg.shared.songbird_decodes.store(2, Ordering::Relaxed);
        leg.shared.expected_loss.store(7, Ordering::Relaxed);
        leg.shared.measured.store(true, Ordering::Relaxed);
        let counters = leg.counters();
        assert_eq!(counters.region.as_deref(), Some("finland"));
        assert_eq!(counters.passed_through, 1_498);
        assert_eq!(
            join_line(42, leg.server().as_deref()),
            "[delivery:42] voice server finland14009.discord.media (region finland)"
        );
        let output = OutputCounters {
            sends_started: 4,
            frames_not_sent: 3_021,
            cut_starts: 0,
            ..OutputCounters::default()
        };
        assert_eq!(
            leave_line(42, &counters, &output),
            "[delivery:42] voice server finland14009.discord.media (region finland): 1500 packets encoded, 1498 passed through, 0 encode errors; expected loss 7 % (measured); 1 reports, last loss 3.9 %, jitter 1.0 ms; sending turned on 4 times, nothing sent for 60.4 s, 0 cut starts"
        );
        let status = serde_json::to_value(output).expect("serializes");
        for field in ["sending", "sends_started", "frames_not_sent", "cut_starts"] {
            assert!(status.get(field).is_some(), "the status carries {field}");
        }
    }

    #[test]
    fn the_token_and_session_id_never_reach_what_is_logged() {
        let token = "MTU2ODUzMzUxODYwMTAzMTY5MA.secret-token";
        let session = "f3a9c2d1e8b7a6c5d4e3f2a1b0c9d8e7";
        let leg = DeliveryLeg::new(Arc::new(DeliveryShared::default()));
        // The connection event carries the session id next to the server; only the
        // server's host and the SSRC are taken from it.
        leg.on_connect(
            &format!("india10045.discord.media:443/?session={session}&token={token}"),
            OURS,
        );
        let counters = leg.counters();
        let logged = [
            join_line(1, leg.server().as_deref()),
            leave_line(1, &counters, &OutputCounters::default()),
            serde_json::to_string(&counters).expect("serializes"),
        ];
        for line in logged {
            assert!(!line.contains(token), "{line}");
            assert!(!line.contains(session), "{line}");
        }
    }
}
