//! The session gate between Discord and the assistant. One [`RelayGate`] per guild; the
//! [`RelayRegistry`] holds them and the app-wide mute flag. See `docs/ARCHITECTURE.md` 4.3.
//!
//! The gate decides, every 20 ms, whether the channel's mixed frame reaches the assistant:
//! - **mute** ends the session, as in the first version: the call closes, nothing is sent,
//!   the assistant's voice is discarded and wake phrases are ignored; in always mode the
//!   unmute starts a new session. The mute and unmute tones play when earcons are on;
//! - **wake-word mode**: audio flows only after an activation phrase, replayed from the end
//!   of the phrase so the question right after it is not lost; a deactivation phrase ends
//!   the session at once, otherwise the idle timeout does;
//! - **speaker names**: when enabled, the assistant is told who starts talking.

use crate::assistant::AssistantSession;
use crate::audio::{peak, AudioGains, Earcon, Frame, FrameSender, GainStage, OutputHandle};
use crate::events::{AppEvent, EventBus};
use crate::settings::{SessionMode, Settings, SpeakerLabels};
use crate::wake::{PhraseKind, PhraseSink, WakeFeed};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

#[derive(Debug, Clone, PartialEq)]
pub struct GateSettings {
    pub mode: SessionMode,
    /// Ends a session after this long without input; `None` means only the deactivation
    /// phrase ends it.
    pub idle_timeout: Option<Duration>,
    pub earcons: bool,
    pub earcon_volume: f32,
    pub speaker_labels: SpeakerLabels,
}

impl GateSettings {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            mode: settings.session.mode,
            idle_timeout: (settings.session.idle_timeout_seconds > 0)
                .then(|| Duration::from_secs(u64::from(settings.session.idle_timeout_seconds))),
            earcons: settings.session.earcons,
            earcon_volume: settings.session.earcon_volume,
            speaker_labels: settings.assistant.speaker_labels,
        }
    }
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
}

impl GateState {
    /// The assistant's voice is discarded while muted and, in wake-word mode, whenever no
    /// session is active, so a deactivation silences it at once.
    fn output_muted(&self) -> bool {
        self.muted || (self.settings.mode == SessionMode::WakeWord && !self.active)
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
        };
        Arc::new(Self {
            guild,
            input,
            input_gain,
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

    pub fn set_speaker_name(&self, ssrc: u32, name: &str) {
        self.lock().speakers.entry(ssrc).or_default().name = Some(name.to_owned());
    }

    /// Handles one Discord voice tick: the mixed frame plus each speaker's own audio.
    pub fn process_tick(&self, mixed: &Frame, speakers: &[(u32, &[i16])]) {
        let now = Instant::now();
        let assistant_signal = self.output.last_signal();
        let ready = self
            .session
            .lock()
            .expect("gate session lock")
            .as_ref()
            .map(|session| session.ready())
            .unwrap_or(false);
        let mut effects = Effects::default();
        let (frame, feed_wake) = {
            let mut state = self.lock();
            if state.assistant_ready != ready {
                state.assistant_ready = ready;
                effects.status_changed = true;
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

            let feed_wake = state.settings.mode == SessionMode::WakeWord && !state.muted;
            let mut passing = false;
            let frame: Option<Frame> = if state.muted {
                // Muted: no session, nothing is sent.
                None
            } else if state.settings.mode == SessionMode::Always {
                passing = true;
                Some(mixed.clone())
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
                        state.replay.push_back(mixed.clone());
                    }
                    None
                } else {
                    passing = true;
                    state.replay.push_back(mixed.clone());
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
                effects.speakers = state.labels_for(&starters, now);
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
            (frame, feed_wake)
        };

        if feed_wake {
            if let Some(wake) = self.wake.as_ref() {
                for (ssrc, samples) in speakers {
                    wake.feed(self.guild, *ssrc, samples);
                }
            }
        }
        if let Some(mut frame) = frame {
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
    /// that follows reaches the assistant but the phrase itself does not. Returns whether
    /// the phrase changed anything.
    pub fn on_phrase(
        &self,
        kind: PhraseKind,
        phrase: &str,
        ssrc: u32,
        since_phrase_end: Duration,
    ) -> bool {
        let mut effects = Effects::default();
        {
            let mut state = self.lock();
            if state.muted || state.settings.mode != SessionMode::WakeWord {
                return false;
            }
            match kind {
                // Activation only counts while waiting; deactivation only while active.
                PhraseKind::Activate if state.active => return false,
                PhraseKind::Deactivate if !state.active => return false,
                PhraseKind::Activate => {
                    state.active = true;
                    state.activated_at = Some(Instant::now());
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
                    let frames = (lookback.as_millis() as usize)
                        .div_ceil(20)
                        .min(state.preroll.len());
                    let skip = state.preroll.len() - frames;
                    state.replay = state.preroll.iter().skip(skip).cloned().collect();
                    effects.earcon = Some(Earcon::Activate);
                    effects.log = Some(format!(
                        "activation phrase «{phrase}» recognized; the assistant is listening"
                    ));
                }
                PhraseKind::Deactivate => {
                    state.end_session();
                    effects.flush_output = true;
                    effects.earcon = Some(Earcon::Deactivate);
                    effects.log = Some(format!(
                        "deactivation phrase «{phrase}» recognized; waiting for the activation phrase"
                    ));
                }
            }
            if !state.settings.earcons {
                effects.earcon = None;
            }
            self.output.set_muted(state.output_muted());
            let wanted = state.wants_session();
            if wanted != state.session_wanted {
                state.session_wanted = wanted;
                effects.session = Some(wanted);
            }
            effects.status_changed = true;
        }
        self.apply_effects(effects);
        true
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
            if state.settings.mode != settings.mode {
                state.end_session();
                effects.flush_output = true;
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
}

impl PhraseSink for RelayRegistry {
    fn on_phrase(
        &self,
        guild: u64,
        kind: PhraseKind,
        phrase: &str,
        ssrc: u32,
        since_phrase_end: Duration,
    ) -> bool {
        self.get(guild)
            .is_some_and(|gate| gate.on_phrase(kind, phrase, ssrc, since_phrase_end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{Assistant, StubAssistant};
    use crate::audio::{frame_channel, output_pair, AudioPorts, FrameReceiver, INPUT_CAPACITY};
    use std::sync::atomic::AtomicUsize;

    /// Counts the blocks fed to the wake engine and keeps the last one.
    struct CountingFeed(AtomicUsize, Mutex<Vec<i16>>);

    impl WakeFeed for CountingFeed {
        fn feed(&self, _: u64, _: u32, samples: &[i16]) {
            self.0.fetch_add(1, Ordering::Relaxed);
            *self.1.lock().unwrap() = samples.to_vec();
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
        }
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
        let feed = Arc::new(CountingFeed(AtomicUsize::new(0), Mutex::new(Vec::new())));
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO));
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::from_millis(0)));
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
            .on_phrase(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO));
        assert!(!rig.gate.status().active);
        assert!(rig
            .gate
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        let activated_at = rig.gate.lock().activated_at;
        assert!(!rig
            .gate
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert_eq!(
            rig.gate.lock().activated_at,
            activated_at,
            "repeating the phrase changes nothing"
        );
        assert!(rig
            .gate
            .on_phrase(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO));
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO);
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO));
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
            let mut bytes = vec![0u8; crate::audio::FRAME_SAMPLES * 4];
            source.read_exact(&mut bytes).unwrap();
            bytes
                .chunks(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs())
                .fold(0f32, f32::max)
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert!(!rig.gate.status().active);
        rig.gate.set_muted(false);
        assert!(
            rig.output.is_muted(),
            "waiting for the wake word keeps the assistant silent"
        );
        rig.gate
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO);
        assert!(!rig.output.is_muted());
        rig.gate
            .on_phrase(PhraseKind::Deactivate, "dismiss", 7, Duration::ZERO);
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::from_millis(100));
        assert_eq!(rig.gate.status().replay_held, 9);
        rig.gate.process_tick(&Frame::silence(), &[]);
        assert!(
            rig.gate.status().replay_held <= 1,
            "silent backlog is skipped"
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::from_millis(60));
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO);
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO);
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
            .on_phrase(PhraseKind::Activate, "luna", 7, Duration::ZERO);
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
        assert!(!registry.on_phrase(1, PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert!(!registry.toggle_muted());
        assert!(registry.on_phrase(1, PhraseKind::Activate, "luna", 7, Duration::ZERO));
        assert!(
            !registry.on_phrase(2, PhraseKind::Activate, "luna", 7, Duration::ZERO),
            "unknown guild"
        );
        assert_eq!(registry.statuses().len(), 1);
        assert!(registry.unregister(1).is_some());
        assert!(registry.statuses().is_empty());
    }

    /// The receive-path cost per 20 ms tick with four speakers: mixing, the gate, the
    /// wake downsampler per speaker and the Opus encode of the downmix. Runs only with
    /// `VOICEBRO_BENCH=1`; the number decides the release profile (`cargo test --release`
    /// with `CARGO_PROFILE_RELEASE_OPT_LEVEL` set to 3 and to z).
    #[test]
    fn receive_path_cost_when_enabled() {
        if std::env::var("VOICEBRO_BENCH").ok().as_deref() != Some("1") {
            return;
        }
        use crate::audio::dsp::StereoDownsampler;
        use crate::audio::frame::mix_into;
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
        let mut encoder = crate::assistant::codex::rtc::new_encoder().unwrap();
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
            let written = encoder.encode(&mono, &mut packet).unwrap();
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
    }
}
