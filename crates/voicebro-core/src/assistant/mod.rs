//! The assistant behind a trait: the gate talks to an [`AssistantSession`] per guild and
//! never knows which engine answers. Codex is one implementation ([`codex`]);
//! [`StubAssistant`] is the one tests and an unknown engine setting fall back to, so the app
//! never panics. See `docs/ARCHITECTURE.md` 4.5.

pub mod chat_log;
pub mod codex;

use crate::audio::AudioPorts;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What [`Assistant::prepare`] returns: the join waits for it.
pub type PrepareFuture<'a> = Pin<Box<dyn Future<Output = Result<(), AssistantError>> + Send + 'a>>;

/// The final answer of a task the assistant's agent completed, as the agent wrote it
/// (change `post-assistant-answers`). Engine-independent: any engine sends its answers
/// through [`AudioPorts::answers`], and the runtime posts them to Discord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAnswer {
    pub guild: u64,
    /// The task the voice delegated, when the engine knows it.
    pub task: Option<String>,
    pub text: String,
}

/// Answers waiting for the runtime's handler. Sixteen waiting means Discord has been
/// refusing or stalling for minutes; a seventeenth is dropped.
pub const ANSWER_QUEUE: usize = 16;

/// Receives the text of what the people in the channel said, as the assistant's own
/// recognizer heard it (change `voice-recognition-rework`, design D2). Engine-independent:
/// any engine hands it over through [`AudioPorts::heard`], and the gate looks for the
/// deactivation phrase in it. A session hands over nothing of its own voice's speech.
pub trait HeardSink: Send + Sync {
    /// The people's speech of `turn` (numbered per session): the text so far, or with
    /// `done` the turn's final text. Called without any of the session's locks held, since
    /// the receiver may end the session from inside the call.
    fn on_heard(&self, guild: u64, turn: u64, text: &str, done: bool);
}

/// A [`HeardSink`] that drops everything: the stub, the probes and the tests.
pub struct NoHeard;

impl HeardSink for NoHeard {
    fn on_heard(&self, _: u64, _: u64, _: &str, _: bool) {}
}

/// The sink that drops everything, as [`AudioPorts::heard`] takes it.
pub fn no_heard() -> Arc<dyn HeardSink> {
    Arc::new(NoHeard)
}

/// Where a session sends its [`AgentAnswer`]s: a bounded channel to the runtime's handler.
/// Sending never waits; a full channel drops the new answer with a warning and counts it.
/// [`AnswerSink::none`] sends nowhere (tests, probes, a stub assistant).
#[derive(Debug, Clone, Default)]
pub struct AnswerSink {
    sender: Option<tokio::sync::mpsc::Sender<AgentAnswer>>,
    dropped: Arc<AtomicU64>,
}

impl AnswerSink {
    /// A sink and the receiver the runtime's handler drains.
    pub fn channel() -> (Self, tokio::sync::mpsc::Receiver<AgentAnswer>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(ANSWER_QUEUE);
        (
            Self {
                sender: Some(sender),
                dropped: Arc::new(AtomicU64::new(0)),
            },
            receiver,
        )
    }

    pub fn none() -> Self {
        Self::default()
    }

    /// Hands the answer over without waiting. Returns whether it was queued.
    pub fn send(&self, answer: AgentAnswer) -> bool {
        let Some(sender) = self.sender.as_ref() else {
            return false;
        };
        match sender.try_send(answer) {
            Ok(()) => true,
            Err(tokio::sync::mpsc::error::TrySendError::Full(answer)) => {
                let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                log::warn!(
                    "[answers:{}] an answer was dropped: {ANSWER_QUEUE} answers are still waiting to be posted ({dropped} dropped so far)",
                    answer.guild
                );
                false
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(answer)) => {
                log::debug!("[answers:{}] no answer handler runs", answer.guild);
                false
            }
        }
    }

    /// Answers dropped because the channel was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssistantError {
    #[error("the assistant is already attached to guild {0}")]
    AlreadyAttached(u64),
    #[error("{0}")]
    Unavailable(String),
}

/// A model the assistant can run, with the reasoning efforts it accepts. Also read back from
/// the saved lists (`codex-lists.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: Option<String>,
    pub is_default: bool,
}

/// What the engine offers right now, read in this run or saved from the last read; the
/// Config tab's Codex card fills its lists from this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub models: Vec<ModelInfo>,
    pub voices: Vec<String>,
    pub default_voice: Option<String>,
    pub signed_in: bool,
    /// When the models and voices were read from the engine, in Unix milliseconds; `None`
    /// when they were never read.
    pub lists_read_at_ms: Option<u64>,
}

/// One guild's call, as the status shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CallStatus {
    pub guild_id: String,
    /// idle, starting, live or stopping.
    pub state: String,
    pub connected: bool,
    /// The Opus codec of the running call, name and version; none without a call.
    pub opus_codec: Option<String>,
    /// The running call's audio counters, or the last ended call's.
    pub audio: Option<codex::rtc::CallAudioCounters>,
    pub thread_id: Option<String>,
    pub last_user_text: Option<String>,
    pub last_assistant_text: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AssistantStatus {
    pub engine: String,
    /// Free text for the UI: "signed in as …", "not signed in", "codex not found", …
    pub summary: String,
    pub signed_in: bool,
    pub account_type: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub executable: Option<String>,
    pub version: Option<String>,
    /// The engine's process is running.
    pub running: bool,
    pub error: Option<String>,
    /// The model and reasoning effort the current thread was opened with; empty means
    /// the account default.
    pub model_in_use: Option<String>,
    pub reasoning_effort_in_use: Option<String>,
    pub calls: Vec<CallStatus>,
}

/// An assistant engine. One per app.
pub trait Assistant: Send + Sync {
    fn id(&self) -> &'static str;
    /// Runs before the bot joins a voice channel of `guild`: starts the engine's process if
    /// it has one and checks it can serve the guild. An error refuses the join with its
    /// message.
    fn prepare(&self, _guild: u64) -> PrepareFuture<'_> {
        Box::pin(async { Ok(()) })
    }
    /// Creates a session for a guild after a successful [`Assistant::prepare`]: the gate
    /// owns the ports' sending side.
    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError>;
    fn detach(&self, guild: u64);
    fn capabilities(&self) -> Capabilities;
    fn status(&self) -> AssistantStatus;
    /// The settings changed; the engine applies what concerns it at the next call.
    fn configure(&self, _settings: &Settings) {}
    /// Stops every call and the engine's process. Called at app exit.
    fn stop(&self) {}
    /// The local voice the Whisper mode speaks with (change `whisper-mode`).
    fn use_tts(&self, _tts: crate::tts::manager::TtsManager) {}
}

/// What the gate talks to. One per guild.
pub trait AssistantSession: Send + Sync {
    /// The gate wants the assistant to hear the channel (or no longer wants it).
    fn session_wanted(&self, wanted: bool);
    /// Whether audio sent now will be heard. While a call is still being set up after an
    /// activation phrase, the gate holds the replay instead of losing it.
    fn ready(&self) -> bool;
    /// Who is about to be heard: the person who said the activation phrase (if any) and
    /// everyone in the channel. Sent before a session starts.
    fn call_context(&self, caller: Option<String>, participants: &[String]);
    /// The names of the people now talking, whenever that set changes.
    fn speakers_changed(&self, names: &[String]);
    /// A question in words (the Whisper mode, change `whisper-mode`). A realtime session
    /// hears audio instead and ignores it.
    fn user_text(&self, _question: UserText) {}
    /// The assistant is still working on an answer (a turn runs or its voice plays), so the
    /// idle timeout does not end the session.
    fn working(&self) -> bool {
        false
    }
}

/// A question Whisper heard, with what the latency line needs (change `whisper-mode`,
/// design D15).
#[derive(Clone)]
pub struct UserText {
    /// The speaker's Discord name, when known.
    pub speaker: Option<String>,
    pub text: String,
    /// The language Whisper used or detected (ISO 639-1).
    pub language: Option<String>,
    pub speech_end: Instant,
    pub text_at: Instant,
    pub audio_seconds: f64,
    pub model: String,
    pub device: String,
    /// The guild's turn latency, where the answer's timings go.
    pub latency: crate::latency::SharedLatency,
}

impl std::fmt::Debug for UserText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserText")
            .field("speaker", &self.speaker)
            .field("text", &self.text)
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

/// An assistant that answers nothing: it becomes ready a configurable time after a
/// session is wanted, drains its input and records every call the gate makes.
pub struct StubAssistant {
    ready_after: Duration,
    sessions: Mutex<Vec<(u64, Arc<StubSession>)>>,
}

impl StubAssistant {
    pub fn new(ready_after: Duration) -> Self {
        Self {
            ready_after,
            sessions: Mutex::new(Vec::new()),
        }
    }

    /// The session attached for `guild`, for tests and the status.
    pub fn session(&self, guild: u64) -> Option<Arc<StubSession>> {
        self.sessions
            .lock()
            .expect("stub sessions lock")
            .iter()
            .find(|(id, _)| *id == guild)
            .map(|(_, session)| Arc::clone(session))
    }
}

impl Default for StubAssistant {
    fn default() -> Self {
        Self::new(Duration::from_millis(500))
    }
}

impl Assistant for StubAssistant {
    fn id(&self) -> &'static str {
        "stub"
    }

    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError> {
        let mut sessions = self.sessions.lock().expect("stub sessions lock");
        if sessions.iter().any(|(id, _)| *id == guild) {
            return Err(AssistantError::AlreadyAttached(guild));
        }
        let session = Arc::new(StubSession::new(guild, ports, self.ready_after));
        sessions.push((guild, Arc::clone(&session)));
        Ok(session)
    }

    fn detach(&self, guild: u64) {
        let mut sessions = self.sessions.lock().expect("stub sessions lock");
        if let Some(index) = sessions.iter().position(|(id, _)| *id == guild) {
            let (_, session) = sessions.remove(index);
            session.detached.store(true, Ordering::Release);
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn status(&self) -> AssistantStatus {
        AssistantStatus {
            engine: self.id().to_owned(),
            summary: "stub assistant: hears the channel and answers nothing".to_owned(),
            calls: self
                .sessions
                .lock()
                .expect("stub sessions lock")
                .iter()
                .filter(|(_, session)| session.wanted.load(Ordering::Acquire))
                .map(|(id, _)| CallStatus {
                    guild_id: id.to_string(),
                    state: "live".to_owned(),
                    connected: true,
                    ..CallStatus::default()
                })
                .collect(),
            ..AssistantStatus::default()
        }
    }
}

pub struct StubSession {
    guild: u64,
    ready_after: Duration,
    wanted: AtomicBool,
    wanted_since: Mutex<Option<Instant>>,
    detached: Arc<AtomicBool>,
    pub wanted_log: Mutex<Vec<bool>>,
    pub contexts: Mutex<Vec<(Option<String>, Vec<String>)>>,
    pub speakers: Mutex<Vec<Vec<String>>>,
    /// The questions in words, as (speaker, text).
    pub texts: Mutex<Vec<(Option<String>, String)>>,
    /// What [`AssistantSession::working`] answers.
    pub busy: AtomicBool,
    frames_heard: Arc<AtomicU64>,
}

impl StubSession {
    fn new(guild: u64, ports: AudioPorts, ready_after: Duration) -> Self {
        let frames_heard = Arc::new(AtomicU64::new(0));
        let detached = Arc::new(AtomicBool::new(false));
        let counter = Arc::clone(&frames_heard);
        let stop = Arc::clone(&detached);
        std::thread::Builder::new()
            .name(format!("voicebro-stub-{guild}"))
            .spawn(move || {
                while !stop.load(Ordering::Acquire) && !ports.output.is_closed() {
                    while ports.input.try_recv().is_some() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
            .expect("spawn the stub assistant thread");
        Self {
            guild,
            ready_after,
            wanted: AtomicBool::new(false),
            wanted_since: Mutex::new(None),
            detached,
            wanted_log: Mutex::new(Vec::new()),
            contexts: Mutex::new(Vec::new()),
            speakers: Mutex::new(Vec::new()),
            texts: Mutex::new(Vec::new()),
            busy: AtomicBool::new(false),
            frames_heard,
        }
    }

    pub fn frames_heard(&self) -> u64 {
        self.frames_heard.load(Ordering::Relaxed)
    }
}

impl AssistantSession for StubSession {
    fn session_wanted(&self, wanted: bool) {
        self.wanted.store(wanted, Ordering::Release);
        *self.wanted_since.lock().expect("stub wanted lock") = wanted.then(Instant::now);
        self.wanted_log.lock().expect("stub log lock").push(wanted);
        log::info!(
            "[stub:{}] session {}",
            self.guild,
            if wanted { "wanted" } else { "not wanted" }
        );
    }

    fn ready(&self) -> bool {
        self.wanted_since
            .lock()
            .expect("stub wanted lock")
            .is_some_and(|since| since.elapsed() >= self.ready_after)
    }

    fn call_context(&self, caller: Option<String>, participants: &[String]) {
        log::info!(
            "[stub:{}] call context: caller {:?}, participants {:?}",
            self.guild,
            caller,
            participants
        );
        self.contexts
            .lock()
            .expect("stub contexts lock")
            .push((caller, participants.to_vec()));
    }

    fn speakers_changed(&self, names: &[String]) {
        self.speakers
            .lock()
            .expect("stub speakers lock")
            .push(names.to_vec());
    }

    fn user_text(&self, question: UserText) {
        self.texts
            .lock()
            .expect("stub texts lock")
            .push((question.speaker, question.text));
    }

    fn working(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::CodecFailures;
    use crate::audio::{frame_channel, output_pair, Frame, INPUT_CAPACITY};

    #[test]
    fn the_stub_becomes_ready_after_the_delay_and_records_the_context() {
        let assistant = StubAssistant::new(Duration::from_millis(30));
        let (sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        let session = assistant
            .attach(
                7,
                AudioPorts {
                    input,
                    output,
                    failures: CodecFailures::none(),
                    answers: AnswerSink::none(),
                    heard: no_heard(),
                },
            )
            .unwrap();
        assert!(matches!(
            assistant.attach(
                7,
                AudioPorts {
                    input: frame_channel(1).1,
                    output: output_pair().0,
                    failures: CodecFailures::none(),
                    answers: AnswerSink::none(),
                    heard: no_heard(),
                }
            ),
            Err(AssistantError::AlreadyAttached(7))
        ));
        assert!(!session.ready(), "not ready before a session is wanted");
        session.call_context(
            Some("Alex".to_owned()),
            &["Alex".to_owned(), "Bob".to_owned()],
        );
        session.session_wanted(true);
        assert!(!session.ready());
        std::thread::sleep(Duration::from_millis(60));
        assert!(session.ready());
        sender.send(Frame::silence());
        for _ in 0..10 {
            if assistant.session(7).unwrap().frames_heard() > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let stub = assistant.session(7).unwrap();
        assert_eq!(stub.frames_heard(), 1);
        assert_eq!(
            *stub.contexts.lock().unwrap(),
            vec![(
                Some("Alex".to_owned()),
                vec!["Alex".to_owned(), "Bob".to_owned()]
            )]
        );
        assert_eq!(assistant.status().calls[0].guild_id, "7");
        assistant.detach(7);
        assert!(assistant.session(7).is_none());
    }
}
