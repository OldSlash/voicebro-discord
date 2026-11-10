//! One guild's Codex voice session: the state machine between the gate's `session_wanted`
//! and the realtime call (idle → starting → live → stopping), restarts, the conversation,
//! and the notifications of its thread.

use super::account::{AccountInfo, Voices};
use super::app_server::AppServer;
use super::conversation::{
    call_instructions, call_names_note, speaker_note, CallContext, Conversation,
};
use super::rtc::{CallReport, CallSlot, RtcCall};
use super::thread::{open_thread, ThreadSettings};
use super::CodexConfig;
use crate::assistant::chat_log::{agent_item_line, ChatLog};
use crate::assistant::{AssistantSession, CallStatus};
use crate::audio::AudioPorts;
use crate::events::EventBus;
use crate::settings::SpeakerLabels;
use crate::status::{log_used, ConnectionErrors};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

const REALTIME_START_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_RESTARTS: u32 = 3;
/// How the log names the voice when the call is started without one.
const SERVER_DEFAULT_VOICE: &str = "server default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Realtime {
    Idle,
    Starting,
    Live,
    Stopping,
}

impl Realtime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Live => "live",
            Self::Stopping => "stopping",
        }
    }
}

struct SessionState {
    realtime: Realtime,
    wanted: bool,
    restarts: u32,
    last_error: Option<String>,
    last_user_text: Option<String>,
    last_assistant_text: Option<String>,
}

fn truncate(text: &str, maximum: usize) -> String {
    if text.chars().count() <= maximum {
        return text.to_owned();
    }
    format!("{}…", text.chars().take(maximum).collect::<String>())
}

/// How the Codex agent's answer reaches the voice model (`codex.handoff_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffMode {
    /// The answer is added to the conversation as its own item (`codexResponsesAsItems`).
    Items,
    /// `codexResponseHandoffMode: commentary`.
    Commentary,
    /// `codexResponseHandoffMode: bemTags`.
    BemTags,
    /// Codex's built-in routing: neither field is sent.
    Thinking,
}

impl HandoffMode {
    /// The setting's value; validation admits only the four, anything else reads as the
    /// default.
    pub fn from_setting(value: &str) -> Self {
        match value {
            "commentary" => HandoffMode::Commentary,
            "bem_tags" => HandoffMode::BemTags,
            "thinking" => HandoffMode::Thinking,
            _ => HandoffMode::Items,
        }
    }

    /// How the log and the chat name the mode.
    pub fn label(self) -> &'static str {
        match self {
            HandoffMode::Items => "conversation items",
            HandoffMode::Commentary => "commentary",
            HandoffMode::BemTags => "tagged channels",
            HandoffMode::Thinking => "built-in (thinking)",
        }
    }

    /// Adds this mode's field to `thread/realtime/start`.
    fn apply(self, params: &mut Value) {
        match self {
            HandoffMode::Items => params["codexResponsesAsItems"] = json!(true),
            HandoffMode::Commentary => params["codexResponseHandoffMode"] = json!("commentary"),
            HandoffMode::BemTags => params["codexResponseHandoffMode"] = json!("bemTags"),
            HandoffMode::Thinking => {}
        }
    }

    /// Removes every handoff field: what the built-in routing sends.
    fn strip(params: &mut Value) {
        if let Some(object) = params.as_object_mut() {
            object.remove("codexResponsesAsItems");
            object.remove("codexResponseHandoffMode");
        }
    }
}

/// `thread/realtime/start` for a WebRTC call, before the conversation is added. `voice`
/// `None` leaves the field out, so the server uses its default voice.
pub fn realtime_start_params(
    thread_id: &str,
    prompt: &str,
    offer: &str,
    voice: Option<String>,
    handoff: HandoffMode,
) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "outputModality": "audio",
        "version": "v3",
        "prompt": prompt,
        "includeStartupContext": false,
        "transport": { "type": "webrtc", "sdp": offer },
    });
    if let Some(voice) = voice {
        params["voice"] = json!(voice);
    }
    handoff.apply(&mut params);
    params
}

/// The line that opens a call in the log and the chat.
pub fn call_start_line(voice: Option<&str>, codex: &str, handoff: HandoffMode) -> String {
    format!(
        "Call starting: voice {}; Codex: {codex}; answers: {}.",
        voice.unwrap_or(SERVER_DEFAULT_VOICE),
        handoff.label()
    )
}

pub struct CodexSession {
    me: Weak<Self>,
    guild: u64,
    handle: Handle,
    server: Arc<AppServer>,
    config: Arc<RwLock<CodexConfig>>,
    account: Arc<Mutex<Option<AccountInfo>>>,
    voices: Arc<Mutex<Option<Voices>>>,
    conversation: Arc<Mutex<Conversation>>,
    workspace: PathBuf,
    thread_id: Mutex<String>,
    call: Mutex<CallContext>,
    ports: AudioPorts,
    state: Mutex<SessionState>,
    rtc: CallSlot,
    /// Where each call of this session reports its counters when it ends.
    report: CallReport,
    pending_answer: Mutex<Option<oneshot::Sender<Result<String, String>>>>,
    chat: Arc<ChatLog>,
    events: EventBus,
    errors: Arc<ConnectionErrors>,
}

pub struct SessionParts {
    pub guild: u64,
    pub handle: Handle,
    pub server: Arc<AppServer>,
    pub config: Arc<RwLock<CodexConfig>>,
    pub account: Arc<Mutex<Option<AccountInfo>>>,
    pub voices: Arc<Mutex<Option<Voices>>>,
    pub conversation: Arc<Mutex<Conversation>>,
    pub workspace: PathBuf,
    pub ports: AudioPorts,
    pub chat: Arc<ChatLog>,
    pub events: EventBus,
    pub errors: Arc<ConnectionErrors>,
}

impl CodexSession {
    pub fn new(parts: SessionParts) -> Arc<Self> {
        let thread_id = parts
            .conversation
            .lock()
            .expect("conversation lock")
            .thread
            .as_ref()
            .filter(|thread| thread.generation == parts.server.generation)
            .map(|thread| thread.id.clone())
            .unwrap_or_default();
        let session = Arc::new_cyclic(|me| Self {
            me: me.clone(),
            guild: parts.guild,
            handle: parts.handle,
            server: parts.server,
            config: parts.config,
            account: parts.account,
            voices: parts.voices,
            conversation: parts.conversation,
            workspace: parts.workspace,
            thread_id: Mutex::new(thread_id.clone()),
            call: Mutex::new(CallContext::default()),
            ports: parts.ports,
            state: Mutex::new(SessionState {
                realtime: Realtime::Idle,
                wanted: false,
                restarts: 0,
                last_error: None,
                last_user_text: None,
                last_assistant_text: None,
            }),
            rtc: Mutex::new(None),
            report: CallReport::new(parts.guild),
            pending_answer: Mutex::new(None),
            chat: parts.chat,
            events: parts.events,
            errors: parts.errors,
        });
        if !thread_id.is_empty() {
            session
                .server
                .map_thread(&thread_id, Arc::downgrade(&session));
        }
        session
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SessionState> {
        self.state.lock().expect("session state lock")
    }

    pub fn thread_id(&self) -> String {
        self.thread_id.lock().expect("thread id lock").clone()
    }

    fn config(&self) -> CodexConfig {
        self.config.read().expect("codex config lock").clone()
    }

    fn chat(&self, kind: &'static str, who: Option<&str>, text: &str) {
        self.chat.push(self.guild, kind, who, text);
    }

    fn emit(&self, message: &str) {
        self.events.log(format!("Codex {}: {message}", self.guild));
    }

    fn set_error(&self, error: String) {
        self.lock().last_error = Some(error.clone());
        self.emit(&error);
        self.errors.report("codex", Some(error));
    }

    fn clear_error(&self) {
        self.lock().last_error = None;
        self.errors.report("codex", None);
    }

    fn realtime(&self) -> Realtime {
        self.lock().realtime
    }

    fn rtc(&self) -> Option<Arc<RtcCall>> {
        self.rtc.lock().expect("rtc lock").clone()
    }

    pub fn status(&self) -> CallStatus {
        let state = self.lock();
        let rtc = self.rtc();
        // The running call's counters, else the last ended call's.
        let audio = rtc
            .as_ref()
            .filter(|rtc| !rtc.is_closed())
            .map(|rtc| rtc.counters())
            .or(*self.report.last.lock().expect("call report lock"));
        CallStatus {
            guild_id: self.guild.to_string(),
            state: state.realtime.as_str().to_owned(),
            connected: rtc.is_some_and(|rtc| rtc.is_connected()),
            audio,
            thread_id: Some(self.thread_id()).filter(|id| !id.is_empty()),
            last_user_text: state.last_user_text.clone(),
            last_assistant_text: state.last_assistant_text.clone(),
            last_error: state.last_error.clone(),
        }
    }

    /// Makes sure the thread matches the model and reasoning settings. A change creates
    /// a new thread; the conversation carries over through the call's initial items.
    async fn ensure_thread(&self) -> Result<(), String> {
        let config = self.config();
        let settings = ThreadSettings {
            instructions: config.instructions.clone(),
            model: config.model.trim().to_owned(),
            effort: config.reasoning_effort.trim().to_owned(),
        };
        let current = self
            .conversation
            .lock()
            .expect("conversation lock")
            .thread
            .clone();
        if let Some(thread) = current.as_ref() {
            if thread.generation == self.server.generation
                && thread.requested == settings
                && thread.id == self.thread_id()
            {
                return Ok(());
            }
        }
        let thread = open_thread(&self.server, &self.events, &self.workspace, &settings).await?;
        let id = thread.id.clone();
        let description = ThreadSettings {
            instructions: String::new(),
            model: thread.model.clone(),
            effort: thread.effort.clone(),
        }
        .describe();
        self.conversation.lock().expect("conversation lock").thread = Some(thread);
        let previous = std::mem::replace(
            &mut *self.thread_id.lock().expect("thread id lock"),
            id.clone(),
        );
        if !previous.is_empty() {
            self.server.unmap_thread(&previous);
        }
        self.server.map_thread(&id, self.me.clone());
        self.chat("system", None, &format!("New Codex thread: {description}."));
        Ok(())
    }

    /// The voice to use and whether it fell back to the default.
    fn choose_voice(&self) -> Option<String> {
        let configured = self.config().voice;
        let voices = self.voices.lock().expect("voices lock").clone();
        match voices {
            Some(voices) => {
                let (voice, fell_back) = voices.choose(&configured);
                if fell_back {
                    self.events.log(log_used(
                        "Codex voice",
                        &configured,
                        voice.as_deref().unwrap_or(SERVER_DEFAULT_VOICE),
                        "not available with this sign-in",
                    ));
                }
                voice
            }
            None => Some(configured).filter(|voice| !voice.trim().is_empty()),
        }
    }

    /// Developer notes for the start of a call: the user's instructions and who is in the
    /// call. They go last in the initial items, where the model follows them.
    fn call_notes(&self) -> Vec<String> {
        let config = self.config();
        let call = self.call.lock().expect("call lock").clone();
        let mut notes = Vec::new();
        if !config.instructions.trim().is_empty() {
            notes.push(config.instructions.trim().to_owned());
        }
        notes.extend(call_names_note(config.speaker_labels, &call));
        notes
    }

    fn prompt(&self) -> String {
        let config = self.config();
        let call = self.call.lock().expect("call lock").clone();
        call_instructions(&config.instructions, config.speaker_labels, &call)
    }

    /// Adds the conversation's identity and history so a new call continues it.
    fn add_conversation(&self, params: &mut Value, with_history: bool) {
        let notes = self.call_notes();
        let conversation = self.conversation.lock().expect("conversation lock");
        params["voiceSessionId"] = json!(conversation.voice_session_id);
        if let Some(id) = conversation.realtime_session_id.as_ref() {
            params["realtimeSessionId"] = json!(id);
        }
        let items = if with_history {
            conversation.initial_items(&notes)
        } else {
            notes
                .iter()
                .map(|text| json!({ "role": "developer", "text": text }))
                .collect()
        };
        if items.is_empty() {
            if let Some(object) = params.as_object_mut() {
                object.remove("initialItems");
            }
        } else {
            params["initialItems"] = Value::Array(items);
        }
    }

    /// Starts realtime. If Codex refuses the answer handoff field (it is experimental), it
    /// retries once with the built-in routing and says so; then, if Codex rejects the
    /// history, with only this call's notes.
    async fn start_with_history(
        &self,
        mut params: Value,
        handoff: HandoffMode,
    ) -> Result<Value, String> {
        self.add_conversation(&mut params, true);
        let had_history = !self
            .conversation
            .lock()
            .expect("conversation lock")
            .is_empty();
        let first = match self
            .server
            .request("thread/realtime/start", params.clone())
            .await
        {
            Err(error) if handoff != HandoffMode::Thinking => {
                let line = log_used(
                    "answer handoff",
                    handoff.label(),
                    HandoffMode::Thinking.label(),
                    &format!("Codex refused the call with it: {error}"),
                );
                self.chat("system", None, &line);
                HandoffMode::strip(&mut params);
                self.server
                    .request("thread/realtime/start", params.clone())
                    .await
            }
            result => result,
        };
        match first {
            Err(error) if had_history => {
                self.emit(&format!(
                    "Codex rejected the conversation history ({error}); starting without it"
                ));
                self.add_conversation(&mut params, false);
                self.server.request("thread/realtime/start", params).await
            }
            result => result,
        }
    }

    async fn start_realtime(self: Arc<Self>) {
        let signed_in = self
            .account
            .lock()
            .expect("account lock")
            .as_ref()
            .is_some_and(|account| account.signed_in);
        if !signed_in {
            self.lock().realtime = Realtime::Idle;
            let message = "Codex is not signed in: use Sign in on the Status tab, finish the sign-in in the browser, then try again".to_owned();
            self.chat("error", None, &message);
            self.set_error(message);
            return;
        }
        if let Err(error) = self.ensure_thread().await {
            self.lock().realtime = Realtime::Idle;
            self.chat("error", None, &error);
            self.set_error(format!("could not open a Codex thread: {error}"));
            return;
        }
        let voice = self.choose_voice();
        let handoff = HandoffMode::from_setting(&self.config().handoff_mode);
        self.emit(&format!(
            "starting the voice call (voice {}, answers: {})",
            voice.as_deref().unwrap_or(SERVER_DEFAULT_VOICE),
            handoff.label()
        ));
        {
            let description = self
                .conversation
                .lock()
                .expect("conversation lock")
                .thread
                .as_ref()
                .map(|thread| {
                    ThreadSettings {
                        instructions: String::new(),
                        model: thread.model.clone(),
                        effort: thread.effort.clone(),
                    }
                    .describe()
                })
                .unwrap_or_default();
            self.chat(
                "system",
                None,
                &call_start_line(voice.as_deref(), &description, handoff),
            );
        }
        if self.config().speaker_labels != SpeakerLabels::Off {
            let summary = self.call.lock().expect("call lock").summary();
            if let Some(summary) = summary {
                self.chat("context", None, &summary);
            }
        }

        // The peer, track and data channel before the offer.
        let (rtc, offer) =
            match RtcCall::open(self.handle.clone(), self.ports.clone(), self.report.clone()).await
            {
                Ok(opened) => opened,
                Err(error) => {
                    self.lock().realtime = Realtime::Idle;
                    self.set_error(format!("could not set up the voice call: {error}"));
                    return;
                }
            };
        let previous = self.rtc.lock().expect("rtc lock").replace(Arc::clone(&rtc));
        if let Some(previous) = previous {
            previous.close().await;
        }
        let (answer_tx, answer_rx) = oneshot::channel();
        *self.pending_answer.lock().expect("answer lock") = Some(answer_tx);

        let params =
            realtime_start_params(&self.thread_id(), &self.prompt(), &offer, voice, handoff);
        if let Err(error) = self.start_with_history(params, handoff).await {
            self.pending_answer.lock().expect("answer lock").take();
            rtc.close().await;
            self.lock().realtime = Realtime::Idle;
            self.chat("error", None, &error);
            self.set_error(format!("could not start the voice call: {error}"));
            return;
        }
        let answer = match tokio::time::timeout(REALTIME_START_TIMEOUT, answer_rx).await {
            Ok(Ok(Ok(sdp))) => sdp,
            Ok(Ok(Err(error))) => {
                self.fail_start(&rtc, format!("the voice call was refused: {error}"))
                    .await;
                return;
            }
            Ok(Err(_)) => {
                self.fail_start(
                    &rtc,
                    "the voice call ended before Codex answered".to_owned(),
                )
                .await;
                return;
            }
            Err(_) => {
                self.fail_start(
                    &rtc,
                    "Codex did not answer the voice call in time".to_owned(),
                )
                .await;
                return;
            }
        };
        if let Err(error) = rtc.set_answer(answer).await {
            self.fail_start(&rtc, format!("could not apply the call answer: {error}"))
                .await;
            return;
        }
        match rtc.wait_connected(CONNECT_TIMEOUT).await {
            Ok(elapsed) => {
                self.emit(&format!("voice call connected after {elapsed:?}"));
                self.clear_error();
            }
            Err(error) => {
                self.fail_start(&rtc, format!("the voice call did not connect: {error}"))
                    .await;
                return;
            }
        }
        // The transport is connected; wait for Codex to report the realtime session.
        let deadline = tokio::time::Instant::now() + REALTIME_START_TIMEOUT;
        while self.realtime() == Realtime::Starting {
            if tokio::time::Instant::now() >= deadline {
                self.fail_start(&rtc, "the voice session did not start in time".to_owned())
                    .await;
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.watch_transport(Arc::clone(&rtc));
        let stop = {
            let state = self.lock();
            !state.wanted && state.realtime == Realtime::Live
        };
        if stop {
            self.lock().realtime = Realtime::Stopping;
            self.stop_realtime().await;
        }
    }

    async fn fail_start(&self, rtc: &RtcCall, error: String) {
        self.pending_answer.lock().expect("answer lock").take();
        rtc.close().await;
        let _ = self
            .server
            .request(
                "thread/realtime/stop",
                json!({ "threadId": self.thread_id() }),
            )
            .await;
        self.lock().realtime = Realtime::Idle;
        self.chat("error", None, &error);
        self.set_error(error);
        self.ports.output.flush();
    }

    /// Ends the session when the transport drops while live, with the same restart rule
    /// as a `closed` notification.
    fn watch_transport(&self, rtc: Arc<RtcCall>) {
        let Some(session) = self.me.upgrade() else {
            return;
        };
        let mut state = rtc.state_watch();
        self.handle.spawn(async move {
            while state.changed().await.is_ok() {
                let current = state.borrow().clone();
                if rtc.is_closed() {
                    break;
                }
                if current == "failed" || current == "disconnected" || current == "closed" {
                    session.on_transport_lost(&current);
                    break;
                }
            }
        });
    }

    fn on_transport_lost(&self, state: &str) {
        let live = {
            let current = self.lock();
            current.realtime == Realtime::Live && self.rtc().is_some_and(|rtc| !rtc.is_closed())
        };
        if !live {
            return;
        }
        self.chat("system", None, &format!("The call dropped ({state})."));
        self.on_closed(&format!("transport {state}"));
    }

    async fn stop_realtime(self: Arc<Self>) {
        let rtc = self.rtc.lock().expect("rtc lock").take();
        if let Some(rtc) = rtc {
            rtc.close().await;
        }
        let _ = self
            .server
            .request(
                "thread/realtime/stop",
                json!({ "threadId": self.thread_id() }),
            )
            .await;
        let restart = {
            let mut state = self.lock();
            if state.realtime == Realtime::Stopping {
                state.realtime = Realtime::Idle;
            }
            if state.wanted && state.realtime == Realtime::Idle {
                state.realtime = Realtime::Starting;
                true
            } else {
                false
            }
        };
        self.ports.output.flush();
        if restart {
            Box::pin(self.start_realtime()).await;
        } else {
            self.emit("voice call stopped");
            self.chat("system", None, "Call ended.");
        }
    }

    /// A `closed` notification or a lost transport: restart while wanted, at most
    /// `MAX_RESTARTS` times per session.
    fn on_closed(&self, reason: &str) {
        let restart = {
            let mut state = self.lock();
            let was_live = state.realtime == Realtime::Live;
            state.realtime = Realtime::Idle;
            if state.wanted && was_live && state.restarts < MAX_RESTARTS {
                state.restarts += 1;
                state.realtime = Realtime::Starting;
                Some(state.restarts)
            } else {
                None
            }
        };
        if let Some(rtc) = self.rtc.lock().expect("rtc lock").take() {
            self.handle.spawn(async move { rtc.close().await });
        }
        self.ports.output.flush();
        self.emit(&format!("voice call closed ({reason})"));
        match (restart, self.me.upgrade()) {
            (Some(attempt), Some(session)) => {
                self.emit(&format!(
                    "restarting the call (attempt {attempt} of {MAX_RESTARTS})"
                ));
                self.handle.spawn(async move {
                    tokio::time::sleep(Duration::from_secs(2 * u64::from(attempt))).await;
                    session.start_realtime().await;
                });
            }
            _ => {
                let wanted = self.lock().wanted;
                if wanted {
                    self.set_error(format!(
                        "the voice call failed {MAX_RESTARTS} times in a row ({reason}); say the deactivation phrase and try again"
                    ));
                }
            }
        }
    }

    /// The app-server declined a request on this thread.
    pub fn on_declined(&self, method: &str) {
        self.chat(
            "system",
            None,
            &format!("Declined {method}: nothing may act on this computer."),
        );
    }

    pub fn on_notification(&self, method: &str, params: &Value) {
        match method {
            "thread/realtime/sdp" => {
                let answer = params
                    .get("sdp")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| "Codex sent an empty SDP answer".to_owned());
                if let Some(sender) = self.pending_answer.lock().expect("answer lock").take() {
                    let _ = sender.send(answer);
                }
            }
            "thread/realtime/started" => {
                if let Some(id) = params.get("realtimeSessionId").and_then(Value::as_str) {
                    self.conversation
                        .lock()
                        .expect("conversation lock")
                        .realtime_session_id = Some(id.to_owned());
                }
                {
                    let mut state = self.lock();
                    state.realtime = Realtime::Live;
                    state.last_error = None;
                }
                self.emit("voice session is live");
                self.chat("system", None, "Call in progress.");
            }
            "thread/realtime/transcript/done" => {
                let role = params.get("role").and_then(Value::as_str).unwrap_or("");
                let text = params
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                if text.is_empty() {
                    return;
                }
                {
                    let mut state = self.lock();
                    if role == "user" {
                        state.last_user_text = Some(truncate(&text, 300));
                    } else {
                        state.last_assistant_text = Some(truncate(&text, 300));
                    }
                }
                let speaker = {
                    let mut conversation = self.conversation.lock().expect("conversation lock");
                    if role == "user" {
                        let speaker = conversation.speakers.clone();
                        let entry = match speaker.as_ref() {
                            Some(names) => format!("{names}: {text}"),
                            None => text.clone(),
                        };
                        conversation.push("user", entry);
                        speaker
                    } else {
                        conversation.push("assistant", text.clone());
                        None
                    }
                };
                if role == "user" {
                    self.chat("user", speaker.as_deref(), &text);
                } else {
                    self.chat("assistant", None, &text);
                }
            }
            "thread/realtime/error" => {
                let message = params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown realtime error")
                    .to_owned();
                if let Some(sender) = self.pending_answer.lock().expect("answer lock").take() {
                    let _ = sender.send(Err(message.clone()));
                }
                self.chat("error", None, &message);
                self.set_error(format!("voice session error: {message}"));
            }
            "thread/realtime/closed" => {
                let reason = params
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("closed")
                    .to_owned();
                if self.realtime() == Realtime::Live {
                    self.chat(
                        "system",
                        None,
                        &format!("The call was interrupted ({reason})."),
                    );
                }
                if self.realtime() == Realtime::Stopping {
                    // Our own stop; stop_realtime finishes the state change.
                    return;
                }
                self.on_closed(&reason);
            }
            // The Codex agent working on a task the voice assistant delegated.
            "turn/started" => self.chat("system", None, "Codex took a task."),
            "turn/completed" => {
                let turn = params.get("turn");
                let status = turn
                    .and_then(|turn| turn.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                let seconds = turn
                    .and_then(|turn| turn.get("durationMs"))
                    .and_then(Value::as_i64)
                    .map(|ms| format!(" in {:.1} s", ms as f64 / 1000.0))
                    .unwrap_or_default();
                let error = turn
                    .and_then(|turn| turn.get("error"))
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default();
                let text = match status {
                    "completed" => format!("Codex finished the task{seconds}."),
                    "interrupted" => format!("Codex interrupted the task{seconds}."),
                    other => format!("Codex: task {other}{seconds}{error}."),
                };
                self.chat(
                    if error.is_empty() { "system" } else { "error" },
                    None,
                    &text,
                );
            }
            "item/completed" => {
                if let Some((kind, text)) = params.get("item").and_then(agent_item_line) {
                    self.chat(kind, None, &text);
                }
            }
            _ => {}
        }
    }

    pub fn on_server_exit(&self) {
        {
            let mut state = self.lock();
            state.realtime = Realtime::Idle;
            state.last_error = Some("Codex app-server exited".to_owned());
        }
        if let Some(rtc) = self.rtc.lock().expect("rtc lock").take() {
            self.handle.spawn(async move { rtc.close().await });
        }
        self.ports.output.flush();
        self.errors.report(
            "codex",
            Some("Codex app-server exited; it restarts at the next call".to_owned()),
        );
    }

    /// Detach: end any call and forget the thread mapping. The conversation survives.
    pub fn shutdown(&self) {
        let thread_id = self.thread_id();
        self.server.unmap_thread(&thread_id);
        let live = {
            let mut state = self.lock();
            state.wanted = false;
            let live = matches!(state.realtime, Realtime::Starting | Realtime::Live);
            state.realtime = Realtime::Idle;
            live
        };
        if live && !thread_id.is_empty() {
            self.server
                .request_without_reply("thread/realtime/stop", json!({ "threadId": thread_id }));
        }
        if let Some(rtc) = self.rtc.lock().expect("rtc lock").take() {
            self.handle.spawn(async move { rtc.close().await });
        }
        self.ports.output.flush();
    }
}

impl AssistantSession for CodexSession {
    fn session_wanted(&self, wanted: bool) {
        let action = {
            let mut state = self.lock();
            state.wanted = wanted;
            if wanted {
                state.restarts = 0;
            }
            match (wanted, state.realtime) {
                (true, Realtime::Idle) => {
                    state.realtime = Realtime::Starting;
                    Some(true)
                }
                (false, Realtime::Starting | Realtime::Live) => {
                    state.realtime = Realtime::Stopping;
                    Some(false)
                }
                _ => None,
            }
        };
        if !wanted {
            self.ports.output.flush();
        }
        let Some(start) = action else {
            return;
        };
        let Some(session) = self.me.upgrade() else {
            return;
        };
        self.handle.spawn(async move {
            if start {
                session.start_realtime().await;
            } else {
                session.stop_realtime().await;
            }
        });
    }

    fn ready(&self) -> bool {
        self.realtime() == Realtime::Live && self.rtc().is_some_and(|rtc| rtc.is_connected())
    }

    fn call_context(&self, caller: Option<String>, participants: &[String]) {
        if let Some(caller) = caller.as_ref() {
            self.conversation
                .lock()
                .expect("conversation lock")
                .speakers = Some(caller.clone());
        }
        *self.call.lock().expect("call lock") = CallContext {
            caller,
            participants: participants.to_vec(),
        };
    }

    fn speakers_changed(&self, names: &[String]) {
        if names.is_empty() {
            return;
        }
        self.conversation
            .lock()
            .expect("conversation lock")
            .speakers = Some(names.join(", "));
        if self.config().speaker_labels != SpeakerLabels::Changes {
            return;
        }
        let Some(note) = speaker_note(names) else {
            return;
        };
        if self.realtime() != Realtime::Live {
            return;
        }
        // Realtime v3 adds developer text as session context; it does not start a reply.
        let Some(session) = self.me.upgrade() else {
            return;
        };
        let params = json!({ "threadId": self.thread_id(), "text": note, "role": "developer" });
        self.handle.spawn(async move {
            match session
                .server
                .request("thread/realtime/appendText", params)
                .await
            {
                Ok(_) => session.chat("context", None, &note),
                Err(error) => session.emit(&format!("speaker note failed: {error}")),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_parameters_leave_out_an_empty_voice() {
        let default = realtime_start_params("t1", "p", "sdp", None, HandoffMode::Thinking);
        assert!(
            default.get("voice").is_none(),
            "server default: no voice field"
        );
        assert_eq!(default["version"], "v3");
        assert_eq!(default["transport"]["type"], "webrtc");
        let chosen = realtime_start_params(
            "t1",
            "p",
            "sdp",
            Some("cove".to_owned()),
            HandoffMode::Thinking,
        );
        assert_eq!(chosen["voice"], "cove");
    }

    #[test]
    fn each_handoff_mode_sends_its_own_field() {
        let start = |mode: &str| {
            realtime_start_params("t1", "p", "sdp", None, HandoffMode::from_setting(mode))
        };
        let items = start("items");
        assert_eq!(items["codexResponsesAsItems"], true);
        assert!(items.get("codexResponseHandoffMode").is_none());
        assert_eq!(
            start("commentary")["codexResponseHandoffMode"],
            "commentary"
        );
        assert!(start("commentary").get("codexResponsesAsItems").is_none());
        assert_eq!(start("bem_tags")["codexResponseHandoffMode"], "bemTags");
        let thinking = start("thinking");
        assert!(thinking.get("codexResponsesAsItems").is_none());
        assert!(
            thinking.get("codexResponseHandoffMode").is_none(),
            "the built-in routing sends what the app sent before"
        );
        assert_eq!(HandoffMode::from_setting(""), HandoffMode::Items);
    }

    /// Codex refuses the handoff field: the start is retried once with the built-in
    /// routing, the chat and the log name both modes, and the built-in routing itself is
    /// sent as it is. Uses the Node fake app-server; checks nothing without Node.
    #[test]
    fn a_refused_handoff_field_falls_back_to_the_built_in_routing() {
        use crate::audio::{frame_channel, output_pair, INPUT_CAPACITY};
        let node = std::process::Command::new("node").arg("--version").output();
        if !node.is_ok_and(|output| output.status.success()) {
            return;
        }
        let fake = format!(
            "{}/testdata/fake-codex/fake_codex_rejects_handoff.cmd",
            env!("CARGO_MANIFEST_DIR")
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let events = EventBus::new(64);
        let server = runtime
            .block_on(AppServer::spawn(
                runtime.handle().clone(),
                events.clone(),
                std::path::Path::new(&fake),
            ))
            .expect("the fake starts");
        let (_sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        let chat = Arc::new(ChatLog::new(events.clone(), dir.path().to_path_buf()));
        let session = CodexSession::new(SessionParts {
            guild: 1,
            handle: runtime.handle().clone(),
            server: Arc::clone(&server),
            config: Arc::new(RwLock::new(CodexConfig::default())),
            account: Arc::new(Mutex::new(None)),
            voices: Arc::new(Mutex::new(None)),
            conversation: Arc::new(Mutex::new(Conversation::new(1))),
            workspace: dir.path().to_path_buf(),
            ports: AudioPorts { input, output },
            chat: Arc::clone(&chat),
            events: events.clone(),
            errors: Arc::new(ConnectionErrors::new(events)),
        });

        let items = realtime_start_params("t1", "p", "sdp", None, HandoffMode::Items);
        let started = runtime
            .block_on(session.start_with_history(items, HandoffMode::Items))
            .expect("the call starts with the built-in routing");
        assert_eq!(started["attempt"], 2, "one refused start, one retry");
        assert!(started["received"].get("codexResponsesAsItems").is_none());
        let line = chat
            .entries()
            .into_iter()
            .map(|entry| entry.text)
            .find(|text| text.contains("answer handoff"))
            .expect("the fallback is in the chat");
        assert!(
            line.contains("conversation items") && line.contains("built-in (thinking)"),
            "{line}"
        );

        let thinking = realtime_start_params("t1", "p", "sdp", None, HandoffMode::Thinking);
        let started = runtime
            .block_on(session.start_with_history(thinking, HandoffMode::Thinking))
            .expect("the built-in routing starts at once");
        assert_eq!(started["attempt"], 3);
        server.kill();
    }

    #[test]
    fn the_call_start_line_names_voice_model_and_handoff() {
        assert_eq!(
            call_start_line(Some("cove"), "default model, reasoning default", HandoffMode::Items),
            "Call starting: voice cove; Codex: default model, reasoning default; answers: conversation items."
        );
        assert!(call_start_line(None, "x", HandoffMode::Thinking)
            .contains("voice server default; Codex: x; answers: built-in (thinking)"));
    }
}
