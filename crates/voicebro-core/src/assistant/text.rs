//! The Whisper mode's text session, shared by every engine (change `whisper-mode`, design D6;
//! made engine-independent by change `claude-and-opencode-engines`, design D2): questions in
//! words become text turns, and the answer is spoken by the local voice sentence by
//! sentence as it streams in. No realtime call is opened.
//!
//! The session knows nothing of an engine's protocol. It talks to a [`TurnBackend`]: start a
//! turn, add a question to the running one, interrupt one; the backend reports what happens
//! as [`TurnEvent`]s through the [`TurnSink`] it was bound to.
//!
//! - A question is prefixed with the speaker's name unless speaker labels are off. The first
//!   turn of each session carries the people in the channel, and the first turn of a new
//!   conversation the guild's last exchanges ([`TurnContext`]).
//! - A new question while an answer runs or plays interrupts it and silences the voice at
//!   once, unless one speaker at a time is on: then it is added to the running turn, or
//!   waits for the next one when the engine cannot add it.
//! - Every agent message of the turn is spoken; the last one of a completed turn is posted
//!   (`AudioPorts.answers`) with the question as the task.
//! - Ending the session interrupts a running turn and silences the voice.

use crate::assistant::chat_log::ChatLog;
use crate::assistant::codex::conversation::{CallContext, Conversation};
use crate::assistant::{AgentAnswer, AssistantSession, CallStatus, UserText};
use crate::audio::AudioPorts;
use crate::events::EventBus;
use crate::latency::{self, LatencyEvent, SharedLatency, WhisperTiming};
use crate::settings::{AssistantEngine, SpeakerLabels};
use crate::status::ConnectionErrors;
use crate::tts::manager::TtsManager;
use crate::tts::player::SpeechPlayer;
use crate::tts::text::SentenceSplitter;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;
use tokio::runtime::Handle;

/// The settings a text session reads at each question.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextOptions {
    pub speaker_labels: SpeakerLabels,
    /// `assistant.one_speaker_at_a_time` (change `speaker-turns`, D6').
    pub one_speaker_at_a_time: bool,
    /// The text answers mode (change `text-answers-mode`, D2): nothing is spoken, and the
    /// latency line is written when the turn completes.
    pub text_answers: bool,
}

/// What goes with a turn besides the question: the people in the channel with the first
/// turn of a session, and the guild's last exchanges with the first turn of a new
/// conversation. Each backend sends them its own way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnContext {
    /// "Discord voice channel. In the channel: … Addressed by: …."
    pub channel: Option<String>,
    /// [`Conversation::history_text`].
    pub history: Option<String>,
}

impl TurnContext {
    pub fn is_empty(&self) -> bool {
        self.channel.is_none() && self.history.is_none()
    }

    /// The context as text before the question, for engines with no field of their own for
    /// it (Claude Code, OpenCode; design D3).
    pub fn prefixed(&self, question: &str) -> String {
        let mut text = String::new();
        if let Some(history) = &self.history {
            text.push_str(history);
            text.push_str("\n\n");
        }
        if let Some(channel) = &self.channel {
            text.push('[');
            text.push_str(channel);
            text.push_str("]\n");
        }
        text.push_str(question);
        text
    }
}

/// What a backend reports about its turns. `turn` is the id [`TurnBackend::start`] returned
/// (or will return): events of a turn the session no longer runs are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    /// A piece of an agent message, as it streams.
    Delta { turn: String, text: String },
    /// An agent message ended; `text` is the whole message.
    Message { turn: String, text: String },
    /// A line for the chat log about the turn's work: an agent message, a tool, reasoning.
    Chat {
        turn: String,
        kind: &'static str,
        text: String,
    },
    /// The turn ended. `ok` when it completed; otherwise `detail` says how ("interrupted",
    /// "failed: …").
    Completed {
        turn: String,
        ok: bool,
        detail: String,
    },
    /// The engine's process or server is gone; `message` says what to do.
    Exited { message: String },
}

/// Where a backend sends its [`TurnEvent`]s.
pub trait TurnSink: Send + Sync {
    fn on_turn_event(&self, event: TurnEvent);
}

/// One engine's text turns for one guild (design D2).
#[async_trait::async_trait]
pub trait TurnBackend: Send + Sync {
    /// The engine, for its name in the chat, the log and the connection errors.
    fn engine(&self) -> AssistantEngine;
    /// Called once by the session it belongs to.
    fn bind(&self, sink: Weak<dyn TurnSink>);
    /// The engine's conversation for the guild (a thread, a process, a session), when it
    /// has one; the history is sent once per conversation.
    fn conversation_id(&self) -> Option<String>;
    /// The engine can take a question now.
    fn ready(&self) -> bool;
    /// Starts a turn with `text` and `context`; returns the turn's id when the engine gave
    /// one, or the reason it refused.
    async fn start(&self, text: String, context: TurnContext) -> Result<Option<String>, String>;
    /// Adds `text` to the running turn `turn` (one speaker at a time); the reason it was
    /// refused otherwise, and the question then waits for the next turn.
    async fn add(&self, turn: &str, text: String) -> Result<(), String>;
    /// Stops turn `turn` on the engine.
    async fn interrupt(&self, turn: &str);
    /// The session ended: forget it.
    fn detach(&self);
}

/// What a text session is built from.
pub struct TextParts {
    pub guild: u64,
    pub handle: Handle,
    pub backend: Arc<dyn TurnBackend>,
    pub options: Arc<dyn Fn() -> TextOptions + Send + Sync>,
    pub conversation: Arc<Mutex<Conversation>>,
    pub ports: AudioPorts,
    pub chat: Arc<ChatLog>,
    pub events: EventBus,
    pub errors: Arc<ConnectionErrors>,
    pub tts: TtsManager,
}

/// The timings of one answer until its voice plays.
struct Timing {
    speech_end: Instant,
    text_at: Instant,
    first_words_at: Option<Instant>,
    audio_seconds: f64,
    model: String,
    device: String,
    latency: SharedLatency,
}

/// The turn the session is answering.
struct Running {
    number: u64,
    /// Known once `start` answered or its first event came.
    turn_id: Option<String>,
    question: String,
    language: Option<String>,
    epoch: u64,
    splitter: SentenceSplitter,
    /// No piece of this turn was queued yet: the first one marks the turn on the player.
    first_piece: bool,
    last_agent_text: Option<String>,
    /// The questions added to this turn (change `speaker-turns`, D6'), after `question`;
    /// the posted answer's task names them all.
    added: Vec<String>,
}

/// A question that could not join the running turn and waits for the next one (D6').
struct Waiting {
    /// As sent to the engine (with the speaker's name).
    text: String,
    question: UserText,
}

#[derive(Default)]
struct TextState {
    wanted: bool,
    call: CallContext,
    /// The call context went with this session's first turn.
    context_sent: bool,
    running: Option<Running>,
    turns: u64,
    /// Turns interrupted or replaced: their late events are ignored.
    stale: HashSet<String>,
    timings: HashMap<u64, Timing>,
    last_user_text: Option<String>,
    last_assistant_text: Option<String>,
    last_error: Option<String>,
    /// Questions waiting for the next turn, in order (one speaker at a time only).
    waiting: Vec<Waiting>,
}

pub struct TextSession {
    me: Weak<Self>,
    guild: u64,
    handle: Handle,
    backend: Arc<dyn TurnBackend>,
    options: Arc<dyn Fn() -> TextOptions + Send + Sync>,
    conversation: Arc<Mutex<Conversation>>,
    ports: AudioPorts,
    chat: Arc<ChatLog>,
    events: EventBus,
    errors: Arc<ConnectionErrors>,
    tts: TtsManager,
    player: Arc<SpeechPlayer>,
    state: Mutex<TextState>,
    /// One turn change at a time: the interrupt goes out before the next start.
    turn_lock: tokio::sync::Mutex<()>,
}

impl TextSession {
    pub fn new(parts: TextParts) -> Arc<Self> {
        let player = SpeechPlayer::new();
        parts.ports.output.attach_provider(Arc::clone(&player) as _);
        let session = Arc::new_cyclic(|me: &Weak<Self>| {
            let hook = me.clone();
            player.on_first_played(Arc::new(move |turn, at| {
                if let Some(session) = hook.upgrade() {
                    session.voice_played(turn, at);
                }
            }));
            Self {
                me: me.clone(),
                guild: parts.guild,
                handle: parts.handle,
                backend: parts.backend,
                options: parts.options,
                conversation: parts.conversation,
                ports: parts.ports,
                chat: parts.chat,
                events: parts.events,
                errors: parts.errors,
                tts: parts.tts,
                player,
                state: Mutex::new(TextState::default()),
                turn_lock: tokio::sync::Mutex::new(()),
            }
        });
        let sink: Weak<dyn TurnSink> = Arc::downgrade(&session) as Weak<dyn TurnSink>;
        session.backend.bind(sink);
        session
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TextState> {
        self.state.lock().expect("text session lock")
    }

    fn name(&self) -> &'static str {
        self.backend.engine().display_name()
    }

    fn error_source(&self) -> &'static str {
        self.backend.engine().name()
    }

    fn chat(&self, kind: &'static str, who: Option<&str>, text: &str) {
        self.chat.push(self.guild, kind, who, text);
    }

    pub fn status(&self) -> CallStatus {
        let state = self.lock();
        let conversation = self.backend.conversation_id();
        CallStatus {
            guild_id: self.guild.to_string(),
            state: if state.running.is_some() {
                "answering"
            } else if state.wanted {
                "listening"
            } else {
                "idle"
            }
            .to_owned(),
            connected: conversation.is_some(),
            thread_id: conversation,
            last_user_text: state.last_user_text.clone(),
            last_assistant_text: state.last_assistant_text.clone(),
            last_error: state.last_error.clone(),
            ..CallStatus::default()
        }
    }

    /// The first frame of `turn`'s voice played: its latency line.
    fn voice_played(&self, turn: u64, at: Instant) {
        let Some(timing) = self.lock().timings.remove(&turn) else {
            return;
        };
        let first_words_at = timing.first_words_at.unwrap_or(at);
        let report = WhisperTiming::new(
            timing.speech_end,
            timing.text_at,
            first_words_at,
            at,
            timing.audio_seconds,
            &timing.model,
            &timing.device,
        )
        .with_engine(self.name())
        .report(turn, &chrono::Local::now().format("%H:%M:%S").to_string());
        crate::detail!(
            "latency",
            "{}",
            latency::log_line(self.guild, &LatencyEvent::Report(report.clone()))
        );
        timing
            .latency
            .lock()
            .expect("latency lock")
            .push_report(report);
    }

    /// A turn of the text answers mode completed (change `text-answers-mode`, D4): its latency
    /// line, without the voice's times.
    fn answered_without_voice(&self, turn: u64) {
        let Some(timing) = self.lock().timings.remove(&turn) else {
            return;
        };
        let report = WhisperTiming::without_voice(
            timing.speech_end,
            timing.text_at,
            timing.first_words_at.unwrap_or_else(Instant::now),
            timing.audio_seconds,
            &timing.model,
            &timing.device,
        )
        .with_engine(self.name())
        .report(turn, &chrono::Local::now().format("%H:%M:%S").to_string());
        crate::detail!(
            "latency",
            "{}",
            latency::log_line(self.guild, &LatencyEvent::Report(report.clone()))
        );
        timing
            .latency
            .lock()
            .expect("latency lock")
            .push_report(report);
    }

    /// The context of the next turn: the people in the channel with the first turn of a
    /// session, the guild's last exchanges with the first turn of a new conversation.
    fn context(&self, state: &mut TextState) -> TurnContext {
        let mut context = TurnContext::default();
        if !state.context_sent {
            state.context_sent = true;
            if (self.options)().speaker_labels != SpeakerLabels::Off {
                context.channel = state
                    .call
                    .summary()
                    .map(|summary| format!("Discord voice channel. {summary}"));
            }
        }
        let id = self.backend.conversation_id().unwrap_or_default();
        let mut conversation = self.conversation.lock().expect("conversation lock");
        if conversation.history_sent_to.as_deref() != Some(id.as_str()) {
            conversation.history_sent_to = Some(id);
            context.history = conversation.history_text();
        }
        context
    }

    /// Interrupts the turn in progress, if any, and silences the voice. Returns the turn id
    /// to interrupt on the engine.
    fn stop_running(&self, state: &mut TextState) -> Option<String> {
        self.player.flush();
        state.waiting.clear();
        let running = state.running.take()?;
        state.timings.remove(&running.number);
        let id = running.turn_id?;
        state.stale.insert(id.clone());
        Some(id)
    }

    fn interrupt(&self, turn_id: String) {
        let backend = Arc::clone(&self.backend);
        self.handle.spawn(async move {
            backend.interrupt(&turn_id).await;
        });
    }

    /// Queues the pieces of `running` for the voice; nothing in the text answers mode.
    fn speak(&self, running: &mut Running, pieces: Vec<String>) {
        if (self.options)().text_answers {
            return;
        }
        for piece in pieces {
            let turn = std::mem::take(&mut running.first_piece).then_some(running.number);
            self.tts.speak(
                &self.player,
                running.epoch,
                piece,
                running.language.clone(),
                turn,
            );
        }
    }

    /// The running turn, when `id` is its id (adopting the id if it was not known yet).
    fn running_for<'a>(state: &'a mut TextState, id: &str) -> Option<&'a mut Running> {
        if state.stale.contains(id) {
            return None;
        }
        let running = state.running.as_mut()?;
        match running.turn_id.as_deref() {
            Some(known) if known == id => Some(running),
            Some(_) => None,
            None => {
                running.turn_id = Some(id.to_owned());
                Some(running)
            }
        }
    }

    fn on_event(&self, event: TurnEvent) {
        match event {
            TurnEvent::Delta { turn, text } => {
                let mut state = self.lock();
                let state = &mut *state;
                let Some(number) = Self::running_for(state, &turn).map(|running| running.number)
                else {
                    return;
                };
                if let Some(timing) = state.timings.get_mut(&number) {
                    timing.first_words_at.get_or_insert_with(Instant::now);
                }
                let Some(running) = state.running.as_mut() else {
                    return;
                };
                let pieces = running.splitter.push(&text);
                self.speak(running, pieces);
            }
            TurnEvent::Message { turn, text } => {
                let mut state = self.lock();
                let state = &mut *state;
                let Some(running) = Self::running_for(state, &turn) else {
                    return;
                };
                let pieces = running.splitter.finish();
                self.speak(running, pieces);
                let text = text.trim().to_owned();
                running.last_agent_text = Some(text.clone());
                state.last_assistant_text = Some(text);
            }
            TurnEvent::Chat { turn, kind, text } => {
                {
                    let mut state = self.lock();
                    if Self::running_for(&mut state, &turn).is_none() {
                        return;
                    }
                }
                // The question itself is in the chat already.
                if kind != "task" {
                    self.chat(kind, None, &text);
                }
            }
            TurnEvent::Completed { turn, ok, detail } => {
                let finished = {
                    let mut state = self.lock();
                    let state = &mut *state;
                    if Self::running_for(state, &turn).is_none() {
                        return;
                    }
                    state.running.take()
                };
                let Some(running) = finished else {
                    return;
                };
                // Questions that waited for this turn start the next one (D6').
                self.start_waiting();
                if (self.options)().text_answers {
                    // No voice will mark the answer: the line is written now, or dropped
                    // with a failed turn.
                    if ok {
                        self.answered_without_voice(running.number);
                    } else {
                        self.lock().timings.remove(&running.number);
                    }
                }
                if ok {
                    if let Some(text) = running.last_agent_text.filter(|text| !text.is_empty()) {
                        {
                            let mut conversation =
                                self.conversation.lock().expect("conversation lock");
                            conversation.push("assistant", text.clone());
                        }
                        // Every question of the turn, the added ones too.
                        let task = std::iter::once(running.question)
                            .chain(running.added)
                            .collect::<Vec<_>>()
                            .join(" / ");
                        self.ports.answers.send(AgentAnswer {
                            guild: self.guild,
                            task: Some(task),
                            text,
                            engine: self.name(),
                        });
                    }
                } else {
                    self.chat(
                        "error",
                        None,
                        &format!("{}: the answer {detail}.", self.name()),
                    );
                }
            }
            TurnEvent::Exited { message } => {
                {
                    let mut state = self.lock();
                    self.stop_running(&mut state);
                    state.last_error = Some(message.clone());
                }
                self.errors.report(self.error_source(), Some(message));
            }
        }
    }

    /// Starts the turn of question `number` after interrupting `previous`.
    async fn start_turn(&self, number: u64, previous: Option<String>, text: String) {
        let _one = self.turn_lock.lock().await;
        if let Some(previous) = previous {
            self.backend.interrupt(&previous).await;
        }
        let context = {
            let mut state = self.lock();
            if state.running.as_ref().map(|running| running.number) != Some(number) {
                return; // Replaced or ended before it started.
            }
            self.context(&mut state)
        };
        match self.backend.start(text, context).await {
            Ok(id) => {
                let stale = {
                    let mut state = self.lock();
                    match state
                        .running
                        .as_mut()
                        .filter(|running| running.number == number)
                    {
                        Some(running) => {
                            if running.turn_id.is_none() {
                                running.turn_id = id.clone();
                            }
                            None
                        }
                        // Ended while it was starting: interrupt it on the engine too.
                        None => id.inspect(|id| {
                            state.stale.insert(id.clone());
                        }),
                    }
                };
                if let Some(stale) = stale {
                    self.interrupt(stale);
                }
                self.errors.report(self.error_source(), None);
            }
            Err(error) => {
                let message = format!("{} could not take the question: {error}", self.name());
                {
                    let mut state = self.lock();
                    if state.running.as_ref().map(|running| running.number) == Some(number) {
                        state.running = None;
                    }
                    state.last_error = Some(message.clone());
                }
                self.chat("error", None, &message);
                self.events
                    .log(format!("{} {}: {message}", self.name(), self.guild));
                self.errors.report(self.error_source(), Some(message));
            }
        }
    }

    /// Makes `question` the running turn and starts it on the engine, after interrupting
    /// `previous`. Returns the turn's number.
    fn begin_turn(&self, text: String, question: UserText, previous: Option<String>) -> u64 {
        let number = {
            let mut state = self.lock();
            state.turns += 1;
            let number = state.turns;
            state.running = Some(Running {
                number,
                turn_id: None,
                question: question.text.clone(),
                language: question.language.clone(),
                epoch: self.player.epoch(),
                splitter: SentenceSplitter::new(),
                first_piece: true,
                last_agent_text: None,
                added: Vec::new(),
            });
            state.timings.insert(
                number,
                Timing {
                    speech_end: question.speech_end,
                    text_at: question.text_at,
                    first_words_at: None,
                    audio_seconds: question.audio_seconds,
                    model: question.model.clone(),
                    device: question.device.clone(),
                    latency: Arc::clone(&question.latency),
                },
            );
            state.last_user_text = Some(question.text.clone());
            number
        };
        if let Some(session) = self.me.upgrade() {
            self.handle.spawn(async move {
                session.start_turn(number, previous, text).await;
            });
        }
        number
    }

    /// Adds `text` to turn `number` once its start has returned (D6'). When the turn has
    /// ended or the engine refuses, the question waits for the next turn, or starts it when
    /// no turn runs any more.
    async fn add_to_turn(&self, number: u64, text: String, question: UserText) {
        let refusal = {
            // The start of the turn holds this lock until its id is known.
            let _one = self.turn_lock.lock().await;
            let id = self
                .lock()
                .running
                .as_ref()
                .filter(|running| running.number == number)
                .and_then(|running| running.turn_id.clone());
            match id {
                Some(id) => self.backend.add(&id, text.clone()).await.err(),
                None => Some("the turn has ended".to_owned()),
            }
        };
        let Some(refusal) = refusal else {
            return;
        };
        self.events.log(format!(
            "{} {}: the question could not join turn {number} ({refusal}); it waits for the next turn",
            self.name(),
            self.guild
        ));
        let start_now = {
            let mut state = self.lock();
            if let Some(running) = state
                .running
                .as_mut()
                .filter(|running| running.number == number)
            {
                if let Some(index) = running
                    .added
                    .iter()
                    .rposition(|added| *added == question.text)
                {
                    running.added.remove(index);
                }
            }
            if state.running.is_some() {
                state.waiting.push(Waiting { text, question });
                None
            } else {
                Some((text, question))
            }
        };
        if let Some((text, question)) = start_now {
            self.begin_turn(text, question, None);
        }
    }

    /// Starts the next turn with every question that waited, in order (D6').
    fn start_waiting(&self) {
        let waiting = std::mem::take(&mut self.lock().waiting);
        let mut waiting = waiting.into_iter();
        let Some(first) = waiting.next() else {
            return;
        };
        let rest: Vec<Waiting> = waiting.collect();
        let text = std::iter::once(first.text)
            .chain(rest.iter().map(|waiting| waiting.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        let number = self.begin_turn(text, first.question, None);
        if let Some(running) = self
            .lock()
            .running
            .as_mut()
            .filter(|running| running.number == number)
        {
            running.added = rest
                .into_iter()
                .map(|waiting| waiting.question.text)
                .collect();
        }
    }

    /// Detach: end the turn, silence the voice and forget the engine's mapping. The
    /// conversation survives.
    pub fn shutdown(&self) {
        let previous = {
            let mut state = self.lock();
            state.wanted = false;
            self.stop_running(&mut state)
        };
        if let Some(previous) = previous {
            self.interrupt(previous);
        }
        self.backend.detach();
        self.ports.output.detach_provider();
    }
}

impl TurnSink for TextSession {
    fn on_turn_event(&self, event: TurnEvent) {
        self.on_event(event);
    }
}

impl AssistantSession for TextSession {
    fn session_wanted(&self, wanted: bool) {
        let previous = {
            let mut state = self.lock();
            state.wanted = wanted;
            if wanted {
                None
            } else {
                state.context_sent = false;
                self.stop_running(&mut state)
            }
        };
        if let Some(previous) = previous {
            self.interrupt(previous);
        }
    }

    fn ready(&self) -> bool {
        self.backend.ready()
    }

    fn call_context(&self, caller: Option<String>, participants: &[String]) {
        let mut state = self.lock();
        state.call = CallContext {
            caller,
            participants: participants.to_vec(),
        };
        state.context_sent = false;
        if let Some(summary) = state.call.summary() {
            drop(state);
            self.chat("context", None, &summary);
        }
    }

    fn speakers_changed(&self, _: &[String]) {}

    fn user_text(&self, question: UserText) {
        if self.backend.conversation_id().is_none() {
            self.chat(
                "error",
                None,
                &format!(
                    "{} has no conversation for this channel; run /join again",
                    self.name()
                ),
            );
            return;
        }
        let options = (self.options)();
        let text = match (&question.speaker, options.speaker_labels) {
            (Some(name), SpeakerLabels::Start | SpeakerLabels::Changes) => {
                format!("{name}: {}", question.text)
            }
            _ => question.text.clone(),
        };
        self.chat("user", question.speaker.as_deref(), &question.text);
        self.conversation
            .lock()
            .expect("conversation lock")
            .push("user", text.clone());
        if options.one_speaker_at_a_time {
            // Change `speaker-turns`, D6': nothing is interrupted. A running turn takes the
            // question when the engine can add it; otherwise, or with none running, it
            // starts the next turn, and its answer is spoken after the current one.
            let added_to = {
                let mut state = self.lock();
                state.last_user_text = Some(question.text.clone());
                state.running.as_mut().map(|running| {
                    running.added.push(question.text.clone());
                    running.number
                })
            };
            if let Some(number) = added_to {
                self.events.log(format!(
                    "{} {}: question added to turn {number}",
                    self.name(),
                    self.guild
                ));
                let Some(session) = self.me.upgrade() else {
                    return;
                };
                self.handle.spawn(async move {
                    session.add_to_turn(number, text, question).await;
                });
                return;
            }
            self.begin_turn(text, question, None);
            return;
        }
        let previous = {
            let mut state = self.lock();
            self.stop_running(&mut state)
        };
        self.begin_turn(text, question, previous);
    }

    fn working(&self) -> bool {
        self.lock().running.is_some() || !self.player.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_context_goes_before_the_question_as_text() {
        let context = TurnContext {
            channel: Some("Discord voice channel. In the channel: Alex, Sam.".to_owned()),
            history: Some("Earlier: …".to_owned()),
        };
        assert_eq!(
            context.prefixed("Alex: hi"),
            "Earlier: …\n\n[Discord voice channel. In the channel: Alex, Sam.]\nAlex: hi"
        );
        assert_eq!(TurnContext::default().prefixed("q"), "q");
        assert!(TurnContext::default().is_empty());
    }
}
