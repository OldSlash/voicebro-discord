//! OpenCode's text turns for the shared [`TextSession`](crate::assistant::text) (change
//! `claude-and-opencode-engines`, designs D2, D3 and D7): one OpenCode session per guild on
//! the app's server.
//!
//! A turn is the backend's own number, from a question sent with `delivery: "queue"` to the
//! end of OpenCode's execution (`session.execution.succeeded`, `.failed` or
//! `.interrupted`). A question added while it runs (one speaker at a time) is sent with
//! `delivery: "steer"` and joins the running loop; with no execution running it is refused
//! and the session starts the next turn. An interrupt waits for the execution's end before
//! the next question goes out, so a late end event never ends the new turn.

use super::server::{OpencodeListener, OpencodeServer};
use super::OpencodeConfig;
use crate::assistant::text::{TurnBackend, TurnContext, TurnEvent, TurnSink};
use crate::settings::AssistantEngine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use std::time::Duration;

/// How long an interrupt waits for the execution to end.
const INTERRUPT_WAIT: Duration = Duration::from_secs(3);

#[derive(Default)]
struct TurnState {
    /// The turn the session runs now.
    current: Option<String>,
    next: u64,
    /// OpenCode reported an execution that has not ended yet.
    executing: bool,
    /// The text of each assistant message part so far, by (message, ordinal), with the
    /// citation marks dropped, and whether the part is inside a mark now.
    texts: HashMap<(String, u64), (String, bool)>,
    /// The tool of each call seen, by call id: `session.tool.input.started` names it, the
    /// later `session.tool.called` carries only the id and the input.
    tools: HashMap<String, String>,
    /// The model the session was last given, as `provider/model`.
    model: String,
}

/// Drops OpenAI's citation marks from model text, which an OpenAI model sends through
/// OpenCode as `\u{e200}cite\u{e202}turn0search0\u{e201}` (OpenCode 2.3.2 with a ChatGPT
/// sign-in, 2026-11-19), and any other character of Unicode's private use area: the voice
/// would read them out and the posted answer would show them. `inside` carries an open mark
/// from one piece of a stream to the next.
pub fn strip_citations(text: &str, inside: &mut bool) -> String {
    let mut kept = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{e200}' => *inside = true,
            '\u{e201}' => *inside = false,
            _ if *inside => {}
            '\u{e000}'..='\u{f8ff}' => {}
            c => kept.push(c),
        }
    }
    kept
}

/// The (message, ordinal) key of a text event.
fn text_key(data: &Value) -> (String, u64) {
    (
        data.get("assistantMessageID")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        data.get("ordinal").and_then(Value::as_u64).unwrap_or(0),
    )
}

pub struct OpencodeTurns {
    me: Weak<Self>,
    server: Arc<OpencodeServer>,
    session: String,
    config: Arc<RwLock<OpencodeConfig>>,
    sink: OnceLock<Weak<dyn TurnSink>>,
    state: Mutex<TurnState>,
    ended: tokio::sync::Notify,
}

impl OpencodeTurns {
    pub fn new(
        server: Arc<OpencodeServer>,
        session: String,
        model: String,
        config: Arc<RwLock<OpencodeConfig>>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            server,
            session,
            config,
            sink: OnceLock::new(),
            state: Mutex::new(TurnState {
                model,
                ..TurnState::default()
            }),
            ended: tokio::sync::Notify::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TurnState> {
        self.state.lock().expect("opencode turns lock")
    }

    fn send(&self, event: TurnEvent) {
        if let Some(sink) = self.sink.get().and_then(Weak::upgrade) {
            sink.on_turn_event(event);
        }
    }

    fn current(&self) -> Option<String> {
        self.lock().current.clone()
    }

    /// The `{providerID, id}` body of a `provider/model` name.
    pub fn model_body(model: &str) -> Option<Value> {
        let (provider, id) = model.split_once('/')?;
        Some(json!({ "providerID": provider, "id": id }))
    }

    /// A model chosen since the session was made goes to OpenCode before the question.
    async fn apply_model(&self) -> Result<(), String> {
        let wanted = self
            .config
            .read()
            .expect("opencode config lock")
            .model
            .trim()
            .to_owned();
        if wanted.is_empty() || self.lock().model == wanted {
            return Ok(());
        }
        let Some(body) = Self::model_body(&wanted) else {
            return Ok(());
        };
        self.server
            .post(
                &format!("/api/session/{}/model", self.session),
                &json!({ "model": body }),
            )
            .await?;
        self.lock().model = wanted;
        Ok(())
    }

    /// A tool call as a chat line: the web search's query, the fetched page, else the tool's
    /// name.
    fn tool_line(name: &str, data: &Value) -> String {
        let input = data.get("input").unwrap_or(&Value::Null);
        let field = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        match name {
            "websearch" => format!("Web search: {}", field("query")),
            "webfetch" => format!("Web fetch: {}", field("url")),
            other => format!("Tool {other}"),
        }
    }

    /// The tool's name in a tool event, or the one an earlier event of the same call gave.
    fn tool_name(&self, data: &Value) -> String {
        let named = data
            .get("name")
            .or_else(|| data.get("tool"))
            .and_then(Value::as_str);
        let id = data.get("id").and_then(Value::as_str).unwrap_or("");
        let mut state = self.lock();
        match named {
            Some(name) => {
                state.tools.insert(id.to_owned(), name.to_owned());
                name.to_owned()
            }
            None => state.tools.get(id).cloned().unwrap_or_default(),
        }
    }

    /// A tool event of OpenCode 2.3.2: `session.tool.input.started` names the tool,
    /// `session.tool.called` carries the input (a chat line), `.success` the result, and a
    /// failure a line with its message.
    fn tool_event(&self, kind: &str, data: &Value) {
        let name = self.tool_name(data);
        let line = match kind {
            "session.tool.called" => Self::tool_line(&name, data),
            _ if kind.ends_with(".failed") || kind.ends_with(".error") => {
                let error = data.get("error");
                let message = error
                    .and_then(|error| error.get("message"))
                    .or(error)
                    .and_then(Value::as_str)
                    .unwrap_or("no reason given");
                let label = match name.as_str() {
                    "websearch" => "Web search",
                    "webfetch" => "Web fetch",
                    "" => "A tool",
                    other => other,
                };
                format!("{label} failed: {message}")
            }
            _ => return,
        };
        if let Some(turn) = self.current() {
            self.send(TurnEvent::Chat {
                turn,
                kind: "tool",
                text: line,
            });
        }
    }

    fn handle(&self, kind: &str, data: &Value) {
        match kind {
            "session.execution.started" => {
                self.lock().executing = true;
            }
            "session.text.delta" => {
                let delta = data.get("delta").and_then(Value::as_str).unwrap_or("");
                let (turn, delta) = {
                    let mut state = self.lock();
                    let (text, inside) = state.texts.entry(text_key(data)).or_default();
                    let delta = strip_citations(delta, inside);
                    text.push_str(&delta);
                    (state.current.clone(), delta)
                };
                if let Some(turn) = turn.filter(|_| !delta.is_empty()) {
                    self.send(TurnEvent::Delta { turn, text: delta });
                }
            }
            "session.text.ended" => {
                let (turn, kept) = {
                    let mut state = self.lock();
                    let kept = state
                        .texts
                        .remove(&text_key(data))
                        .map(|(text, _)| text)
                        .unwrap_or_default();
                    (state.current.clone(), kept)
                };
                let text = data
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| strip_citations(text, &mut false))
                    .unwrap_or(kept)
                    .trim_end()
                    .to_owned();
                if let Some(turn) = turn.filter(|_| !text.trim().is_empty()) {
                    self.send(TurnEvent::Message {
                        turn: turn.clone(),
                        text: text.clone(),
                    });
                    self.send(TurnEvent::Chat {
                        turn,
                        kind: "agent",
                        text,
                    });
                }
            }
            "session.execution.succeeded"
            | "session.execution.failed"
            | "session.execution.interrupted" => {
                let turn = {
                    let mut state = self.lock();
                    state.executing = false;
                    state.texts.clear();
                    state.tools.clear();
                    state.current.take()
                };
                self.ended.notify_waiters();
                let Some(turn) = turn else {
                    return;
                };
                let (ok, detail) = match kind {
                    "session.execution.succeeded" => (true, "completed".to_owned()),
                    "session.execution.interrupted" => (false, "interrupted".to_owned()),
                    _ => {
                        let message = data
                            .get("error")
                            .and_then(|error| error.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("no reason given");
                        (false, format!("failed: {message}"))
                    }
                };
                self.send(TurnEvent::Completed { turn, ok, detail });
            }
            other if other.starts_with("session.tool.") => self.tool_event(other, data),
            _ => {}
        }
    }
}

#[async_trait::async_trait]
impl TurnBackend for OpencodeTurns {
    fn engine(&self) -> AssistantEngine {
        AssistantEngine::Opencode
    }

    fn bind(&self, sink: Weak<dyn TurnSink>) {
        let _ = self.sink.set(sink);
        self.server
            .listen(&self.session, self.me.clone() as Weak<dyn OpencodeListener>);
    }

    fn conversation_id(&self) -> Option<String> {
        Some(self.session.clone())
    }

    fn ready(&self) -> bool {
        self.server.is_alive()
    }

    async fn start(&self, text: String, context: TurnContext) -> Result<Option<String>, String> {
        self.apply_model().await?;
        let id = {
            let mut state = self.lock();
            state.next += 1;
            let id = format!("{}-{}", self.session, state.next);
            state.current = Some(id.clone());
            id
        };
        let sent = self
            .server
            .post(
                &format!("/api/session/{}/prompt", self.session),
                &json!({ "text": context.prefixed(&text), "delivery": "queue" }),
            )
            .await;
        if let Err(error) = sent {
            let mut state = self.lock();
            if state.current.as_deref() == Some(id.as_str()) {
                state.current = None;
            }
            return Err(error);
        }
        Ok(Some(id))
    }

    async fn add(&self, turn: &str, text: String) -> Result<(), String> {
        {
            let state = self.lock();
            if !state.executing || state.current.as_deref() != Some(turn) {
                return Err("the turn has ended".to_owned());
            }
        }
        self.server
            .post(
                &format!("/api/session/{}/prompt", self.session),
                &json!({ "text": text, "delivery": "steer" }),
            )
            .await
            .map(|_| ())
    }

    async fn interrupt(&self, turn: &str) {
        if self.current().as_deref() != Some(turn) {
            return;
        }
        let ended = self.ended.notified();
        tokio::pin!(ended);
        ended.as_mut().enable();
        let interrupted = self
            .server
            .post(
                &format!("/api/session/{}/interrupt", self.session),
                &json!({}),
            )
            .await
            .ok()
            .and_then(|reply| reply.get("interrupted").and_then(Value::as_bool));
        if interrupted == Some(true) && self.lock().executing {
            let _ = tokio::time::timeout(INTERRUPT_WAIT, ended).await;
        }
        // The turn ended on the engine whatever the event said.
        let mut state = self.lock();
        if state.current.as_deref() == Some(turn) {
            state.current = None;
            state.executing = false;
        }
    }

    fn detach(&self) {
        self.server.unlisten(&self.session);
    }
}

impl OpencodeListener for OpencodeTurns {
    fn on_event(&self, kind: &str, data: &Value) {
        self.handle(kind, data);
    }

    fn on_exit(&self) {
        self.send(TurnEvent::Exited {
            message: "OpenCode stopped; run /join again".to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_name_becomes_the_session_body() {
        assert_eq!(
            OpencodeTurns::model_body("openrouter/anthropic/claude-sonnet-4.5"),
            Some(json!({ "providerID": "openrouter", "id": "anthropic/claude-sonnet-4.5" }))
        );
        assert_eq!(OpencodeTurns::model_body("gpt-5"), None);
        assert_eq!(
            OpencodeTurns::tool_line(
                "websearch",
                &json!({ "id": "call_1", "input": { "query": "London weather" } })
            ),
            "Web search: London weather"
        );
    }

    /// The mark an OpenAI model left in a live answer (2026-11-19), whole and split across
    /// the stream's pieces.
    #[test]
    fn citation_marks_are_dropped() {
        let answer = "Rain is possible later today. \u{e200}cite\u{e202}turn0search0\u{e201}";
        assert_eq!(
            strip_citations(answer, &mut false),
            "Rain is possible later today. "
        );
        let mut inside = false;
        let pieces = [
            "later today. \u{e200}ci",
            "te\u{e202}turn0",
            "search0\u{e201} Bring",
            " an umbrella.",
        ];
        let streamed: String = pieces
            .iter()
            .map(|piece| strip_citations(piece, &mut inside))
            .collect();
        assert_eq!(streamed, "later today.  Bring an umbrella.");
        assert!(!inside);
        assert_eq!(
            strip_citations("7–11 °C, Bangkok ✓", &mut false),
            "7–11 °C, Bangkok ✓"
        );
    }
}
