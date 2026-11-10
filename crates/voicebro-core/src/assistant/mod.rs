//! The assistant behind a trait: the gate talks to an [`AssistantSession`] per guild and
//! never knows which engine answers. Codex is one implementation; [`StubAssistant`] is the
//! one tests and an unknown engine setting fall back to, so the app never panics.
//! See `docs/ARCHITECTURE.md` 4.5.

use crate::audio::AudioPorts;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum AssistantError {
    #[error("the assistant is already attached to guild {0}")]
    AlreadyAttached(u64),
    #[error("{0}")]
    Unavailable(String),
}

/// A model the assistant can run, with the reasoning efforts it accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: Option<String>,
    pub is_default: bool,
}

/// What the engine offers right now; the Settings tab fills its lists from this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub models: Vec<ModelInfo>,
    pub voices: Vec<String>,
    pub signed_in: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AssistantStatus {
    pub engine: String,
    /// Free text for the UI: "signed in as …", "not signed in", "codex not found", …
    pub summary: String,
    pub signed_in: bool,
    /// Guilds with a live call, by id.
    pub calls: Vec<String>,
}

/// An assistant engine. One per app.
pub trait Assistant: Send + Sync {
    fn id(&self) -> &'static str;
    /// Prepares a session for a guild: the gate owns the ports' sending side.
    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError>;
    fn detach(&self, guild: u64);
    fn capabilities(&self) -> Capabilities;
    fn status(&self) -> AssistantStatus;
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
            signed_in: false,
            calls: self
                .sessions
                .lock()
                .expect("stub sessions lock")
                .iter()
                .filter(|(_, session)| session.wanted.load(Ordering::Acquire))
                .map(|(id, _)| id.to_string())
                .collect(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{frame_channel, output_pair, Frame, INPUT_CAPACITY};

    #[test]
    fn the_stub_becomes_ready_after_the_delay_and_records_the_context() {
        let assistant = StubAssistant::new(Duration::from_millis(30));
        let (sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        let session = assistant.attach(7, AudioPorts { input, output }).unwrap();
        assert!(matches!(
            assistant.attach(
                7,
                AudioPorts {
                    input: frame_channel(1).1,
                    output: output_pair().0
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
        assert_eq!(assistant.status().calls, vec!["7".to_owned()]);
        assistant.detach(7);
        assert!(assistant.session(7).is_none());
    }
}
