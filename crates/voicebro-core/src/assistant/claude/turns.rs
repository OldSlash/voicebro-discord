//! Claude Code's text turns for the shared [`TextSession`](crate::assistant::text) (change
//! `claude-and-opencode-engines`, designs D2 to D4).
//!
//! Claude Code has no turn ids: it answers the user lines in the order they came, so the
//! backend numbers the turns it starts and gives every message to the oldest turn without a
//! `result` yet. A `result` ends that turn. A question cannot join a running turn (a line
//! sent meanwhile would only run after it), so `add` is refused and the session starts it
//! as the next turn. The channel facts and the history go before the question as text.

use super::process::{ClaudeListener, ClaudeProcess};
use crate::assistant::text::{TurnBackend, TurnContext, TurnEvent, TurnSink};
use crate::settings::AssistantEngine;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[derive(Default)]
struct Turns {
    /// The turns sent and not yet ended, oldest first: the first is running.
    queue: VecDeque<String>,
    next: u64,
    /// The tool of each call of this process, by its id, so a failed result can name it.
    tools: HashMap<String, String>,
}

pub struct ClaudeTurns {
    me: Weak<Self>,
    process: Arc<ClaudeProcess>,
    sink: OnceLock<Weak<dyn TurnSink>>,
    turns: Mutex<Turns>,
}

impl ClaudeTurns {
    pub fn new(process: Arc<ClaudeProcess>) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            process,
            sink: OnceLock::new(),
            turns: Mutex::new(Turns::default()),
        })
    }

    fn send(&self, event: TurnEvent) {
        if let Some(sink) = self.sink.get().and_then(Weak::upgrade) {
            sink.on_turn_event(event);
        }
    }

    fn running(&self) -> Option<String> {
        self.turns
            .lock()
            .expect("claude turns lock")
            .queue
            .front()
            .cloned()
    }

    /// A tool result Claude Code reports as an error, as a chat line: "Web search failed: …"
    /// with the first line of its message. `names` holds the tool of each call seen.
    fn failed_tool_line(block: &Value, names: &HashMap<String, String>) -> Option<String> {
        if !block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return None;
        }
        let name = block
            .get("tool_use_id")
            .and_then(Value::as_str)
            .and_then(|id| names.get(id))
            .map(String::as_str)
            .unwrap_or("");
        let label = match name {
            "WebSearch" => "Web search",
            "WebFetch" => "Web fetch",
            "" => "A tool",
            other => other,
        };
        let content = block.get("content");
        let text = content
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                content.and_then(Value::as_array).map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            })
            .unwrap_or_default();
        let first = text.lines().next().unwrap_or("").trim();
        let message: String = if first.chars().count() > 200 {
            first.chars().take(200).chain("…".chars()).collect()
        } else {
            first.to_owned()
        };
        Some(format!(
            "{label} failed: {}",
            if message.is_empty() {
                "no reason given"
            } else {
                &message
            }
        ))
    }

    /// A tool use as a chat line.
    fn tool_line(block: &Value) -> String {
        let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
        let input = block.get("input");
        let field = |key: &str| {
            input
                .and_then(|input| input.get(key))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        match name {
            "WebSearch" => format!("Web search: {}", field("query")),
            "WebFetch" => format!("Web fetch: {}", field("url")),
            other => format!("Tool {other}"),
        }
    }

    /// How a turn that did not complete ended, for the chat.
    fn failure(result: &Value) -> String {
        let reason = result
            .get("terminal_reason")
            .and_then(Value::as_str)
            .unwrap_or("");
        if reason.starts_with("aborted") {
            return "interrupted".to_owned();
        }
        let subtype = result
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or("error");
        let text = result
            .get("result")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| format!(": {text}"))
            .unwrap_or_default();
        format!("failed ({subtype}){text}")
    }

    fn handle(&self, message: &Value) {
        match message.get("type").and_then(Value::as_str) {
            Some("stream_event") => {
                let event = message.get("event");
                let delta = event
                    .filter(|event| {
                        event.get("type").and_then(Value::as_str) == Some("content_block_delta")
                    })
                    .and_then(|event| event.get("delta"))
                    .filter(|delta| delta.get("type").and_then(Value::as_str) == Some("text_delta"))
                    .and_then(|delta| delta.get("text"))
                    .and_then(Value::as_str);
                if let (Some(text), Some(turn)) = (delta, self.running()) {
                    self.send(TurnEvent::Delta {
                        turn,
                        text: text.to_owned(),
                    });
                }
            }
            Some("assistant") => {
                let Some(turn) = self.running() else {
                    return;
                };
                let blocks = message
                    .get("message")
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            if text.trim().is_empty() {
                                continue;
                            }
                            self.send(TurnEvent::Message {
                                turn: turn.clone(),
                                text: text.clone(),
                            });
                            self.send(TurnEvent::Chat {
                                turn: turn.clone(),
                                kind: "agent",
                                text,
                            });
                        }
                        Some("tool_use") | Some("server_tool_use") => {
                            if let (Some(id), Some(name)) = (
                                block.get("id").and_then(Value::as_str),
                                block.get("name").and_then(Value::as_str),
                            ) {
                                self.turns
                                    .lock()
                                    .expect("claude turns lock")
                                    .tools
                                    .insert(id.to_owned(), name.to_owned());
                            }
                            self.send(TurnEvent::Chat {
                                turn: turn.clone(),
                                kind: "tool",
                                text: Self::tool_line(&block),
                            });
                        }
                        _ => {}
                    }
                }
            }
            // Claude Code hands each tool's result back as a `user` message.
            Some("user") => {
                let Some(turn) = self.running() else {
                    return;
                };
                let blocks = message
                    .get("message")
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    let line = {
                        let turns = self.turns.lock().expect("claude turns lock");
                        Self::failed_tool_line(&block, &turns.tools)
                    };
                    if let Some(text) = line {
                        self.send(TurnEvent::Chat {
                            turn: turn.clone(),
                            kind: "tool",
                            text,
                        });
                    }
                }
            }
            Some("result") => {
                let Some(turn) = self
                    .turns
                    .lock()
                    .expect("claude turns lock")
                    .queue
                    .pop_front()
                else {
                    return;
                };
                let ok = message.get("subtype").and_then(Value::as_str) == Some("success")
                    && !message
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                let detail = if ok {
                    "completed".to_owned()
                } else {
                    Self::failure(message)
                };
                self.send(TurnEvent::Completed { turn, ok, detail });
            }
            _ => {}
        }
    }
}

#[async_trait::async_trait]
impl TurnBackend for ClaudeTurns {
    fn engine(&self) -> AssistantEngine {
        AssistantEngine::Claude
    }

    fn bind(&self, sink: Weak<dyn TurnSink>) {
        let _ = self.sink.set(sink);
        self.process
            .listen(Some(self.me.clone() as Weak<dyn ClaudeListener>));
    }

    fn conversation_id(&self) -> Option<String> {
        Some(format!("claude-{}", self.process.number))
    }

    fn ready(&self) -> bool {
        self.process.is_alive()
    }

    async fn start(&self, text: String, context: TurnContext) -> Result<Option<String>, String> {
        let id = {
            let mut turns = self.turns.lock().expect("claude turns lock");
            turns.next += 1;
            let id = format!("claude-{}-{}", self.process.number, turns.next);
            turns.queue.push_back(id.clone());
            id
        };
        if let Err(error) = self.process.send_user(&context.prefixed(&text)) {
            self.turns
                .lock()
                .expect("claude turns lock")
                .queue
                .retain(|turn| *turn != id);
            return Err(error);
        }
        Ok(Some(id))
    }

    async fn add(&self, _turn: &str, _text: String) -> Result<(), String> {
        Err("Claude Code answers one question at a time".to_owned())
    }

    async fn interrupt(&self, turn: &str) {
        // Only the running turn can be stopped; one that already ended needs nothing.
        if self.running().as_deref() == Some(turn) {
            let _ = self
                .process
                .control(json!({ "subtype": "interrupt" }))
                .await;
        }
    }

    fn detach(&self) {
        self.process.listen(None);
    }
}

impl ClaudeListener for ClaudeTurns {
    fn on_message(&self, message: &Value) {
        self.handle(message);
    }

    fn on_exit(&self) {
        self.send(TurnEvent::Exited {
            message: "Claude Code stopped; run /join again".to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_that_did_not_complete_says_how() {
        assert_eq!(
            ClaudeTurns::failure(&json!({
                "subtype": "error_during_execution",
                "terminal_reason": "aborted_streaming",
            })),
            "interrupted"
        );
        assert_eq!(
            ClaudeTurns::failure(&json!({
                "subtype": "error_during_execution",
                "result": "rate limited",
            })),
            "failed (error_during_execution): rate limited"
        );
        assert_eq!(
            ClaudeTurns::tool_line(
                &json!({ "name": "WebSearch", "input": { "query": "London weather" } })
            ),
            "Web search: London weather"
        );
    }

    /// The denial Claude Code 2.1.317 gave a live search on 2026-11-20 before the tools were
    /// allowed in advance.
    #[test]
    fn a_failed_tool_is_a_chat_line() {
        let names = HashMap::from([("toolu_1".to_owned(), "WebSearch".to_owned())]);
        let denied = json!({
            "type": "tool_result", "tool_use_id": "toolu_1", "is_error": true,
            "content": "Permission to use WebSearch has been denied because Claude Code is running in don't ask mode. IMPORTANT: You *may* attempt…",
        });
        assert_eq!(
            ClaudeTurns::failed_tool_line(&denied, &names).as_deref(),
            Some("Web search failed: Permission to use WebSearch has been denied because Claude Code is running in don't ask mode. IMPORTANT: You *may* attempt…")
        );
        let ok = json!({ "type": "tool_result", "tool_use_id": "toolu_1", "content": "results" });
        assert_eq!(ClaudeTurns::failed_tool_line(&ok, &names), None);
        let unknown = json!({
            "type": "tool_result", "tool_use_id": "toolu_9", "is_error": true,
            "content": [{ "type": "text", "text": "Fetch failed\nsecond line" }],
        });
        assert_eq!(
            ClaudeTurns::failed_tool_line(&unknown, &names).as_deref(),
            Some("A tool failed: Fetch failed")
        );
    }
}
