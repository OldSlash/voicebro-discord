//! The Codex session of the Whisper mode (change `whisper-mode`, design D6): questions in
//! words as text turns on the guild's thread, the answer spoken by the local voice sentence
//! by sentence as it streams in. No realtime call is opened.
//!
//! - A question is `turn/start { threadId, input: [{ type: "text", text }] }`, prefixed with
//!   the speaker's name unless speaker labels are off. The first turn of each session
//!   carries the people in the channel as `additionalContext`, and the first turn on a new
//!   thread the guild's last exchanges.
//! - A new question while an answer runs or plays interrupts it (`turn/interrupt` before the
//!   new `turn/start`) and silences the voice at once.
//! - Every agent message of the turn is spoken; the last one of a completed turn is posted
//!   (`AudioPorts.answers`) with the question as the task.
//! - Ending the session interrupts a running turn and silences the voice.

use super::app_server::{AppServer, ThreadListener};
use super::conversation::{CallContext, Conversation};
use super::CodexConfig;
use crate::assistant::chat_log::{agent_item_line, ChatLog};
use crate::assistant::{AgentAnswer, AssistantSession, CallStatus, UserText};
use crate::audio::AudioPorts;
use crate::events::EventBus;
use crate::latency::{self, LatencyEvent, SharedLatency, WhisperTiming};
use crate::settings::SpeakerLabels;
use crate::status::ConnectionErrors;
use crate::tts::manager::TtsManager;
use crate::tts::player::SpeechPlayer;
use crate::tts::text::SentenceSplitter;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Instant;
use tokio::runtime::Handle;

/// What a text session is built from.
pub struct TextParts {
    pub guild: u64,
    pub handle: Handle,
    pub server: Arc<AppServer>,
    pub config: Arc<RwLock<CodexConfig>>,
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
    /// Known once `turn/start` answered or its first notification came.
    turn_id: Option<String>,
    question: String,
    language: Option<String>,
    epoch: u64,
    splitter: SentenceSplitter,
    /// No piece of this turn was queued yet: the first one marks the turn on the player.
    first_piece: bool,
    last_agent_text: Option<String>,
}

#[derive(Default)]
struct TextState {
    wanted: bool,
    call: CallContext,
    /// The call context went with this session's first turn.
    context_sent: bool,
    running: Option<Running>,
    turns: u64,
    /// Turns interrupted or replaced: their late notifications are ignored.
    stale: HashSet<String>,
    timings: HashMap<u64, Timing>,
    last_user_text: Option<String>,
    last_assistant_text: Option<String>,
    last_error: Option<String>,
}

pub struct CodexTextSession {
    me: Weak<Self>,
    guild: u64,
    handle: Handle,
    server: Arc<AppServer>,
    config: Arc<RwLock<CodexConfig>>,
    conversation: Arc<Mutex<Conversation>>,
    thread_id: String,
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

impl CodexTextSession {
    pub fn new(parts: TextParts) -> Arc<Self> {
        let thread_id = parts
            .conversation
            .lock()
            .expect("conversation lock")
            .thread
            .as_ref()
            .filter(|thread| thread.generation == parts.server.generation)
            .map(|thread| thread.id.clone())
            .unwrap_or_default();
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
                server: parts.server,
                config: parts.config,
                conversation: parts.conversation,
                thread_id: thread_id.clone(),
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
        if !thread_id.is_empty() {
            session
                .server
                .map_thread(&thread_id, Arc::downgrade(&session) as _);
        }
        session
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TextState> {
        self.state.lock().expect("text session lock")
    }

    fn chat(&self, kind: &'static str, who: Option<&str>, text: &str) {
        self.chat.push(self.guild, kind, who, text);
    }

    pub fn status(&self) -> CallStatus {
        let state = self.lock();
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
            connected: !self.thread_id.is_empty(),
            thread_id: (!self.thread_id.is_empty()).then(|| self.thread_id.clone()),
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

    /// The `additionalContext` of a turn: the people in the channel with the first turn of
    /// a session, the guild's last exchanges with the first turn on a new thread. Both are
    /// `application` context, which takes no `name` (Codex CLI 0.167.0 refuses one: "additional
    /// context name is only supported for untrusted context"; the canary of task 7.3).
    fn context(&self, state: &mut TextState) -> Option<Value> {
        let mut context = serde_json::Map::new();
        let labels = self
            .config
            .read()
            .expect("codex config lock")
            .speaker_labels;
        if !state.context_sent {
            state.context_sent = true;
            if labels != SpeakerLabels::Off {
                if let Some(summary) = state.call.summary() {
                    context.insert(
                        "voicebro.channel".to_owned(),
                        json!({
                            "kind": "application",
                            "value": format!("Discord voice channel. {summary}"),
                        }),
                    );
                }
            }
        }
        let mut conversation = self.conversation.lock().expect("conversation lock");
        if conversation.history_sent_to.as_deref() != Some(self.thread_id.as_str()) {
            conversation.history_sent_to = Some(self.thread_id.clone());
            if let Some(history) = conversation.history_text() {
                context.insert(
                    "voicebro.history".to_owned(),
                    json!({
                        "kind": "application",
                        "value": history,
                    }),
                );
            }
        }
        (!context.is_empty()).then_some(Value::Object(context))
    }

    /// Interrupts the turn in progress, if any, and silences the voice. Returns the turn id
    /// to interrupt on the server.
    fn stop_running(&self, state: &mut TextState) -> Option<String> {
        self.player.flush();
        let running = state.running.take()?;
        state.timings.remove(&running.number);
        let id = running.turn_id?;
        state.stale.insert(id.clone());
        Some(id)
    }

    fn interrupt(&self, turn_id: String) {
        let server = Arc::clone(&self.server);
        let thread_id = self.thread_id.clone();
        self.handle.spawn(async move {
            let _ = server
                .request(
                    "turn/interrupt",
                    json!({ "threadId": thread_id, "turnId": turn_id }),
                )
                .await;
        });
    }

    /// Queues the pieces of `running` for the voice.
    fn speak(&self, running: &mut Running, pieces: Vec<String>) {
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

    /// The running turn, when `params` belong to it (adopting the turn id if it was not
    /// known yet).
    fn running_for<'a>(state: &'a mut TextState, params: &Value) -> Option<&'a mut Running> {
        let id = params.get("turnId").and_then(Value::as_str).or_else(|| {
            params
                .get("turn")
                .and_then(|turn| turn.get("id"))
                .and_then(Value::as_str)
        })?;
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

    fn on_notification_inner(&self, method: &str, params: &Value) {
        match method {
            "item/agentMessage/delta" => {
                let delta = params.get("delta").and_then(Value::as_str).unwrap_or("");
                let mut state = self.lock();
                let state = &mut *state;
                let Some(number) = Self::running_for(state, params).map(|running| running.number)
                else {
                    return;
                };
                if let Some(timing) = state.timings.get_mut(&number) {
                    timing.first_words_at.get_or_insert_with(Instant::now);
                }
                let Some(running) = state.running.as_mut() else {
                    return;
                };
                let pieces = running.splitter.push(delta);
                self.speak(running, pieces);
            }
            "item/completed" => {
                let item = params.get("item").cloned().unwrap_or(Value::Null);
                let kind = item.get("type").and_then(Value::as_str);
                let line = agent_item_line(&item);
                {
                    let mut state = self.lock();
                    let state = &mut *state;
                    let Some(running) = Self::running_for(state, params) else {
                        return;
                    };
                    if kind == Some("agentMessage") {
                        let pieces = running.splitter.finish();
                        self.speak(running, pieces);
                        let text = item
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_owned();
                        running.last_agent_text = Some(text.clone());
                        state.last_assistant_text = Some(text);
                    }
                }
                // The question itself is in the chat already.
                if let Some((kind, text)) = line.filter(|(kind, _)| *kind != "task") {
                    self.chat(kind, None, &text);
                }
            }
            "turn/completed" => {
                let turn = params.get("turn");
                let status = turn
                    .and_then(|turn| turn.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                let finished = {
                    let mut state = self.lock();
                    let state = &mut *state;
                    if Self::running_for(state, params).is_none() {
                        return;
                    }
                    state.running.take()
                };
                let Some(running) = finished else {
                    return;
                };
                if status == "completed" {
                    if let Some(text) = running.last_agent_text.filter(|text| !text.is_empty()) {
                        {
                            let mut conversation =
                                self.conversation.lock().expect("conversation lock");
                            conversation.push("assistant", text.clone());
                        }
                        self.ports.answers.send(AgentAnswer {
                            guild: self.guild,
                            task: Some(running.question),
                            text,
                        });
                    }
                } else {
                    let error = turn
                        .and_then(|turn| turn.get("error"))
                        .and_then(|error| error.get("message"))
                        .and_then(Value::as_str)
                        .map(|message| format!(": {message}"))
                        .unwrap_or_default();
                    self.chat(
                        "error",
                        None,
                        &format!("Codex: the answer {status}{error}."),
                    );
                }
            }
            _ => {}
        }
    }

    /// Starts the turn of question `number` after interrupting `previous`.
    async fn start_turn(&self, number: u64, previous: Option<String>, text: String) {
        let _one = self.turn_lock.lock().await;
        if let Some(previous) = previous {
            let _ = self
                .server
                .request(
                    "turn/interrupt",
                    json!({ "threadId": self.thread_id, "turnId": previous }),
                )
                .await;
        }
        let params = {
            let mut state = self.lock();
            if state.running.as_ref().map(|running| running.number) != Some(number) {
                return; // Replaced or ended before it started.
            }
            let mut params = json!({
                "threadId": self.thread_id,
                "input": [{ "type": "text", "text": text }],
            });
            if let Some(context) = self.context(&mut state) {
                params["additionalContext"] = context;
            }
            params
        };
        match self.server.request("turn/start", params).await {
            Ok(reply) => {
                let id = reply
                    .get("turn")
                    .and_then(|turn| turn.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
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
                        // Ended while it was starting: interrupt it on the server too.
                        None => id.inspect(|id| {
                            state.stale.insert(id.clone());
                        }),
                    }
                };
                if let Some(stale) = stale {
                    self.interrupt(stale);
                }
                self.errors.report("codex", None);
            }
            Err(error) => {
                let message = format!("Codex could not take the question: {error}");
                {
                    let mut state = self.lock();
                    if state.running.as_ref().map(|running| running.number) == Some(number) {
                        state.running = None;
                    }
                    state.last_error = Some(message.clone());
                }
                self.chat("error", None, &message);
                self.events.log(format!("Codex {}: {message}", self.guild));
                self.errors.report("codex", Some(message));
            }
        }
    }

    /// Detach: end the turn, silence the voice and forget the thread mapping. The
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
        self.server.unmap_thread(&self.thread_id);
        self.ports.output.detach_provider();
    }
}

impl ThreadListener for CodexTextSession {
    fn on_notification(&self, method: &str, params: &Value) {
        self.on_notification_inner(method, params);
    }

    fn on_declined(&self, method: &str) {
        self.events.log(format!(
            "Codex {}: declined {method} (nothing may act on this computer)",
            self.guild
        ));
    }

    fn on_server_exit(&self) {
        {
            let mut state = self.lock();
            self.stop_running(&mut state);
            state.last_error = Some("Codex app-server exited".to_owned());
        }
        self.errors.report(
            "codex",
            Some("Codex app-server exited; run /join again".to_owned()),
        );
    }
}

impl AssistantSession for CodexTextSession {
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
        !self.thread_id.is_empty() && self.server.is_alive()
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
        if self.thread_id.is_empty() {
            self.chat(
                "error",
                None,
                "Codex has no thread for this channel; run /join again",
            );
            return;
        }
        let labels = self
            .config
            .read()
            .expect("codex config lock")
            .speaker_labels;
        let text = match (&question.speaker, labels) {
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
        let (number, previous) = {
            let mut state = self.lock();
            let previous = self.stop_running(&mut state);
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
            (number, previous)
        };
        let Some(session) = self.me.upgrade() else {
            return;
        };
        self.handle.spawn(async move {
            session.start_turn(number, previous, text).await;
        });
    }

    fn working(&self) -> bool {
        self.lock().running.is_some() || !self.player.is_empty()
    }
}
