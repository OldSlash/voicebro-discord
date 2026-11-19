//! The session gate between Discord and the assistant. One [`RelayGate`] per guild; the
//! [`RelayRegistry`] holds them and the app-wide mute flag. See `docs/ARCHITECTURE.md` 4.3.
//!
//! The gate decides, every 20 ms, whether the channel's mixed frame reaches the assistant:
//! - **mute** ends the session: the call closes, nothing is sent,
//!   the assistant's voice is discarded and wake phrases are ignored; in always mode the
//!   unmute starts a new session. The mute and unmute tones play when earcons are on;
//! - **wake-word mode**: audio flows only after an activation phrase, replayed from the end
//!   of the phrase so the question right after it is not lost; a deactivation phrase, from
//!   the wake engine or from the call's transcript of the people's speech, ends the session
//!   at once, otherwise the idle timeout does;
//! - **speaker names**: when enabled, the assistant is told who starts talking.

use crate::assistant::{AssistantSession, HeardSink, UserText};
use crate::audio::{peak, AudioGains, Earcon, Frame, FrameSender, GainStage, OutputHandle};
use crate::events::{AppEvent, EventBus};
use crate::latency::{self, SharedLatency};
use crate::settings::{AssistantMode, SessionMode, Settings, SpeakerLabels};
use crate::stt::phrases::{decide, Decision};
use crate::stt::transcriber::{Transcript, TranscriptSink};
use crate::wake::spotter::transcript_match_after;
use crate::wake::{
    Phrase, PhraseKind, PhraseOutcome, PhraseSet, PhraseSink, WakeFeed, REACTIVATION_GUARD,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod floor;

/// About 5 s of 20 ms frames kept before an activation phrase is confirmed. A phrase
/// confirmed only by a final result arrives after the whole utterance.
pub const PREROLL_FRAMES: usize = 250;
/// Upper bound for audio waiting to be replayed after activation (about 6 s). Reaching it
/// while the assistant is still not ready ends the session with an error.
pub const MAX_REPLAY_FRAMES: usize = 300;
/// Replay used when the recognizer gives no timing for the phrase.
const DEFAULT_LOOKBACK: Duration = Duration::from_millis(1_000);
/// Extra audio replayed before the phrase end, so the next word is not clipped.
const LOOKBACK_MARGIN: Duration = Duration::from_millis(80);
/// Peak level treated as someone speaking.
pub const VOICE_PEAK_THRESHOLD: i32 = 700;
/// A speaker silent for this long starts a new utterance when they talk again.
const UTTERANCE_GAP: Duration = Duration::from_millis(700);
/// Speakers heard within this window count as talking at the same time.
const CONCURRENT_WINDOW: Duration = Duration::from_millis(400);
/// A deactivation phrase at the end of the call's transcript so far, whose last word may
/// still go on, counts once no newer piece came for this long (design D2).
pub const TRANSCRIPT_HOLD: Duration = Duration::from_millis(300);
/// A turn of the call's transcript that ended this recently is read together with the next
/// one, since the call's voice detection may end a turn at a pause inside a phrase.
pub const PREVIOUS_TURN_WINDOW: Duration = Duration::from_millis(2_000);

#[derive(Debug, Clone, PartialEq)]
pub struct GateSettings {
    pub mode: SessionMode,
    /// Ends a session after this long without input; `None` means only the deactivation
    /// phrase ends it.
    pub idle_timeout: Option<Duration>,
    pub earcons: bool,
    pub earcon_volume: f32,
    pub speaker_labels: SpeakerLabels,
    /// The selected wake model's deactivation phrases in effect (stored or default), which
    /// the call's transcript is checked for. No vocabulary check: the call's recognizer
    /// knows every word.
    pub deactivation_phrases: Vec<Phrase>,
    /// The Whisper mode (change `whisper-mode`, design D2): Whisper's text is the input and
    /// spots the phrases; no audio goes to the assistant.
    pub whisper: bool,
    /// The Whisper mode's phrases in effect.
    pub whisper_phrases: PhraseSet,
    /// `assistant.one_speaker_at_a_time` (change `speaker-turns`, D0): read at the start of
    /// each session; a session never switches halfway.
    pub one_speaker_at_a_time: bool,
}

impl GateSettings {
    pub fn from_settings(settings: &Settings) -> Self {
        let whisper_phrases = settings.whisper.phrases_in_effect();
        Self {
            mode: settings.session.mode,
            idle_timeout: (settings.session.idle_timeout_seconds > 0)
                .then(|| Duration::from_secs(u64::from(settings.session.idle_timeout_seconds))),
            earcons: settings.session.earcons,
            earcon_volume: settings.session.earcon_volume,
            speaker_labels: settings.assistant.speaker_labels,
            deactivation_phrases: Phrase::list(&settings.wake.selected_phrases().deactivation),
            whisper: settings.assistant.mode == AssistantMode::Whisper,
            whisper_phrases: PhraseSet::new(
                &whisper_phrases.activation,
                &whisper_phrases.deactivation,
            ),
            one_speaker_at_a_time: settings.assistant.one_speaker_at_a_time,
        }
    }
}

/// Where a deactivation came from; the gate's log line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeactivationSource {
    WakeEngine,
    CallTranscript,
    /// Whisper's text in the Whisper mode.
    Whisper,
}

impl DeactivationSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::WakeEngine => "the wake engine",
            Self::CallTranscript => "the call's transcript",
            Self::Whisper => "Whisper's text",
        }
    }
}

/// What the gate checks the call's transcript for, as the log states it.
pub fn transcript_phrases_line(settings: &GateSettings) -> String {
    if settings.mode != SessionMode::WakeWord {
        return "the call's transcript ends no session (always mode)".to_owned();
    }
    if settings.deactivation_phrases.is_empty() {
        return "the call's transcript ends no session (no deactivation phrase)".to_owned();
    }
    format!(
        "the call's transcript ends a session on: {}",
        Phrase::texts(&settings.deactivation_phrases).join(", ")
    )
}

/// The gate's line for a deactivation, naming its source (design D5).
pub fn deactivation_line(phrase: &str, source: DeactivationSource) -> String {
    format!(
        "deactivation phrase «{phrase}» from {}; the session ended, waiting for the activation phrase",
        source.label()
    )
}

/// A deactivation phrase at the end of a transcript turn, held until the word is known to
/// have ended.
#[derive(Debug, Clone)]
struct HeardHold {
    turn: u64,
    phrase: String,
    at: Instant,
}

/// A finished turn of the call's transcript.
#[derive(Debug, Clone)]
struct HeardTurn {
    turn: u64,
    text: String,
    at: Instant,
}

/// Reports an assistant failure the gate detected (the replay cap reached while the
/// assistant was not ready). The runtime implements it with its connection errors.
pub trait ErrorReporter: Send + Sync {
    fn report(&self, source: &str, message: Option<String>);
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GateCounters {
    /// Frames the assistant did not take in time (the input queue was full).
    pub input_dropped: u64,
    /// Frames dropped from the replay because the assistant was not ready for too long.
    pub replay_dropped: u64,
    /// Sessions ended because the assistant never became ready.
    pub replay_timeouts: u64,
    /// Times the floor passed to a person who waited (change `speaker-turns`, D8).
    pub floor_passes: u64,
    /// Frames dropped from full waiting buffers.
    pub floor_dropped: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateStatus {
    pub guild_id: String,
    pub mode: SessionMode,
    pub muted: bool,
    /// A session is active: in always mode whenever connected, in wake-word mode after an
    /// activation phrase.
    pub active: bool,
    pub listening_for_wake_word: bool,
    pub session_wanted: bool,
    pub assistant_ready: bool,
    pub last_wake_phrase: Option<String>,
    /// Frames waiting for the assistant to become ready.
    pub replay_held: usize,
    /// People waiting for the floor (change `speaker-turns`).
    pub floor_waiting: usize,
    pub counters: GateCounters,
}

#[derive(Default)]
struct SpeakerState {
    name: Option<String>,
    last_voice: Option<Instant>,
}

struct GateState {
    settings: GateSettings,
    muted: bool,
    active: bool,
    activated_at: Option<Instant>,
    last_user_voice: Option<Instant>,
    last_wake_phrase: Option<String>,
    preroll: VecDeque<Frame>,
    replay: VecDeque<Frame>,
    session_wanted: bool,
    assistant_ready: bool,
    speakers: HashMap<u32, SpeakerState>,
    participants: Vec<String>,
    last_label: Option<Vec<String>>,
    counters: GateCounters,
    /// The guild's receive ticks, numbered from 1, for the wake feed.
    tick: u64,
    /// The last deactivation, from either source: activations within
    /// [`REACTIVATION_GUARD`] of it are ignored.
    deactivated_at: Option<Instant>,
    heard_hold: Option<HeardHold>,
    /// The last finished turn of the call's transcript, read with the next one.
    heard_previous: Option<HeardTurn>,
    /// One speaker at a time (change `speaker-turns`): the floor, the waiting buffers and
    /// the per-speaker rings, kept while the setting is on.
    floor: floor::Floor,
    /// Whether the running session uses the floor, decided when it started (D0); `None`
    /// before its first passing tick.
    floor_on: Option<bool>,
}

impl GateState {
    /// The assistant's voice is discarded while muted and, in wake-word mode, whenever no
    /// session is active, so a deactivation silences it at once.
    /// A deactivation phrase held at the end of the call's transcript silences the voice
    /// at once, so its reply to the phrase is not heard while the gate waits to see whether
    /// the word went on; the voice comes back if it did.
    fn output_muted(&self) -> bool {
        self.muted
            || (self.settings.mode == SessionMode::WakeWord && !self.active)
            || self.heard_hold.is_some()
    }

    /// Mute ends the session: nothing is spent while muted.
    fn wants_session(&self) -> bool {
        !self.muted && (self.settings.mode == SessionMode::Always || self.active)
    }

    fn end_session(&mut self) {
        self.active = false;
        self.activated_at = None;
        self.replay.clear();
        self.last_label = None;
        self.heard_hold = None;
        self.heard_previous = None;
        self.floor.clear();
        self.floor_on = None;
    }

    /// The floor's frame for the call this tick (D1 to D5), with the note it calls for: at
    /// `start` and `changes` every change of the floor, at `off` none (check 6.1 of the
    /// change: with a note at every change the answers named the right person in 7 of 10
    /// calls and the wrong one in none; with a note only for a person who waited, right in
    /// 2 and wrong in 2; without notes, right in 3 and wrong in 5). The person already named
    /// as the caller gets no second note. Silence when nobody holds the floor.
    fn floor_frame(
        &mut self,
        speakers: &[(u32, &[i16])],
        answer_playing: bool,
    ) -> (Frame, Option<Vec<String>>) {
        let labels = self.settings.speaker_labels;
        let hold = if labels == SpeakerLabels::Off {
            0
        } else {
            floor::NOTE_HOLD_TICKS
        };
        let passes = self.floor.passes;
        let dropped = self.floor.dropped;
        let out = self.floor.tick(speakers, answer_playing, hold);
        self.counters.floor_passes += self.floor.passes - passes;
        self.counters.floor_dropped += self.floor.dropped - dropped;
        if let Some(change) = out.change.filter(|change| change.waited) {
            let name = self
                .speakers
                .get(&change.ssrc)
                .and_then(|speaker| speaker.name.clone())
                .unwrap_or_else(|| format!("ssrc {}", change.ssrc));
            crate::detail!(
                "floor",
                "floor: passes to {name}, who waited; {} still waiting; passes {}, waiting frames dropped {}",
                self.floor.waiting(),
                self.counters.floor_passes,
                self.counters.floor_dropped
            );
        }
        let note = out.change.and_then(|change| {
            let wanted = labels != SpeakerLabels::Off;
            let name = self.speakers.get(&change.ssrc)?.name.clone()?;
            (wanted && self.last_label.as_deref() != Some(std::slice::from_ref(&name))).then(|| {
                self.last_label = Some(vec![name.clone()]);
                vec![name]
            })
        });
        (out.frame.unwrap_or_else(Frame::silence), note)
    }

    /// The one deactivation path of both sources (design D4): ends the session, flushes
    /// the assistant's output, queues the falling earcon, logs the source, and starts the
    /// guard against an activation right after it.
    fn deactivate(
        &mut self,
        phrase: &str,
        source: DeactivationSource,
        now: Instant,
        effects: &mut Effects,
    ) {
        self.end_session();
        self.deactivated_at = Some(now);
        effects.flush_output = true;
        effects.earcon = Some(Earcon::Deactivate);
        effects.log = Some(deactivation_line(phrase, source));
    }

    /// What every state change ends with: the tone only with earcons on, the output's mute,
    /// and the session wanted or not.
    fn settle(&mut self, output: &OutputHandle, effects: &mut Effects) {
        if !self.settings.earcons {
            effects.earcon = None;
        }
        output.set_muted(self.output_muted());
        let wanted = self.wants_session();
        if wanted != self.session_wanted {
            self.session_wanted = wanted;
            effects.session = Some(wanted);
        }
        effects.status_changed = true;
    }

    /// Names to announce for this tick, if the set of talkers changed.
    fn labels_for(&mut self, starters: &[u32], now: Instant) -> Option<Vec<String>> {
        if self.settings.speaker_labels != SpeakerLabels::Changes || starters.is_empty() {
            return None;
        }
        let mut names = self
            .speakers
            .iter()
            .filter(|(ssrc, speaker)| {
                starters.contains(ssrc)
                    || speaker
                        .last_voice
                        .is_some_and(|at| now.saturating_duration_since(at) <= CONCURRENT_WINDOW)
            })
            .filter_map(|(_, speaker)| speaker.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        if names.is_empty() || self.last_label.as_ref() == Some(&names) {
            return None;
        }
        self.last_label = Some(names.clone());
        Some(names)
    }

    fn status(&self, guild: u64) -> GateStatus {
        GateStatus {
            guild_id: guild.to_string(),
            mode: self.settings.mode,
            muted: self.muted,
            active: self.wants_session(),
            listening_for_wake_word: !self.muted
                && self.settings.mode == SessionMode::WakeWord
                && !self.active,
            session_wanted: self.session_wanted,
            assistant_ready: self.assistant_ready,
            last_wake_phrase: self.last_wake_phrase.clone(),
            replay_held: self.replay.len(),
            floor_waiting: self.floor.waiting(),
            counters: self.counters,
        }
    }
}

/// What a tick or a phrase decided, so side effects run after the state lock is released.
#[derive(Default)]
struct Effects {
    earcon: Option<Earcon>,
    session: Option<bool>,
    flush_output: bool,
    speakers: Option<Vec<String>>,
    /// Person who said the activation phrase, passed with the call context.
    caller: Option<String>,
    log: Option<String>,
    error: Option<String>,
    status_changed: bool,
}

pub struct RelayGate {
    guild: u64,
    input: FrameSender,
    /// Scales what is sent to the assistant; the wake feed and the pre-roll stay unscaled.
    input_gain: Arc<GainStage>,
    /// The guild's turn latency, told about every frame with speech sent to the assistant.
    latency: SharedLatency,
    output: OutputHandle,
    events: EventBus,
    errors: Option<Arc<dyn ErrorReporter>>,
    state: Mutex<GateState>,
    session: Mutex<Option<Arc<dyn AssistantSession>>>,
    wake: Option<Arc<dyn WakeFeed>>,
}

impl RelayGate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        guild: u64,
        input: FrameSender,
        input_gain: Arc<GainStage>,
        output: OutputHandle,
        settings: GateSettings,
        muted: bool,
        events: EventBus,
        wake: Option<Arc<dyn WakeFeed>>,
        errors: Option<Arc<dyn ErrorReporter>>,
    ) -> Arc<Self> {
        output.set_muted(muted || settings.mode == SessionMode::WakeWord);
        log::info!("gate {guild}: {}", transcript_phrases_line(&settings));
        let state = GateState {
            settings,
            muted,
            active: false,
            activated_at: None,
            last_user_voice: None,
            last_wake_phrase: None,
            preroll: VecDeque::with_capacity(PREROLL_FRAMES),
            replay: VecDeque::new(),
            session_wanted: false,
            assistant_ready: false,
            speakers: HashMap::new(),
            participants: Vec::new(),
            last_label: None,
            counters: GateCounters::default(),
            tick: 0,
            deactivated_at: None,
            heard_hold: None,
            heard_previous: None,
            floor: floor::Floor::new(),
            floor_on: None,
        };
        Arc::new(Self {
            guild,
            input,
            input_gain,
            latency: SharedLatency::default(),
            output,
            events,
            errors,
            state: Mutex::new(state),
            session: Mutex::new(None),
            wake,
        })
    }

    pub fn guild(&self) -> u64 {
        self.guild
    }

    pub fn output(&self) -> &OutputHandle {
        &self.output
    }

    /// The guild's turn latency tracker; the output reports playback to the same one.
    pub fn latency(&self) -> SharedLatency {
        Arc::clone(&self.latency)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, GateState> {
        self.state.lock().expect("gate state lock")
    }

    /// Installs the assistant session and tells it right away whether a session is wanted.
    pub fn set_session(&self, session: Arc<dyn AssistantSession>) {
        let wanted = {
            let mut state = self.lock();
            state.session_wanted = state.wants_session();
            state.session_wanted
        };
        *self.session.lock().expect("gate session lock") = Some(Arc::clone(&session));
        if wanted {
            if let Some(participants) = self.context_participants() {
                session.call_context(None, &participants);
            }
        }
        session.session_wanted(wanted);
        self.publish_status();
    }

    /// Participants to share with the assistant, unless speaker labels are off.
    fn context_participants(&self) -> Option<Vec<String>> {
        let state = self.lock();
        (state.settings.speaker_labels != SpeakerLabels::Off).then(|| state.participants.clone())
    }

    pub fn set_participants(&self, names: &[String]) {
        self.lock().participants = names.to_vec();
    }

    pub fn participants(&self) -> Vec<String> {
        self.lock().participants.clone()
    }

    /// The speaker names known so far, for a gate that replaces this one.
    pub fn speaker_names(&self) -> Vec<(u32, String)> {
        self.lock()
            .speakers
            .iter()
            .filter_map(|(ssrc, speaker)| speaker.name.clone().map(|name| (*ssrc, name)))
            .collect()
    }

    pub fn set_speaker_name(&self, ssrc: u32, name: &str) {
        self.lock().speakers.entry(ssrc).or_default().name = Some(name.to_owned());
    }

    /// Handles one Discord voice tick: the mixed frame plus each speaker's own audio.
    pub fn process_tick(&self, mixed: &Frame, speakers: &[(u32, &[i16])]) {
        let now = Instant::now();
        let assistant_signal = self.output.last_signal();
        let (ready, working) = self
            .session
            .lock()
            .expect("gate session lock")
            .as_ref()
            .map(|session| (session.ready(), session.working()))
            .unwrap_or((false, false));
        let mut effects = Effects::default();
        let (frame, feed_wake, tick) = {
            let mut state = self.lock();
            state.tick += 1;
            let tick = state.tick;
            if state.assistant_ready != ready {
                state.assistant_ready = ready;
                effects.status_changed = true;
            }
            // A deactivation phrase held at the end of the transcript counts once no newer
            // piece came for the hold's time: the word did not go on.
            if let Some(hold) = state.heard_hold.clone() {
                if now.saturating_duration_since(hold.at) >= TRANSCRIPT_HOLD {
                    state.deactivate(
                        &hold.phrase,
                        DeactivationSource::CallTranscript,
                        now,
                        &mut effects,
                    );
                }
            }
            if mixed.peak() >= VOICE_PEAK_THRESHOLD {
                state.last_user_voice = Some(now);
            }
            let mut starters = Vec::new();
            for (ssrc, samples) in speakers {
                if peak(samples) < VOICE_PEAK_THRESHOLD {
                    continue;
                }
                let speaker = state.speakers.entry(*ssrc).or_default();
                let new_utterance = speaker
                    .last_voice
                    .is_none_or(|at| now.saturating_duration_since(at) > UTTERANCE_GAP);
                speaker.last_voice = Some(now);
                if new_utterance {
                    starters.push(*ssrc);
                }
            }
            if state.preroll.len() >= PREROLL_FRAMES {
                state.preroll.pop_front();
            }
            state.preroll.push_back(mixed.clone());
            // One speaker at a time (change `speaker-turns`, D6): each speaker's own pre-roll,
            // kept while the setting is on, for the next activation.
            if state.settings.one_speaker_at_a_time && !state.settings.whisper && !state.muted {
                state.floor.remember(tick, speakers);
            }
            // The assistant's voice played within 200 ms: a waiting buffer must not cut it (D4).
            let answer_playing = assistant_signal
                .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(200));
            let mut floor_note = None;

            // The Whisper mode hears the channel in both session modes: its text is the input.
            let feed_wake = !state.muted
                && (state.settings.whisper || state.settings.mode == SessionMode::WakeWord);
            let mut passing = false;
            let frame: Option<Frame> = if state.muted {
                // Muted: no session, nothing is sent.
                None
            } else if state.settings.whisper {
                // Whisper mode: questions go as text; only the idle timeout is checked here.
                // A turn in progress or its voice counts as activity.
                if working {
                    state.last_user_voice = Some(now);
                }
                let last_activity = [state.activated_at, state.last_user_voice, assistant_signal]
                    .into_iter()
                    .flatten()
                    .max()
                    .unwrap_or(now);
                let timed_out = state.settings.mode == SessionMode::WakeWord
                    && state.active
                    && state.settings.idle_timeout.is_some_and(|timeout| {
                        now.saturating_duration_since(last_activity) > timeout
                    });
                if timed_out {
                    state.end_session();
                    effects.flush_output = true;
                    effects.earcon = Some(Earcon::Deactivate);
                    effects.log = Some(
                        "session ended after no input; waiting for the activation phrase"
                            .to_owned(),
                    );
                }
                None
            } else if state.settings.mode == SessionMode::Always {
                passing = true;
                // The session decides once whether it uses the floor (D0).
                let setting = state.settings.one_speaker_at_a_time;
                if *state.floor_on.get_or_insert(setting) {
                    let (frame, note) = state.floor_frame(speakers, answer_playing);
                    floor_note = note;
                    Some(frame)
                } else {
                    Some(mixed.clone())
                }
            } else if state.active {
                // The assistant's own answer also counts as activity, so a long reply is
                // never cut by the timeout.
                let last_activity = [state.activated_at, state.last_user_voice, assistant_signal]
                    .into_iter()
                    .flatten()
                    .max()
                    .unwrap_or(now);
                let timed_out = state
                    .settings
                    .idle_timeout
                    .is_some_and(|timeout| now.saturating_duration_since(last_activity) > timeout);
                if timed_out {
                    state.end_session();
                    effects.earcon = Some(Earcon::Deactivate);
                    effects.log = Some(
                        "session ended after no input; waiting for the activation phrase"
                            .to_owned(),
                    );
                    None
                } else if !ready {
                    let live = if state.floor_on == Some(true) {
                        let (frame, note) = state.floor_frame(speakers, answer_playing);
                        floor_note = note;
                        frame
                    } else {
                        mixed.clone()
                    };
                    if state.replay.len() >= MAX_REPLAY_FRAMES {
                        // Nothing waits forever: drop the held audio and end the session.
                        let dropped = state.replay.len() as u64;
                        state.counters.replay_dropped += dropped;
                        state.counters.replay_timeouts += 1;
                        state.end_session();
                        effects.earcon = Some(Earcon::Deactivate);
                        effects.error =
                            Some("the assistant did not become ready in time".to_owned());
                        effects.log = Some(format!(
                            "the assistant did not become ready within {:.1} s; {dropped} frames dropped and the session ended",
                            MAX_REPLAY_FRAMES as f64 * 0.02
                        ));
                    } else {
                        state.replay.push_back(live);
                    }
                    None
                } else {
                    passing = true;
                    let live = if state.floor_on == Some(true) {
                        let (frame, note) = state.floor_frame(speakers, answer_playing);
                        floor_note = note;
                        frame
                    } else {
                        mixed.clone()
                    };
                    state.replay.push_back(live);
                    let next = state.replay.pop_front();
                    // Catch up with live audio by skipping silent backlog frames.
                    while state.replay.len() > 1
                        && state
                            .replay
                            .front()
                            .is_some_and(|frame| frame.peak() < VOICE_PEAK_THRESHOLD)
                    {
                        state.replay.pop_front();
                    }
                    next
                }
            } else {
                None
            };
            if passing {
                // With the floor the note follows the floor (D5); otherwise the set of people
                // who started talking.
                effects.speakers = if state.floor_on == Some(true) {
                    floor_note
                } else {
                    state.labels_for(&starters, now)
                };
            } else if floor_note.is_some() {
                // A pass while the replay is held: the note goes out ahead of the buffer.
                effects.speakers = floor_note;
            }
            self.output.set_muted(state.output_muted());
            let wanted = state.wants_session();
            if wanted != state.session_wanted {
                state.session_wanted = wanted;
                effects.session = Some(wanted);
            }
            if effects.earcon.is_some() && !state.settings.earcons {
                effects.earcon = None;
            }
            (frame, feed_wake, tick)
        };

        if feed_wake {
            if let Some(wake) = self.wake.as_ref() {
                for (ssrc, samples) in speakers {
                    wake.feed(self.guild, *ssrc, tick, samples);
                }
                // After the speakers' frames: the engine counts absence in these ticks.
                wake.tick(self.guild, tick);
            }
        }
        if let Some(mut frame) = frame {
            // Speech as the gate counts it, before the gain.
            if frame.peak() >= VOICE_PEAK_THRESHOLD {
                let event = self.latency.lock().expect("latency lock").speech(now);
                if let Some(event) = event {
                    crate::detail!("latency", "{}", latency::log_line(self.guild, &event));
                }
            }
            // Live and replayed frames alike; the pre-roll and the wake feed keep the
            // channel's own level.
            self.input_gain.scale_frame(&mut frame);
            if self.input.send(frame) {
                self.lock().counters.input_dropped += 1;
            }
        }
        self.apply_effects(effects);
    }

    /// Called by the wake worker when a phrase is confirmed. `since_phrase_end` is how long
    /// ago the phrase ended; after an activation the replay starts there, so the question
    /// that follows reaches the assistant but the phrase itself does not.
    pub fn on_phrase(
        &self,
        kind: PhraseKind,
        phrase: &str,
        ssrc: u32,
        since_phrase_end: Duration,
    ) -> PhraseOutcome {
        self.on_phrase_at(kind, phrase, ssrc, since_phrase_end, Instant::now())
    }

    /// [`Self::on_phrase`] at a given instant, so tests can step past the guard.
    pub fn on_phrase_at(
        &self,
        kind: PhraseKind,
        phrase: &str,
        ssrc: u32,
        since_phrase_end: Duration,
        now: Instant,
    ) -> PhraseOutcome {
        let mut effects = Effects::default();
        {
            let mut state = self.lock();
            // In the Whisper mode Whisper's text spots the phrases; Vosk is off.
            if state.muted || state.settings.mode != SessionMode::WakeWord || state.settings.whisper
            {
                return PhraseOutcome::NotListening;
            }
            match kind {
                // Activation only counts while waiting; deactivation only while active.
                PhraseKind::Activate if state.active => return PhraseOutcome::AlreadyInState,
                PhraseKind::Deactivate if !state.active => return PhraseOutcome::AlreadyInState,
                PhraseKind::Activate
                    if state.deactivated_at.is_some_and(|at| {
                        now.saturating_duration_since(at) < REACTIVATION_GUARD
                    }) =>
                {
                    return PhraseOutcome::RightAfterDeactivation;
                }
                PhraseKind::Activate => {
                    state.active = true;
                    state.activated_at = Some(now);
                    state.last_wake_phrase = Some(phrase.to_owned());
                    let caller = state
                        .speakers
                        .get(&ssrc)
                        .and_then(|speaker| speaker.name.clone());
                    // The caller is named in the call context, so no separate note.
                    state.last_label = caller.clone().map(|name| vec![name]);
                    effects.caller = caller;
                    let lookback = if since_phrase_end.as_millis() < 20 * PREROLL_FRAMES as u128 {
                        since_phrase_end
                    } else {
                        DEFAULT_LOOKBACK
                    } + LOOKBACK_MARGIN;
                    let wanted = (lookback.as_millis() as usize).div_ceil(20);
                    // One speaker at a time (change `speaker-turns`, D6): the caller holds the
                    // floor and only their speech is replayed; what others said since the end
                    // of the phrase waits in their buffers.
                    let use_floor = state.settings.one_speaker_at_a_time;
                    state.floor_on = Some(use_floor);
                    if use_floor {
                        state.replay = state.floor.activate(ssrc, wanted);
                    } else {
                        let frames = wanted.min(state.preroll.len());
                        let skip = state.preroll.len() - frames;
                        state.replay = state.preroll.iter().skip(skip).cloned().collect();
                    }
                    effects.earcon = Some(Earcon::Activate);
                    effects.log = Some(format!(
                        "activation phrase «{phrase}» recognized; the assistant is listening"
                    ));
                }
                PhraseKind::Deactivate => {
                    state.deactivate(phrase, DeactivationSource::WakeEngine, now, &mut effects);
                }
            }
            state.settle(&self.output, &mut effects);
        }
        self.apply_effects(effects);
        PhraseOutcome::Applied
    }

    /// The people's speech the call transcribed, joined per turn (design D2): the turn's
    /// text so far, or its final text with `done`. In wake-word mode, during a session and
    /// with mute off, a deactivation phrase in it ends the session through the same path as
    /// the wake engine's. A phrase whose last word may still go on is held until the next
    /// piece, the turn's end or [`TRANSCRIPT_HOLD`] without a newer piece.
    pub fn on_heard(&self, turn: u64, text: &str, done: bool) -> PhraseOutcome {
        self.on_heard_at(turn, text, done, Instant::now())
    }

    pub fn on_heard_at(&self, turn: u64, text: &str, done: bool, now: Instant) -> PhraseOutcome {
        let mut effects = Effects::default();
        {
            let mut state = self.lock();
            if state.muted || state.settings.mode != SessionMode::WakeWord {
                state.heard_hold = None;
                return PhraseOutcome::NotListening;
            }
            if !state.active {
                state.heard_hold = None;
                return PhraseOutcome::AlreadyInState;
            }
            // A hold from an earlier turn: that turn ended, so its last word did too.
            let ended = state
                .heard_hold
                .as_ref()
                .filter(|hold| hold.turn != turn)
                .map(|hold| hold.phrase.clone());
            // The call's own voice detection may cut a phrase at a pause ("Отбой" | ",
            // братан" as two turns): the previous turn, when it ended just before, is read
            // with this one, and only a match that ends in this turn counts.
            let previous = state
                .heard_previous
                .as_ref()
                .filter(|previous| {
                    previous.turn != turn
                        && now.saturating_duration_since(previous.at) <= PREVIOUS_TURN_WINDOW
                })
                .map(|previous| previous.text.clone());
            let (joined, earlier_words) = match previous {
                Some(previous) => (
                    format!("{previous} {text}"),
                    crate::settings::phrase_words(&previous).len(),
                ),
                None => (text.to_owned(), 0),
            };
            let found = transcript_match_after(
                &joined,
                &state.settings.deactivation_phrases,
                earlier_words,
            );
            if done {
                state.heard_previous = Some(HeardTurn {
                    turn,
                    text: text.to_owned(),
                    at: now,
                });
            }
            let confirmed = match (ended, found) {
                (Some(phrase), _) => Some(phrase),
                (None, Some(found)) if !found.open || done => Some(found.phrase),
                (None, Some(found)) => {
                    state.heard_hold = Some(HeardHold {
                        turn,
                        phrase: found.phrase,
                        at: now,
                    });
                    None
                }
                (None, None) => {
                    state.heard_hold = None;
                    None
                }
            };
            let Some(phrase) = confirmed else {
                // Held: the voice is silenced until the hold ends either way.
                self.output.set_muted(state.output_muted());
                return PhraseOutcome::AlreadyInState;
            };
            state.deactivate(
                &phrase,
                DeactivationSource::CallTranscript,
                now,
                &mut effects,
            );
            state.settle(&self.output, &mut effects);
        }
        self.apply_effects(effects);
        PhraseOutcome::Applied
    }

    /// What Whisper heard one speaker say, in the Whisper mode (change `whisper-mode`,
    /// design D4): an activation phrase while waiting starts the session, and the words
    /// after it are the first question; a deactivation phrase during a session ends it; any
    /// other utterance during a session (always, in always mode) is a question. While muted
    /// or outside the Whisper mode nothing happens.
    pub fn on_transcript(&self, transcript: Transcript) -> Decision {
        self.on_transcript_at(transcript, Instant::now())
    }

    pub fn on_transcript_at(&self, transcript: Transcript, now: Instant) -> Decision {
        let mut effects = Effects::default();
        let (decision, question, speaker) = {
            let mut state = self.lock();
            if state.muted || !state.settings.whisper {
                return Decision::Ignore;
            }
            let decision = decide(
                &transcript.text,
                state.settings.mode,
                state.active,
                &state.settings.whisper_phrases,
            );
            let speaker = state
                .speakers
                .get(&transcript.ssrc)
                .and_then(|speaker| speaker.name.clone());
            let question = match &decision {
                Decision::Activate { phrase, .. }
                    if state.deactivated_at.is_some_and(|at| {
                        now.saturating_duration_since(at) < REACTIVATION_GUARD
                    }) =>
                {
                    let _ = phrase;
                    return Decision::Ignore;
                }
                Decision::Activate { phrase, question } => {
                    state.active = true;
                    state.activated_at = Some(now);
                    state.last_wake_phrase = Some(phrase.clone());
                    state.last_label = speaker.clone().map(|name| vec![name]);
                    effects.caller = speaker.clone();
                    effects.earcon = Some(Earcon::Activate);
                    effects.log = Some(format!(
                        "activation phrase «{phrase}» in Whisper's text; the assistant is listening"
                    ));
                    state.settle(&self.output, &mut effects);
                    question.clone()
                }
                Decision::Deactivate { phrase } => {
                    state.deactivate(phrase, DeactivationSource::Whisper, now, &mut effects);
                    state.settle(&self.output, &mut effects);
                    None
                }
                Decision::Question(text) => Some(text.clone()),
                Decision::Ignore => None,
            };
            if question.is_some() {
                state.last_user_voice = Some(now);
            }
            (decision, question, speaker)
        };
        self.apply_effects(effects);
        if let Some(text) = question {
            let session = self.session.lock().expect("gate session lock").clone();
            if let Some(session) = session {
                session.user_text(UserText {
                    speaker,
                    text,
                    language: transcript.language,
                    speech_end: transcript.speech_end,
                    text_at: transcript.text_at,
                    audio_seconds: transcript.audio_seconds,
                    model: transcript.model,
                    device: transcript.device,
                    latency: Arc::clone(&self.latency),
                });
            }
        }
        decision
    }

    pub fn set_muted(&self, muted: bool) {
        let mut effects = Effects::default();
        {
            let mut state = self.lock();
            if state.muted == muted {
                return;
            }
            state.muted = muted;
            if muted {
                // The session ends with the mute; in wake-word mode the next one needs the
                // activation phrase again.
                state.end_session();
                effects.flush_output = true;
            }
            self.output.set_muted(state.output_muted());
            let wanted = state.wants_session();
            if wanted != state.session_wanted {
                state.session_wanted = wanted;
                effects.session = Some(wanted);
            }
            if state.settings.earcons {
                effects.earcon = Some(if muted { Earcon::Mute } else { Earcon::Unmute });
            }
            effects.log = Some(if muted {
                "assistant muted: the session ended and nothing is sent".to_owned()
            } else if state.settings.mode == SessionMode::Always {
                "assistant unmuted: a new session starts".to_owned()
            } else {
                "assistant unmuted: waiting for the activation phrase".to_owned()
            });
            effects.status_changed = true;
        }
        self.apply_effects(effects);
    }

    pub fn update_settings(&self, settings: GateSettings) {
        let mut effects = Effects::default();
        {
            let mut state = self.lock();
            if state.settings.mode != settings.mode || state.settings.whisper != settings.whisper {
                state.end_session();
                effects.flush_output = true;
            }
            // A phrase change never ends a session; a held match of the old list is dropped.
            if state.settings.deactivation_phrases != settings.deactivation_phrases {
                state.heard_hold = None;
                log::info!(
                    "gate {}: {}",
                    self.guild,
                    transcript_phrases_line(&settings)
                );
            }
            state.settings = settings;
            self.output.set_muted(state.output_muted());
            let wanted = state.wants_session();
            if wanted != state.session_wanted {
                state.session_wanted = wanted;
                effects.session = Some(wanted);
            }
            effects.status_changed = true;
        }
        self.apply_effects(effects);
    }

    pub fn status(&self) -> GateStatus {
        self.lock().status(self.guild)
    }

    fn publish_status(&self) {
        self.events.publish(AppEvent::Gate(Box::new(self.status())));
    }

    fn apply_effects(&self, effects: Effects) {
        if effects.flush_output {
            self.output.flush();
        }
        if let Some(earcon) = effects.earcon {
            let volume = self.lock().settings.earcon_volume;
            self.output.play_earcon(earcon, volume);
        }
        let session = self.session.lock().expect("gate session lock").clone();
        if let Some(session) = session {
            if let Some(names) = effects.speakers.as_ref() {
                session.speakers_changed(names);
            }
            if effects.session == Some(true) {
                if let Some(participants) = self.context_participants() {
                    session.call_context(effects.caller.clone(), &participants);
                }
            }
            if let Some(wanted) = effects.session {
                session.session_wanted(wanted);
            }
        }
        if let Some(message) = effects.log {
            self.events.log(format!("gate {}: {message}", self.guild));
        }
        if let Some(error) = effects.error {
            if let Some(errors) = self.errors.as_ref() {
                errors.report("assistant", Some(error));
            }
        }
        if effects.status_changed || effects.session.is_some() || effects.earcon.is_some() {
            self.publish_status();
        }
    }
}

/// All gates of guilds the bot is in a voice channel of, plus the app-wide mute state
/// shared by the hotkey and the mute buttons and the two gains shared by every gate and
/// every output.
#[derive(Clone, Default)]
pub struct RelayRegistry {
    gates: Arc<Mutex<BTreeMap<u64, Arc<RelayGate>>>>,
    muted: Arc<AtomicBool>,
    gains: AudioGains,
}

impl RelayRegistry {
    pub fn gains(&self) -> &AudioGains {
        &self.gains
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    pub fn register(&self, gate: Arc<RelayGate>) {
        gate.set_muted(self.is_muted());
        self.gates
            .lock()
            .expect("registry lock")
            .insert(gate.guild(), gate);
    }

    pub fn unregister(&self, guild: u64) -> Option<Arc<RelayGate>> {
        self.gates.lock().expect("registry lock").remove(&guild)
    }

    pub fn get(&self, guild: u64) -> Option<Arc<RelayGate>> {
        self.gates
            .lock()
            .expect("registry lock")
            .get(&guild)
            .cloned()
    }

    pub fn all(&self) -> Vec<Arc<RelayGate>> {
        self.gates
            .lock()
            .expect("registry lock")
            .values()
            .cloned()
            .collect()
    }

    /// Sets the app-wide mute and pushes it into every gate. Returns whether it changed.
    pub fn set_muted(&self, muted: bool) -> bool {
        let previous = self.muted.swap(muted, Ordering::AcqRel);
        for gate in self.all() {
            gate.set_muted(muted);
        }
        previous != muted
    }

    pub fn toggle_muted(&self) -> bool {
        let next = !self.is_muted();
        self.set_muted(next);
        next
    }

    pub fn apply_settings(&self, settings: &GateSettings) {
        for gate in self.all() {
            gate.update_settings(settings.clone());
        }
    }

    pub fn statuses(&self) -> BTreeMap<String, GateStatus> {
        self.all()
            .into_iter()
            .map(|gate| (gate.guild().to_string(), gate.status()))
            .collect()
    }

    /// The last measured turns of every guild the bot is in.
    pub fn latency_reports(&self) -> BTreeMap<String, Vec<latency::TurnReport>> {
        self.all()
            .into_iter()
            .map(|gate| {
                let turns = gate.latency.lock().expect("latency lock").recent();
                (gate.guild().to_string(), turns)
            })
            .collect()
    }
}

impl PhraseSink for RelayRegistry {
    fn on_phrase(
        &self,
        guild: u64,
        kind: PhraseKind,
        phrase: &str,
        ssrc: u32,
        since_phrase_end: Duration,
    ) -> PhraseOutcome {
        self.get(guild).map_or(PhraseOutcome::NotListening, |gate| {
            gate.on_phrase(kind, phrase, ssrc, since_phrase_end)
        })
    }
}

/// What Whisper heard reaches the guild's gate (the Whisper mode).
impl TranscriptSink for RelayRegistry {
    fn on_transcript(&self, transcript: Transcript) {
        if let Some(gate) = self.get(transcript.guild) {
            gate.on_transcript(transcript);
        }
    }
}

/// The call's transcripts of the people's speech reach the guild's gate.
impl HeardSink for RelayRegistry {
    fn on_heard(&self, guild: u64, turn: u64, text: &str, done: bool) {
        if let Some(gate) = self.get(guild) {
            gate.on_heard(turn, text, done);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{Assistant, StubAssistant};
    use crate::audio::{frame_channel, output_pair, AudioPorts, FrameReceiver, INPUT_CAPACITY};
    use std::sync::atomic::AtomicUsize;

    /// Counts the blocks fed to the wake engine, keeps the last one and records the ticks:
    /// each frame's and each end of a tick.
    struct CountingFeed(AtomicUsize, Mutex<Vec<i16>>, Mutex<Vec<(u64, Option<u32>)>>);

    impl WakeFeed for CountingFeed {
        fn feed(&self, _: u64, ssrc: u32, tick: u64, samples: &[i16]) {
            self.0.fetch_add(1, Ordering::Relaxed);
            *self.1.lock().unwrap() = samples.to_vec();
            self.2.lock().unwrap().push((tick, Some(ssrc)));
        }
        fn tick(&self, _: u64, tick: u64) {
            self.2.lock().unwrap().push((tick, None));
        }
    }

    impl RelayGate {
        /// Whether a phrase changed the session, for the tests that predate the outcome.
        fn phrase_applied(
            &self,
            kind: PhraseKind,
            phrase: &str,
            ssrc: u32,
            since_phrase_end: Duration,
        ) -> bool {
            self.on_phrase(kind, phrase, ssrc, since_phrase_end) == PhraseOutcome::Applied
        }
    }

    #[derive(Default)]
    struct Errors(Mutex<Vec<(String, Option<String>)>>);

    impl ErrorReporter for Errors {
        fn report(&self, source: &str, message: Option<String>) {
            self.0.lock().unwrap().push((source.to_owned(), message));
        }
    }

    fn settings(mode: SessionMode, timeout: Option<Duration>) -> GateSettings {
        GateSettings {
            mode,
            idle_timeout: timeout,
            earcons: true,
            earcon_volume: 0.2,
            speaker_labels: SpeakerLabels::Changes,
            deactivation_phrases: Phrase::list(&["dismiss".to_owned()]),
            whisper: false,
            whisper_phrases: PhraseSet::default(),
            one_speaker_at_a_time: false,
        }
    }

    /// A gate in the Whisper mode with "hey bro" and "goodbye".
    fn whisper_rig(mode: SessionMode, timeout: Option<Duration>) -> Rig {
        let rig = rig(mode, timeout, Duration::ZERO);
        let mut whisper = settings(mode, timeout);
        whisper.whisper = true;
        whisper.whisper_phrases = PhraseSet::new(&["hey bro".to_owned()], &["goodbye".to_owned()]);
        rig.gate.update_settings(whisper);
        rig
    }

    fn heard(text: &str) -> Transcript {
        Transcript {
            guild: 1,
            ssrc: 7,
            text: text.to_owned(),
            language: Some("en".to_owned()),
            speech_end: Instant::now(),
            text_at: Instant::now(),
            audio_seconds: 2.0,
            transcribe_ms: 300,
            model: "whisper-base-q8_0".to_owned(),
            device: "cpu".to_owned(),
        }
    }

    /// Scenario "Waiting in Whisper mode": every speaker reaches Whisper, nothing reaches
    /// the assistant's input, and the output is muted; also in always mode.
    #[test]
    fn whisper_mode_feeds_whisper_and_sends_the_assistant_no_audio() {
        for mode in [SessionMode::WakeWord, SessionMode::Always] {
            let rig = whisper_rig(mode, None);
            let loud = vec![3_000i16; 1920];
            for _ in 0..10 {
                rig.gate.process_tick(&voice(), &[(7, &loud), (8, &loud)]);
            }
            assert_eq!(rig.feed.0.load(Ordering::Relaxed), 20, "{mode:?}");
            assert!(
                drain(&rig.input).is_empty(),
                "{mode:?}: no audio to the input"
            );
            assert_eq!(rig.output.is_muted(), mode == SessionMode::WakeWord);
        }
    }

    /// Scenario "Muted in Whisper mode": no audio reaches Whisper and no text counts.
    #[test]
    fn muted_whisper_mode_feeds_nothing_and_ignores_text() {
        let rig = whisper_rig(SessionMode::WakeWord, None);
        rig.gate.set_muted(true);
        rig.gate
            .process_tick(&voice(), &[(7, &vec![3_000i16; 1920])]);
        assert_eq!(rig.feed.0.load(Ordering::Relaxed), 0);
        assert_eq!(
            rig.gate.on_transcript(heard("Hey bro, hi")),
            Decision::Ignore
        );
        assert!(!rig.gate.status().active);
    }

    /// Scenarios "Phrase and question in one breath", "Phrase alone", "Talk while waiting"
    /// and a deactivation inside a sentence.
    #[test]
    fn whisper_text_starts_asks_and_ends_the_session() {
        let rig = whisper_rig(SessionMode::WakeWord, None);
        rig.gate.set_speaker_name(7, "Alex");
        rig.gate
            .set_participants(&["Alex".to_owned(), "Sam".to_owned()]);
        assert_eq!(
            rig.gate.on_transcript(heard("I think we push left.")),
            Decision::Ignore
        );
        assert!(rig.stub.texts.lock().unwrap().is_empty());
        rig.gate
            .on_transcript(heard("Hey bro, what is the weather in Bangkok?"));
        assert!(rig.gate.status().active);
        assert!(!rig.output.is_muted());
        assert_eq!(
            rig.stub
                .contexts
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .0
                .as_deref(),
            Some("Alex"),
            "the caller goes with the context before the question"
        );
        assert_eq!(
            *rig.stub.texts.lock().unwrap(),
            vec![(
                Some("Alex".to_owned()),
                "what is the weather in Bangkok?".to_owned()
            )]
        );
        rig.gate.on_transcript(heard("And in London?"));
        assert_eq!(rig.stub.texts.lock().unwrap().len(), 2);
        assert!(matches!(
            rig.gate.on_transcript(heard("Ok thanks, goodbye.")),
            Decision::Deactivate { .. }
        ));
        assert!(!rig.gate.status().active);
        assert_eq!(
            rig.stub.texts.lock().unwrap().len(),
            2,
            "nothing more was asked"
        );
        assert_eq!(rig.stub.wanted_log.lock().unwrap().last(), Some(&false));
        // Right after the goodbye another member's microphone may carry its end.
        assert_eq!(rig.gate.on_transcript(heard("Hey bro.")), Decision::Ignore);
    }

    /// Scenario "Codex still working in Whisper mode": a turn in progress or its voice
    /// keeps the session; the timeout then elapses.
    #[test]
    fn a_working_assistant_keeps_the_whisper_session() {
        let rig = whisper_rig(SessionMode::WakeWord, Some(Duration::from_millis(150)));
        rig.gate
            .on_transcript(heard("Hey bro, find me a flight to London."));
        rig.stub.busy.store(true, Ordering::Release);
        for _ in 0..20 {
            rig.gate.process_tick(&Frame::silence(), &[]);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(rig.gate.status().active, "kept while Codex works");
        rig.stub.busy.store(false, Ordering::Release);
        for _ in 0..12 {
            rig.gate.process_tick(&Frame::silence(), &[]);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !rig.gate.status().active,
            "the timeout elapsed after the answer"
        );
    }

    struct Rig {
        gate: Arc<RelayGate>,
        feed: Arc<CountingFeed>,
        stub: Arc<crate::assistant::StubSession>,
        input: FrameReceiver,
        input_gain: Arc<GainStage>,
        output: OutputHandle,
        errors: Arc<Errors>,
    }

    fn rig(mode: SessionMode, timeout: Option<Duration>, ready_after: Duration) -> Rig {
        let (sender, receiver) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        std::mem::forget(_source);
        let feed = Arc::new(CountingFeed(
            AtomicUsize::new(0),
            Mutex::new(Vec::new()),
            Mutex::new(Vec::new()),
        ));
        let errors = Arc::new(Errors::default());
        let input_gain = Arc::new(GainStage::new());
        let gate = RelayGate::new(
            1,
            sender,
            Arc::clone(&input_gain),
            output.clone(),
            settings(mode, timeout),
            false,
            EventBus::new(64),
            Some(feed.clone() as Arc<dyn WakeFeed>),
            Some(errors.clone() as Arc<dyn ErrorReporter>),
        );
        // The stub drains a separate receiver here so the test can inspect the queue itself.
        let assistant = StubAssistant::new(ready_after);
        let (_, idle) = frame_channel(1);
        let session = assistant
            .attach(
                1,
                AudioPorts {
                    input: idle,
                    output: output.clone(),
                    failures: crate::audio::codec::CodecFailures::none(),
                    answers: crate::assistant::AnswerSink::none(),
                    heard: crate::assistant::no_heard(),
                },
            )
            .unwrap();
        gate.set_session(session);
        let stub = assistant.session(1).unwrap();
        std::mem::forget(assistant);
        Rig {
            gate,
            feed,
            stub,
            input: receiver,
            input_gain,
            output,
            errors,
        }
    }

    #[test]
    fn the_input_gain_scales_what_the_assistant_gets_not_what_the_wake_engine_hears() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        assert!(rig
            .gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        let tick = |frame: &Frame| {
            rig.gate.process_tick(frame, &[(7, frame.as_slice())]);
            drain(&rig.input)
                .pop()
                .expect("a frame reached the assistant")
        };
        let level = Frame::from_slice(&[1_000; crate::audio::FRAME_SAMPLES]);

        assert_eq!(tick(&level), level, "1.0 leaves the frame unchanged");
        rig.input_gain.set_gain(2.0);
        assert_eq!(tick(&level).as_slice()[0], 2_000);
        assert_eq!(
            rig.feed.1.lock().unwrap()[0],
            1_000,
            "the wake feed is not scaled"
        );

        rig.input_gain.set_gain(0.0);
        assert_eq!(tick(&level).peak(), 0, "0.0 sends silence");
        assert_eq!(
            rig.feed.1.lock().unwrap()[0],
            1_000,
            "the wake engine still hears"
        );

        rig.input_gain.set_gain(4.0);
        let clips = rig.input_gain.clips();
        let sent = tick(&Frame::from_slice(&[10_000]));
        assert_eq!(sent.as_slice()[0], 32_767);
        assert_eq!(rig.input_gain.clips() - clips, 1);
    }

    fn voice() -> Frame {
        Frame::from_slice(&[5_000; crate::audio::FRAME_SAMPLES])
    }

    fn drain(input: &FrameReceiver) -> Vec<Frame> {
        std::iter::from_fn(|| input.try_recv()).collect()
    }

    #[test]
    fn activation_listens_and_times_out_without_input() {
        let rig = rig(
            SessionMode::WakeWord,
            Some(Duration::from_millis(60)),
            Duration::ZERO,
        );
        let voice = voice();
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert_eq!(rig.feed.0.load(Ordering::Relaxed), 1);
        assert!(rig.gate.status().listening_for_wake_word);
        assert!(
            drain(&rig.input).is_empty(),
            "nothing reaches the assistant while waiting"
        );

        assert!(rig
            .gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::from_millis(0)));
        let status = rig.gate.status();
        assert!(status.active);
        assert_eq!(status.last_wake_phrase.as_deref(), Some("luna"));
        assert_eq!(status.replay_held, 1);

        std::thread::sleep(Duration::from_millis(90));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(
            !rig.gate.status().active,
            "no input for the timeout ends the session"
        );
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![false, true, false]
        );
    }

    #[test]
    fn phrases_only_count_in_their_state() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        assert!(!rig
            .gate
            .phrase_applied(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO));
        assert!(!rig.gate.status().active);
        assert!(rig
            .gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        let activated_at = rig.gate.lock().activated_at;
        assert!(!rig
            .gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert_eq!(
            rig.gate.lock().activated_at,
            activated_at,
            "repeating the phrase changes nothing"
        );
        assert!(rig
            .gate
            .phrase_applied(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO));
        assert!(!rig.gate.status().active);
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![false, true, false]
        );
    }

    #[test]
    fn without_a_timeout_only_the_deactivation_phrase_ends_the_session() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        std::thread::sleep(Duration::from_millis(30));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(rig.gate.status().active);
    }

    #[test]
    fn mute_ends_the_session_sends_nothing_and_unmute_restarts_it_in_always_mode() {
        let rig = rig(SessionMode::Always, None, Duration::ZERO);
        let voice = voice();
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert_eq!(drain(&rig.input).len(), 1);
        rig.output.push(voice.clone());
        assert_eq!(rig.output.queued(), 1);
        rig.gate.set_muted(true);
        assert!(rig.output.is_muted());
        assert_eq!(rig.output.queued(), 0, "queued answer audio is discarded");
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert!(drain(&rig.input).is_empty(), "no frame is sent while muted");
        assert_eq!(
            rig.feed.0.load(Ordering::Relaxed),
            0,
            "always mode never feeds the wake engine"
        );
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![true, false],
            "mute ends the session"
        );
        assert!(!rig.gate.status().active);
        rig.gate.set_muted(false);
        assert!(!rig.output.is_muted());
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![true, false, true],
            "unmute starts a new session"
        );
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert_eq!(drain(&rig.input).len(), 1);
    }

    #[test]
    fn mute_in_wake_word_mode_ends_an_active_session() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        assert!(rig
            .gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert!(rig.gate.status().active);
        rig.gate.set_muted(true);
        assert!(!rig.gate.status().active);
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![false, true, false]
        );
        rig.gate.set_muted(false);
        let status = rig.gate.status();
        assert!(!status.active && status.listening_for_wake_word);
        assert_eq!(
            rig.stub.wanted_log.lock().unwrap().last(),
            Some(&false),
            "unmute waits for the activation phrase"
        );
    }

    #[test]
    fn mute_changes_play_their_tones_even_though_the_output_is_muted() {
        use std::io::Read;
        let tone_after_mute = |earcons: bool| {
            let (sender, _receiver) = frame_channel(INPUT_CAPACITY);
            let (output, mut source) = output_pair();
            let mut gate_settings = settings(SessionMode::Always, None);
            gate_settings.earcons = earcons;
            let gate = RelayGate::new(
                1,
                sender,
                Arc::new(GainStage::new()),
                output,
                gate_settings,
                false,
                EventBus::new(16),
                None,
                None,
            );
            gate.set_muted(true);
            // From silence the tone waits for sending toward Discord to be on.
            let mut bytes = vec![0u8; crate::audio::FRAME_SAMPLES * 4];
            let mut loudest = 0f32;
            for _ in 0..=crate::audio::output::SEND_LEAD {
                source.read_exact(&mut bytes).unwrap();
                loudest = bytes
                    .chunks(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs())
                    .fold(loudest, f32::max);
            }
            loudest
        };
        assert!(tone_after_mute(true) > 0.05, "the mute tone plays");
        assert_eq!(tone_after_mute(false), 0.0, "no tone with earcons off");
    }

    #[test]
    fn mute_in_wake_word_mode_ignores_the_activation_phrase() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        rig.gate.set_muted(true);
        let voice = voice();
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert_eq!(
            rig.feed.0.load(Ordering::Relaxed),
            0,
            "no wake feed while muted"
        );
        assert!(!rig
            .gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert!(!rig.gate.status().active);
        rig.gate.set_muted(false);
        assert!(
            rig.output.is_muted(),
            "waiting for the wake word keeps the assistant silent"
        );
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        assert!(!rig.output.is_muted());
        rig.gate
            .phrase_applied(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO);
        assert!(
            rig.output.is_muted(),
            "deactivation silences the assistant immediately"
        );
    }

    #[test]
    fn replay_starts_at_the_phrase_end_and_catches_up() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        for _ in 0..10 {
            rig.gate.process_tick(&Frame::silence(), &[]);
        }
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::from_millis(100));
        assert_eq!(rig.gate.status().replay_held, 9);
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(
            rig.gate.status().replay_held <= 1,
            "silent backlog is skipped"
        );
    }

    /// A speaker's frame at `level`, so what reaches the call says whose it is.
    fn voice_at(level: i16) -> Vec<i16> {
        (0..crate::audio::FRAME_SAMPLES)
            .map(|i| if i % 2 == 0 { level } else { -level })
            .collect()
    }

    /// The levels of the frames the gate passed to the call so far.
    fn passed_levels(rig: &Rig) -> Vec<i32> {
        std::iter::from_fn(|| rig.input.try_recv())
            .map(|frame| frame.peak())
            .collect()
    }

    /// Change `speaker-turns`: an always-mode gate with one speaker at a time on or off and
    /// speaker names for 7 (Alex) and 8 (Sam).
    fn floor_rig(on: bool) -> Rig {
        let rig = rig(SessionMode::Always, None, Duration::ZERO);
        let mut settings = settings(SessionMode::Always, None);
        settings.one_speaker_at_a_time = on;
        settings.speaker_labels = SpeakerLabels::Start;
        rig.gate.update_settings(settings);
        rig.gate.set_speaker_name(7, "Alex");
        rig.gate.set_speaker_name(8, "Sam");
        rig
    }

    /// Alex talks 10 ticks; Sam starts on the third and talks 10; then silence. Returns the
    /// levels passed to the call, read after every tick (the queue toward the call is short).
    fn overlap(rig: &Rig) -> Vec<i32> {
        let (alex, sam, quiet) = (
            voice_at(1000),
            voice_at(2000),
            vec![0i16; crate::audio::FRAME_SAMPLES],
        );
        let mut levels = Vec::new();
        for tick in 0..60 {
            let a: &[i16] = if tick < 10 { &alex } else { &quiet };
            let s: &[i16] = if (2..12).contains(&tick) {
                &sam
            } else {
                &quiet
            };
            let mut sums = Vec::new();
            let mut mixed = Frame::silence();
            crate::audio::frame::mix_into(&[a, s], &mut sums, &mut mixed);
            rig.gate.process_tick(&mixed, &[(7, a), (8, s)]);
            levels.extend(passed_levels(rig));
        }
        levels
    }

    /// Requirement "One speaker at a time is optional": off, the call gets the mix.
    #[test]
    fn with_the_switch_off_the_call_gets_the_mix() {
        let rig = floor_rig(false);
        let levels = overlap(&rig);
        assert!(levels.contains(&1500), "Alex and Sam mixed: {levels:?}");
        assert_eq!(rig.gate.status().counters.floor_passes, 0);
    }

    /// Requirements "One speaker at a time toward the call" and "The floor passes after a
    /// pause": on, Alex alone, then Sam's buffered speech after Alex's pause, with a note
    /// naming Sam at `start` labels; never a mix.
    #[test]
    fn with_the_switch_on_one_person_at_a_time() {
        let rig = floor_rig(true);
        let levels = overlap(&rig);
        assert!(!levels.contains(&1500), "never mixed: {levels:?}");
        assert_eq!(
            levels.iter().filter(|level| **level == 1000).count(),
            10,
            "{levels:?}"
        );
        assert_eq!(
            levels.iter().filter(|level| **level == 2000).count(),
            10,
            "{levels:?}"
        );
        let first_sam = levels.iter().position(|level| *level == 2000).unwrap();
        assert!(levels[..first_sam].iter().all(|level| *level != 2000));
        assert_eq!(rig.gate.status().counters.floor_passes, 1);
        assert_eq!(
            *rig.stub.speakers.lock().unwrap(),
            vec![vec!["Alex".to_owned()], vec!["Sam".to_owned()]],
            "a note at every change of the floor at `start` labels too"
        );
    }

    /// With the floor and labels `off`, no note at all.
    #[test]
    fn with_the_switch_on_and_labels_off_no_note() {
        let rig = floor_rig(true);
        let mut settings = settings(SessionMode::Always, None);
        settings.one_speaker_at_a_time = true;
        settings.speaker_labels = SpeakerLabels::Off;
        rig.gate.update_settings(settings);
        let levels = overlap(&rig);
        assert_eq!(levels.iter().filter(|level| **level == 2000).count(), 10);
        assert!(rig.stub.speakers.lock().unwrap().is_empty());
    }

    /// Requirement "Speaker labels" with the floor: at `changes` a note at every change of
    /// the floor, Alex's free take and the pass to Sam.
    #[test]
    fn with_the_switch_on_changes_labels_name_every_floor_change() {
        let rig = floor_rig(true);
        let mut settings = settings(SessionMode::Always, None);
        settings.one_speaker_at_a_time = true;
        settings.speaker_labels = SpeakerLabels::Changes;
        rig.gate.update_settings(settings);
        let levels = overlap(&rig);
        assert_eq!(levels.iter().filter(|level| **level == 2000).count(), 10);
        assert_eq!(
            *rig.stub.speakers.lock().unwrap(),
            vec![vec!["Alex".to_owned()], vec!["Sam".to_owned()]]
        );
    }

    /// Requirement "A waiting person's speech is kept": a mute empties the buffers.
    #[test]
    fn a_mute_empties_the_waiting_buffers() {
        let rig = floor_rig(true);
        let (alex, sam) = (voice_at(1000), voice_at(2000));
        for _ in 0..5 {
            rig.gate
                .process_tick(&Frame::silence(), &[(7, &alex), (8, &sam)]);
        }
        assert_eq!(rig.gate.status().floor_waiting, 1);
        rig.gate.set_muted(true);
        assert_eq!(rig.gate.status().floor_waiting, 0);
        let _ = passed_levels(&rig);
        rig.gate.set_muted(false);
        let quiet = vec![0i16; crate::audio::FRAME_SAMPLES];
        for _ in 0..40 {
            rig.gate
                .process_tick(&Frame::silence(), &[(7, &quiet), (8, &quiet)]);
        }
        assert!(
            !passed_levels(&rig).contains(&2000),
            "nothing of Sam's buffer"
        );
    }

    /// Requirement "Pre-roll replay from the end of the phrase", scenario "Someone else talks
    /// during the phrase": on, only the caller's speech is replayed and Sam's waits.
    #[test]
    fn an_activation_with_the_switch_on_replays_only_the_caller() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        let mut settings = settings(SessionMode::WakeWord, None);
        settings.one_speaker_at_a_time = true;
        rig.gate.update_settings(settings);
        let (alex, sam) = (voice_at(1000), voice_at(2000));
        for _ in 0..20 {
            rig.gate
                .process_tick(&Frame::silence(), &[(7, &alex), (8, &sam)]);
        }
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::from_millis(100));
        assert_eq!(rig.gate.status().floor_waiting, 1, "Sam waits");
        rig.gate.process_tick(&Frame::silence(), &[(7, &alex)]);
        let levels = passed_levels(&rig);
        assert!(
            !levels.is_empty() && levels.iter().all(|level| *level == 1000),
            "{levels:?}"
        );
    }

    #[test]
    fn replay_is_held_until_the_assistant_is_ready_then_played_in_order() {
        let rig = rig(SessionMode::WakeWord, None, Duration::from_millis(80));
        let voice = voice();
        for _ in 0..10 {
            rig.gate.process_tick(&voice, &[]);
        }
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::from_millis(60));
        let held = rig.gate.status().replay_held;
        assert_eq!(held, 7, "60 ms + 80 ms margin = 7 frames");
        for _ in 0..10 {
            rig.gate.process_tick(&voice, &[]);
        }
        assert_eq!(rig.gate.status().replay_held, held + 10);
        assert!(drain(&rig.input).is_empty(), "nothing sent while not ready");
        std::thread::sleep(Duration::from_millis(100));
        rig.gate.process_tick(&voice, &[]);
        assert_eq!(
            rig.gate.status().replay_held,
            held + 10,
            "voiced frames are not skipped"
        );
        assert_eq!(drain(&rig.input).len(), 1);
        assert!(rig.gate.status().assistant_ready);
    }

    #[test]
    fn replay_cap_reached_while_not_ready_ends_the_session_with_an_error() {
        let rig = rig(SessionMode::WakeWord, None, Duration::from_secs(3_600));
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        let voice = voice();
        for _ in 0..MAX_REPLAY_FRAMES + 1 {
            rig.gate.process_tick(&voice, &[]);
        }
        let status = rig.gate.status();
        assert!(!status.active);
        assert_eq!(status.counters.replay_timeouts, 1);
        assert_eq!(status.counters.replay_dropped, MAX_REPLAY_FRAMES as u64);
        assert_eq!(rig.errors.0.lock().unwrap().len(), 1);
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![false, true, false]
        );
    }

    #[test]
    fn assistant_audio_keeps_the_session_alive() {
        let rig = rig(
            SessionMode::WakeWord,
            Some(Duration::from_millis(60)),
            Duration::ZERO,
        );
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        std::thread::sleep(Duration::from_millis(40));
        rig.output.push(voice()); // the assistant speaks
        std::thread::sleep(Duration::from_millis(40));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(
            rig.gate.status().active,
            "80 ms since activation but 40 ms since the answer"
        );
        std::thread::sleep(Duration::from_millis(70));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(!rig.gate.status().active);
    }

    #[test]
    fn always_mode_wants_a_session_at_once() {
        let rig = rig(SessionMode::Always, None, Duration::ZERO);
        assert_eq!(*rig.stub.wanted_log.lock().unwrap(), vec![true]);
        assert!(rig.gate.status().active);
        assert!(!rig.output.is_muted());
        rig.gate
            .update_settings(settings(SessionMode::WakeWord, None));
        assert_eq!(*rig.stub.wanted_log.lock().unwrap(), vec![true, false]);
        assert!(rig.output.is_muted());
    }

    #[test]
    fn speaker_names_are_announced_when_people_start_talking() {
        let rig = rig(SessionMode::Always, None, Duration::ZERO);
        rig.gate.set_speaker_name(7, "Alex");
        rig.gate.set_speaker_name(9, "Bob");
        let voice = voice();
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        rig.gate
            .process_tick(&voice, &[(7, voice.as_slice()), (9, voice.as_slice())]);
        assert_eq!(
            *rig.stub.speakers.lock().unwrap(),
            vec![
                vec!["Alex".to_owned()],
                vec!["Alex".to_owned(), "Bob".to_owned()]
            ]
        );
    }

    #[test]
    fn activation_sends_the_caller_and_participants_before_the_call() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        rig.gate.set_speaker_name(7, "Alex");
        rig.gate
            .set_participants(&["Alex".to_owned(), "Bob".to_owned()]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        assert_eq!(
            *rig.stub.contexts.lock().unwrap(),
            vec![(
                Some("Alex".to_owned()),
                vec!["Alex".to_owned(), "Bob".to_owned()]
            )]
        );
        let voice = voice();
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert!(
            rig.stub.speakers.lock().unwrap().is_empty(),
            "the caller is not announced again"
        );
    }

    #[test]
    fn registry_mute_applies_to_all_gates_and_routes_phrases() {
        let registry = RelayRegistry::default();
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        registry.register(rig.gate.clone());
        assert!(registry.toggle_muted());
        assert!(rig.gate.status().muted);
        assert_eq!(
            registry.on_phrase(1, PhraseKind::Activate, "luna", 7, Duration::ZERO),
            PhraseOutcome::NotListening,
            "muted"
        );
        assert!(!registry.toggle_muted());
        assert_eq!(
            registry.on_phrase(1, PhraseKind::Activate, "luna", 7, Duration::ZERO),
            PhraseOutcome::Applied
        );
        assert_eq!(
            registry.on_phrase(2, PhraseKind::Activate, "luna", 7, Duration::ZERO),
            PhraseOutcome::NotListening,
            "unknown guild"
        );
        HeardSink::on_heard(&registry, 1, 0, "ok dismiss", true);
        assert!(
            !rig.gate.status().active,
            "the registry routes the transcript to the guild's gate"
        );
        assert_eq!(registry.statuses().len(), 1);
        assert!(registry.unregister(1).is_some());
        assert!(registry.statuses().is_empty());
    }

    /// Task 2.1: one tick per `process_tick` in wake-word mode, after the speakers' frames
    /// and with their tick; none while muted or in always mode.
    #[test]
    fn the_wake_feed_gets_each_tick_after_its_frames() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        let voice = voice();
        rig.gate
            .process_tick(&voice, &[(7, voice.as_slice()), (9, voice.as_slice())]);
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert_eq!(
            *rig.feed.2.lock().unwrap(),
            vec![(1, Some(7)), (1, Some(9)), (1, None), (2, None)]
        );
        rig.gate.set_muted(true);
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        rig.gate.set_muted(false);
        rig.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert_eq!(
            rig.feed.2.lock().unwrap()[4..],
            [(4, Some(7)), (4, None)],
            "nothing while muted; the numbering goes on"
        );
        let always = rig_with(SessionMode::Always, true);
        always.gate.process_tick(&voice, &[(7, voice.as_slice())]);
        assert!(always.feed.2.lock().unwrap().is_empty(), "always mode");
    }

    /// Task 3.1: the guard against an activation right after a deactivation lives in the
    /// gate and covers both sources.
    #[test]
    fn an_activation_right_after_a_deactivation_is_ignored() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let phrase = |kind, ms| rig.gate.on_phrase_at(kind, "x", 7, Duration::ZERO, at(ms));
        assert_eq!(phrase(PhraseKind::Activate, 0), PhraseOutcome::Applied);
        assert_eq!(
            phrase(PhraseKind::Activate, 100),
            PhraseOutcome::AlreadyInState
        );
        assert_eq!(
            phrase(PhraseKind::Deactivate, 1_000),
            PhraseOutcome::Applied
        );
        assert_eq!(
            phrase(PhraseKind::Deactivate, 1_100),
            PhraseOutcome::AlreadyInState
        );
        assert_eq!(
            phrase(PhraseKind::Activate, 1_800),
            PhraseOutcome::RightAfterDeactivation
        );
        assert!(!rig.gate.status().active);
        assert_eq!(phrase(PhraseKind::Activate, 3_000), PhraseOutcome::Applied);
        assert!(rig.gate.status().active);
        // From the transcript too.
        assert_eq!(
            rig.gate.on_heard_at(0, "ok dismiss.", false, at(3_500)),
            PhraseOutcome::Applied
        );
        assert_eq!(
            phrase(PhraseKind::Activate, 4_300),
            PhraseOutcome::RightAfterDeactivation,
            "scenario «The wake word right after the phrase»"
        );
        rig.gate.set_muted(true);
        assert_eq!(
            phrase(PhraseKind::Activate, 9_000),
            PhraseOutcome::NotListening
        );
    }

    /// A provider that plays a voiced frame on every pull, like the call's jitter buffer
    /// in the middle of an answer.
    #[derive(Default)]
    struct Answer {
        resets: AtomicUsize,
    }

    impl crate::audio::FrameProvider for Answer {
        fn next_frame(&self, out: &mut Frame) -> bool {
            *out = Frame::from_slice(&[5_000; crate::audio::FRAME_SAMPLES]);
            true
        }
        fn reset(&self) {
            self.resets.fetch_add(1, Ordering::Relaxed);
        }
        fn voice_ahead(&self) -> bool {
            true
        }
    }

    /// Task 3.2, scenarios "Deactivation mid-answer" and "Earcons off": the falling earcon
    /// plays whole after a deactivation in the middle of an answer and the Codex session's
    /// flushes after it, and nothing of the answer plays any more.
    #[test]
    fn the_falling_earcon_plays_when_a_session_ends_mid_answer() {
        let left_after_deactivation = |earcons: bool| -> (Vec<f32>, usize) {
            let (sender, _receiver) = frame_channel(INPUT_CAPACITY);
            let (output, mut source) = output_pair();
            let answer = Arc::new(Answer::default());
            output.attach_provider(answer.clone());
            let mut gate_settings = settings(SessionMode::WakeWord, None);
            gate_settings.earcons = earcons;
            let gate = RelayGate::new(
                1,
                sender,
                Arc::new(GainStage::new()),
                output.clone(),
                gate_settings,
                false,
                EventBus::new(16),
                None,
                None,
            );
            assert!(gate.phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO));
            let mut frame = vec![0f32; crate::audio::FRAME_SAMPLES];
            // The answer plays; the activation tone is mixed in, then the voice alone.
            for _ in 0..40 {
                assert!(source.fill_frame(&mut frame));
            }
            assert!(frame.iter().any(|s| (s - 5_000.0 / 32_768.0).abs() < 1e-3));
            assert!(gate.phrase_applied(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO));
            // What the Codex session does when the session is no longer wanted.
            output.flush();
            output.flush();
            let mut left = Vec::new();
            for _ in 0..200 {
                assert!(source.fill_frame(&mut frame));
                left.extend(frame.chunks(2).map(|pair| pair[0]));
            }
            (left, answer.resets.load(Ordering::Relaxed))
        };
        let tone = crate::audio::earcon::samples(Earcon::Deactivate, 0.2);
        let (left, resets) = left_after_deactivation(true);
        assert!(resets >= 1, "the deactivation resets the answer's buffer");
        let start = left
            .windows(tone.len())
            .position(|window| {
                window
                    .iter()
                    .zip(&tone)
                    .all(|(got, want)| (got - want).abs() < 1e-6)
            })
            .expect("every sample of the falling earcon is in the packets");
        let rest = left[..start].iter().chain(&left[start + tone.len()..]);
        assert!(
            rest.into_iter().all(|sample| *sample == 0.0),
            "no frame of the answer plays after the deactivation"
        );
        let (left, _) = left_after_deactivation(false);
        assert!(
            left.iter().all(|sample| *sample == 0.0),
            "with earcons off, no tone and no answer"
        );
    }

    /// Scenario "The wake engine misses the phrase during an answer", "Punctuation and
    /// case", "Both sources hear the phrase".
    #[test]
    fn the_transcript_ends_the_session_through_the_same_path() {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        rig.output.push(voice());
        assert_eq!(
            rig.gate.on_heard(0, "Okay, DISMISS!", false),
            PhraseOutcome::Applied
        );
        let status = rig.gate.status();
        assert!(!status.active);
        assert!(rig.output.is_muted(), "the voice stops at once");
        assert_eq!(rig.output.queued(), 0);
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![false, true, false],
            "the call stops"
        );
        assert_eq!(
            rig.gate
                .on_phrase(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO),
            PhraseOutcome::AlreadyInState,
            "the wake engine's confirmation after it changes nothing"
        );
        assert_eq!(
            deactivation_line("dismiss", DeactivationSource::CallTranscript),
            "deactivation phrase «dismiss» from the call's transcript; the session ended, waiting for the activation phrase"
        );
    }

    /// Scenarios "The phrase arrives in pieces" and "A longer word is not the phrase".
    #[test]
    fn a_phrase_at_the_end_of_the_transcript_waits_for_the_word_to_end() {
        let rig = rig_with_phrases(&["goodbye", "stop"]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        assert_eq!(
            rig.gate.on_heard(0, " Good", false),
            PhraseOutcome::AlreadyInState
        );
        assert!(!rig.output.is_muted());
        rig.gate.on_heard(0, " Goodbye", false);
        assert!(rig.gate.status().active, "held: the word may go on");
        assert!(
            rig.output.is_muted(),
            "the voice is silenced during the hold"
        );
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(rig.gate.status().active, "not yet 300 ms");
        // The gate's tick past the hold confirms it.
        std::thread::sleep(TRANSCRIPT_HOLD + Duration::from_millis(20));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(
            !rig.gate.status().active,
            "ended without waiting for the turn's end"
        );

        let rig = rig_with_phrases(&["goodbye", "stop"]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        rig.gate.on_heard(0, " stop", false);
        assert!(rig.output.is_muted());
        rig.gate.on_heard(0, " stopwatch", false);
        assert!(
            !rig.output.is_muted(),
            "the word went on: the voice comes back"
        );
        std::thread::sleep(TRANSCRIPT_HOLD + Duration::from_millis(20));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(rig.gate.status().active, "«stopwatch» is not «stop»");
        assert!(!rig.output.is_muted());
        rig.gate.on_heard(0, " stopwatch. Stop", false);
        assert!(rig.gate.status().active);
        rig.gate.on_heard(0, " stopwatch. Stop", true);
        assert!(!rig.gate.status().active, "the turn's end confirms it");

        let rig = rig_with_phrases(&["goodbye"]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        rig.gate.on_heard(3, " goodbye", false);
        assert!(rig.gate.status().active);
        rig.gate.on_heard(4, " and", false);
        assert!(
            !rig.gate.status().active,
            "the next turn means the word ended"
        );
    }

    /// Scenario "The phrase cut into two turns": the live calls of 2026-11-17 showed the
    /// call's voice detection ending a turn at the comma of "Отбой, братан".
    #[test]
    fn a_phrase_cut_into_two_turns_still_counts() {
        let rig = rig_with_phrases(&["отбой братан"]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        rig.gate.on_heard(4, " Отбой", false);
        rig.gate.on_heard(4, " Отбой", true);
        assert!(rig.gate.status().active, "«отбой» alone is not the phrase");
        rig.gate.on_heard(5, ", братан", false);
        assert!(rig.output.is_muted(), "held: the voice is silenced");
        rig.gate.on_heard(5, ", братан", true);
        assert!(
            !rig.gate.status().active,
            "the two turns together are the phrase"
        );

        // Only a match that ends in the new turn counts, and only after a recent turn.
        let rig = rig_with_phrases(&["отбой братан"]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        let start = Instant::now();
        rig.gate.on_heard_at(1, "Отбой", true, start);
        rig.gate.on_heard_at(
            2,
            "братан, что нового?",
            true,
            start + Duration::from_secs(3),
        );
        assert!(
            rig.gate.status().active,
            "the previous turn ended 3 s before"
        );
    }

    /// Scenario "The voice replies while the phrase is held".
    #[test]
    fn the_voices_reply_is_not_heard_while_the_phrase_is_held() {
        let rig = rig_with_phrases(&["отбой братан"]);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        rig.gate.on_heard(2, " Отбой, братан", false);
        assert!(rig.gate.status().active);
        rig.output.push(voice()); // the voice's reply starts
        assert_eq!(
            rig.output.queued(),
            0,
            "the reply is discarded, never played"
        );
        std::thread::sleep(TRANSCRIPT_HOLD + Duration::from_millis(20));
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(!rig.gate.status().active);
        assert_eq!(
            *rig.stub.wanted_log.lock().unwrap(),
            vec![false, true, false]
        );
    }

    /// Scenarios "Always mode" and "Phrases of another model", and a phrase-only save
    /// during a session.
    #[test]
    fn the_transcript_counts_only_in_a_wake_word_session_with_the_selected_phrases() {
        let always = rig_with(SessionMode::Always, true);
        assert_eq!(
            always.gate.on_heard(0, "dismiss.", true),
            PhraseOutcome::NotListening
        );
        assert!(always.gate.status().active);

        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        assert_eq!(
            rig.gate.on_heard(0, "dismiss.", true),
            PhraseOutcome::AlreadyInState,
            "no session"
        );
        let mut settings = Settings::default();
        settings.wake.phrases.insert(
            crate::wake::catalog::RUSSIAN_SMALL.to_owned(),
            crate::settings::ModelPhrases {
                activation: None,
                deactivation: Some(vec!["отбой братан".to_owned()]),
            },
        );
        let selected = GateSettings::from_settings(&settings);
        assert_eq!(
            Phrase::texts(&selected.deactivation_phrases),
            ["peace out"],
            "a model with no entry gives its default, not another model's phrases"
        );
        rig.gate.update_settings(selected);
        rig.gate
            .phrase_applied(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        rig.gate.on_heard(0, "отбой братан.", true);
        assert!(rig.gate.status().active, "another model's phrase");
        // A phrase-only save keeps the session; the next transcript uses the new list.
        settings.wake.model = crate::wake::catalog::RUSSIAN_SMALL.to_owned();
        settings.wake.engine = "vosk".to_owned();
        rig.gate
            .update_settings(GateSettings::from_settings(&settings));
        assert!(
            rig.gate.status().active,
            "a phrase change never ends a session"
        );
        rig.gate.on_heard(1, "ну всё, отбой, братан.", true);
        assert!(!rig.gate.status().active);
    }

    fn rig_with(mode: SessionMode, earcons: bool) -> Rig {
        let rig = rig(mode, None, Duration::ZERO);
        let mut gate_settings = settings(mode, None);
        gate_settings.earcons = earcons;
        rig.gate.update_settings(gate_settings);
        rig
    }

    fn rig_with_phrases(phrases: &[&str]) -> Rig {
        let rig = rig(SessionMode::WakeWord, None, Duration::ZERO);
        let mut gate_settings = settings(SessionMode::WakeWord, None);
        gate_settings.deactivation_phrases =
            Phrase::list(&phrases.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>());
        rig.gate.update_settings(gate_settings);
        rig
    }

    /// The receive-path cost per 20 ms tick with four speakers: mixing, the gate, the
    /// wake downsampler per speaker and the Opus encode of the downmix. Runs only with
    /// `VOICEBRO_BENCH=1`; the number decides the release profile (`cargo test --release`
    /// with `CARGO_PROFILE_RELEASE_OPT_LEVEL` set to 3 and to z). The codec of
    /// `VOICEBRO_OPUS_CODEC` (libopus when unset) encodes toward the call, decodes the
    /// playout, and, in a second line, decodes the four speakers as the app does with a
    /// pure-Rust codec (change `opus-codec-experiment`).
    #[test]
    fn receive_path_cost_when_enabled() {
        if std::env::var("VOICEBRO_BENCH").ok().as_deref() != Some("1") {
            return;
        }
        use crate::audio::codec::{
            test_signal::Speechlike, Application, Channels, EncoderParams, OpusCodec, VoiceEncoder,
        };
        use crate::audio::dsp::StereoDownsampler;
        use crate::audio::frame::mix_into;
        let codec = OpusCodec::from_env().expect("VOICEBRO_OPUS_CODEC");
        eprintln!("codec: {}", codec.label());
        let rig = rig(SessionMode::Always, None, Duration::ZERO);
        let speakers: Vec<Frame> = (0..4)
            .map(|index| {
                let samples: Vec<i16> = (0..crate::audio::FRAME_SAMPLES)
                    .map(|i| ((i * (index + 3)) % 2_000) as i16 - 1_000)
                    .collect();
                Frame::from_slice(&samples)
            })
            .collect();
        let mut downsamplers: Vec<StereoDownsampler> =
            (0..4).map(|_| StereoDownsampler::new(3)).collect();
        // The call's encoder settings, and one jitter-buffer decode per tick for the
        // assistant's voice on the playout side.
        let mut encoder = crate::assistant::codex::rtc::new_encoder(codec).unwrap();
        let jitter = crate::assistant::codex::jitter::JitterBuffer::new();
        let mut played = Frame::silence();
        let mut to_mono = StereoDownsampler::new(1);
        let mut packet = vec![0u8; 4000];
        let mut scratch = Vec::new();
        let mut mixed = Frame::silence();
        let ticks = 5_000;
        let started = std::time::Instant::now();
        for tick in 0..ticks {
            let blocks: Vec<&[i16]> = speakers.iter().map(|frame| frame.as_slice()).collect();
            mix_into(&blocks, &mut scratch, &mut mixed);
            let tagged: Vec<(u32, &[i16])> = blocks
                .iter()
                .enumerate()
                .map(|(index, block)| (index as u32, *block))
                .collect();
            rig.gate.process_tick(&mixed, &tagged);
            let _ = drain(&rig.input);
            for (downsampler, block) in downsamplers.iter_mut().zip(&blocks) {
                let _ = downsampler.process(block);
            }
            let mono = to_mono.process(mixed.as_slice());
            let written = encoder.encode_i16(&mono, &mut packet).unwrap();
            jitter.insert(
                tick as u16,
                (tick as u32).wrapping_mul(960),
                bytes::Bytes::copy_from_slice(&packet[..written]),
            );
            let _ = crate::audio::FrameProvider::next_frame(&jitter, &mut played);
        }
        let per_tick = started.elapsed().as_secs_f64() * 1e6 / ticks as f64;
        eprintln!(
            "receive path with 4 speakers plus one playout decode: {per_tick:.1} us per 20 ms tick"
        );

        // The four speakers decoded by the app: Discord clients' packets (libopus, stereo
        // VOIP at 64 kbit/s with FEC), one decoder per speaker, as with a pure-Rust codec.
        let streams: Vec<Vec<Vec<u8>>> = (0..4u64)
            .map(|seed| {
                let mut client = VoiceEncoder::new(
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
                let mut signal = Speechlike::new(seed + 1);
                let mut out = vec![0u8; 4_000];
                (0..250)
                    .map(|_| {
                        let written = client.encode_f32(&signal.stereo_frame(), &mut out).unwrap();
                        out[..written].to_vec()
                    })
                    .collect()
            })
            .collect();
        let mut decoders = crate::discord::speaker_decode::SpeakerDecoders::new(
            codec,
            crate::audio::codec::CodecFailures::none(),
        );
        let mut costs = Vec::with_capacity(ticks);
        for tick in 0..ticks {
            let began = std::time::Instant::now();
            decoders.begin_tick();
            for (ssrc, stream) in streams.iter().enumerate() {
                let packet = &stream[tick % stream.len()];
                decoders
                    .decode(ssrc as u32, Some((tick as u16, packet)))
                    .unwrap();
            }
            costs.push(began.elapsed().as_secs_f64() * 1e6);
            std::hint::black_box(decoders.frame(0));
        }
        costs.sort_by(f64::total_cmp);
        let mean = costs.iter().sum::<f64>() / costs.len() as f64;
        eprintln!(
            "decoding 4 speakers in the app: mean {mean:.1} us, p99 {:.1} us, max {:.1} us per 20 ms tick",
            costs[(costs.len() - 1) * 99 / 100],
            costs[costs.len() - 1]
        );
    }
}
