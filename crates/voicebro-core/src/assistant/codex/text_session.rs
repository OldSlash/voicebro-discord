//! Codex's text turns for the Whisper mode (change `whisper-mode`, design D6), as the
//! [`TurnBackend`] of the shared [`crate::assistant::text::TextSession`] (change
//! `claude-and-opencode-engines`, design D2).
//!
//! - A question is `turn/start { threadId, input: [{ type: "text", text }] }` on the guild's
//!   thread. The people in the channel and the guild's last exchanges go as
//!   `additionalContext` (`application` context, which takes no `name`: Codex CLI 0.167.0
//!   refuses one with "additional context name is only supported for untrusted context").
//! - A question added to the running turn is `turn/steer { threadId, expectedTurnId,
//!   input }`; Codex CLI 0.167.0 refuses it with "no active turn to steer" once the turn
//!   has ended.
//! - An interrupt is `turn/interrupt { threadId, turnId }`.
//! - `item/agentMessage/delta`, `item/completed` and `turn/completed` of the thread become
//!   [`TurnEvent`]s.

use super::app_server::{AppServer, ThreadListener};
use crate::assistant::chat_log::agent_item_line;
use crate::assistant::text::{TurnBackend, TurnContext, TurnEvent, TurnSink};
use crate::events::EventBus;
use crate::settings::AssistantEngine;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock, Weak};

/// The guild's Codex thread as a turn backend.
pub struct CodexTurns {
    me: Weak<Self>,
    guild: u64,
    server: Arc<AppServer>,
    /// Empty when the guild has no thread of this server's generation.
    thread_id: String,
    events: EventBus,
    sink: OnceLock<Weak<dyn TurnSink>>,
}

impl CodexTurns {
    pub fn new(
        guild: u64,
        server: Arc<AppServer>,
        thread_id: String,
        events: EventBus,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            guild,
            server,
            thread_id,
            events,
            sink: OnceLock::new(),
        })
    }

    fn send(&self, event: TurnEvent) {
        if let Some(sink) = self.sink.get().and_then(Weak::upgrade) {
            sink.on_turn_event(event);
        }
    }

    /// The turn a notification belongs to: `turnId`, or `turn.id`.
    fn turn_of(params: &Value) -> Option<String> {
        params
            .get("turnId")
            .and_then(Value::as_str)
            .or_else(|| {
                params
                    .get("turn")
                    .and_then(|turn| turn.get("id"))
                    .and_then(Value::as_str)
            })
            .map(str::to_owned)
    }

    /// The `additionalContext` of a turn, when there is any.
    fn additional_context(context: &TurnContext) -> Option<Value> {
        let mut fields = serde_json::Map::new();
        if let Some(channel) = &context.channel {
            fields.insert(
                "voicebro.channel".to_owned(),
                json!({ "kind": "application", "value": channel }),
            );
        }
        if let Some(history) = &context.history {
            fields.insert(
                "voicebro.history".to_owned(),
                json!({ "kind": "application", "value": history }),
            );
        }
        (!fields.is_empty()).then_some(Value::Object(fields))
    }
}

#[async_trait::async_trait]
impl TurnBackend for CodexTurns {
    fn engine(&self) -> AssistantEngine {
        AssistantEngine::Codex
    }

    fn bind(&self, sink: Weak<dyn TurnSink>) {
        let _ = self.sink.set(sink);
        if !self.thread_id.is_empty() {
            self.server
                .map_thread(&self.thread_id, self.me.clone() as Weak<dyn ThreadListener>);
        }
    }

    fn conversation_id(&self) -> Option<String> {
        (!self.thread_id.is_empty()).then(|| self.thread_id.clone())
    }

    fn ready(&self) -> bool {
        !self.thread_id.is_empty() && self.server.is_alive()
    }

    async fn start(&self, text: String, context: TurnContext) -> Result<Option<String>, String> {
        let mut params = json!({
            "threadId": self.thread_id,
            "input": [{ "type": "text", "text": text }],
        });
        if let Some(context) = Self::additional_context(&context) {
            params["additionalContext"] = context;
        }
        let reply = self.server.request("turn/start", params).await?;
        Ok(reply
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    async fn add(&self, turn: &str, text: String) -> Result<(), String> {
        self.server
            .request(
                "turn/steer",
                json!({
                    "threadId": self.thread_id,
                    "expectedTurnId": turn,
                    "input": [{ "type": "text", "text": text }],
                }),
            )
            .await
            .map(|_| ())
    }

    async fn interrupt(&self, turn: &str) {
        let _ = self
            .server
            .request(
                "turn/interrupt",
                json!({ "threadId": self.thread_id, "turnId": turn }),
            )
            .await;
    }

    fn detach(&self) {
        self.server.unmap_thread(&self.thread_id);
    }
}

impl ThreadListener for CodexTurns {
    fn on_notification(&self, method: &str, params: &Value) {
        let Some(turn) = Self::turn_of(params) else {
            return;
        };
        match method {
            "item/agentMessage/delta" => {
                let text = params
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                self.send(TurnEvent::Delta { turn, text });
            }
            "item/completed" => {
                let item = params.get("item").cloned().unwrap_or(Value::Null);
                if item.get("type").and_then(Value::as_str) == Some("agentMessage") {
                    let text = item
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    self.send(TurnEvent::Message {
                        turn: turn.clone(),
                        text,
                    });
                }
                if let Some((kind, text)) = agent_item_line(&item) {
                    self.send(TurnEvent::Chat { turn, kind, text });
                }
            }
            "turn/completed" => {
                let status = params
                    .get("turn")
                    .and_then(|turn| turn.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                let error = params
                    .get("turn")
                    .and_then(|turn| turn.get("error"))
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default();
                self.send(TurnEvent::Completed {
                    turn,
                    ok: status == "completed",
                    detail: format!("{status}{error}"),
                });
            }
            _ => {}
        }
    }

    fn on_declined(&self, method: &str) {
        self.events.log(format!(
            "Codex {}: declined {method} (nothing may act on this computer)",
            self.guild
        ));
    }

    fn on_server_exit(&self) {
        self.send(TurnEvent::Exited {
            message: "Codex app-server exited; run /join again".to_owned(),
        });
    }
}
