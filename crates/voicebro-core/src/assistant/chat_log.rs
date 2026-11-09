//! The assistant chat log: what was heard, said, sent as context, and what the delegated
//! agent did. Kept in memory (bounded), pushed to the UI as an event per entry, and
//! appended to a daily file under the logs directory.

use crate::events::{AppEvent, EventBus};
use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Entries kept in memory.
pub const MAX_ENTRIES: usize = 600;

/// One line of the chat view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatEntry {
    pub id: u64,
    pub guild_id: String,
    /// Local time, HH:MM:SS.
    pub at: String,
    /// user, assistant, context, task, agent, reasoning, tool, system, or error.
    pub kind: &'static str,
    pub who: Option<String>,
    pub text: String,
}

pub struct ChatLog {
    entries: Mutex<VecDeque<ChatEntry>>,
    sequence: AtomicU64,
    events: EventBus,
    dir: PathBuf,
}

impl ChatLog {
    pub fn new(events: EventBus, logs_dir: PathBuf) -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            sequence: AtomicU64::new(1),
            events,
            dir: logs_dir,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> Vec<ChatEntry> {
        self.entries
            .lock()
            .expect("chat lock")
            .iter()
            .cloned()
            .collect()
    }

    /// Adds a line: kept in memory, pushed to the UI, appended to today's file.
    pub fn push(&self, guild: u64, kind: &'static str, who: Option<&str>, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let now = chrono::Local::now();
        let entry = ChatEntry {
            id: self.sequence.fetch_add(1, Ordering::Relaxed),
            guild_id: guild.to_string(),
            at: now.format("%H:%M:%S").to_string(),
            kind,
            who: who.map(str::to_owned),
            text: text.to_owned(),
        };
        {
            let mut entries = self.entries.lock().expect("chat lock");
            entries.push_back(entry.clone());
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        log::info!(
            "[chat:{guild}] {kind}{}: {text}",
            who.map(|w| format!(" {w}")).unwrap_or_default()
        );
        self.events.publish(AppEvent::Chat(Box::new(entry)));
        let speaker = who.map(|who| format!(" {who}:")).unwrap_or_default();
        let line = format!(
            "{} [{guild}] {kind}{speaker} {}\n",
            now.format("%Y-%m-%d %H:%M:%S"),
            text.replace('\n', "\n    ")
        );
        let path = self
            .dir
            .join(format!("codex-chat-{}.log", now.format("%Y-%m-%d")));
        let written = std::fs::create_dir_all(&self.dir).and_then(|_| {
            use std::io::Write as _;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(line.as_bytes()))
        });
        if let Err(error) = written {
            log::warn!("chat log: could not write {}: {error}", path.display());
        }
    }
}

/// The task text of a voice delegation. Codex wraps it as
/// `<realtime_delegation><input>…</input><transcript_delta>…</realtime_delegation>`.
pub fn delegated_task_text(message: &str) -> String {
    let inner = message
        .split_once("<input>")
        .and_then(|(_, rest)| rest.split_once("</input>"))
        .map(|(input, _)| input.trim())
        .filter(|input| !input.is_empty());
    inner.unwrap_or(message.trim()).to_owned()
}

/// One delegated-work item from the Codex agent, as a chat line (kind, text).
pub fn agent_item_line(item: &Value) -> Option<(&'static str, String)> {
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("").trim();
    let joined = |key: &str| {
        item.get(key)
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| {
                        part.as_str()
                            .or_else(|| part.get("text").and_then(Value::as_str))
                    })
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    };
    let status = item
        .get("status")
        .and_then(Value::as_str)
        .map(|status| format!(" ({status})"))
        .unwrap_or_default();
    let line = match item.get("type").and_then(Value::as_str)? {
        "userMessage" => ("task", delegated_task_text(&joined("content"))),
        "agentMessage" => ("agent", text("text").to_owned()),
        "reasoning" => ("reasoning", joined("summary")),
        "plan" => ("agent", format!("Plan: {}", text("text"))),
        "commandExecution" => ("tool", format!("Command: {}{status}", text("command"))),
        "webSearch" => ("tool", format!("Web search: {}", text("query"))),
        "mcpToolCall" => (
            "tool",
            format!("Tool {}/{}{status}", text("server"), text("tool")),
        ),
        "dynamicToolCall" => ("tool", format!("Tool {}{status}", text("tool"))),
        "fileChange" => ("tool", format!("File change{status}")),
        "imageGeneration" => ("tool", format!("Image generation{status}")),
        "collabAgentToolCall" | "subAgentActivity" => ("tool", "Helper agent".to_owned()),
        _ => return None,
    };
    (!line.1.trim().is_empty()).then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn delegated_tasks_are_unwrapped_and_items_become_lines() {
        assert_eq!(
            agent_item_line(&json!({ "type": "userMessage", "id": "1",
                "content": [{ "type": "text", "text": "Find the weather" }] })),
            Some(("task", "Find the weather".to_owned()))
        );
        assert_eq!(
            agent_item_line(&json!({ "type": "userMessage", "id": "1b", "content": [{
                "type": "text",
                "text": "<realtime_delegation>\n  <input>Look up the weather</input>\n  <transcript_delta>user: Look up the weather</transcript_delta>\n</realtime_delegation>"
            }] })),
            Some(("task", "Look up the weather".to_owned()))
        );
        assert_eq!(
            agent_item_line(&json!({ "type": "webSearch", "id": "2", "query": "weather" })),
            Some(("tool", "Web search: weather".to_owned()))
        );
        assert_eq!(
            agent_item_line(&json!({ "type": "reasoning", "id": "3",
                "summary": ["Thinking", ""], "content": [] })),
            Some(("reasoning", "Thinking".to_owned()))
        );
        assert_eq!(
            agent_item_line(&json!({ "type": "reasoning", "id": "4", "summary": [] })),
            None
        );
        assert_eq!(
            agent_item_line(&json!({ "type": "contextCompaction", "id": "5" })),
            None
        );
    }

    #[test]
    fn the_log_is_bounded_published_and_written_to_a_daily_file() {
        let dir = tempfile::tempdir().unwrap();
        let events = EventBus::new(1024);
        let mut receiver = events.subscribe();
        let log = ChatLog::new(events, dir.path().join("logs"));
        log.push(7, "user", Some("Alex"), "  hello  ");
        log.push(7, "system", None, "");
        for index in 0..MAX_ENTRIES + 5 {
            log.push(7, "assistant", None, &format!("line {index}"));
        }
        let entries = log.entries();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(
            entries.last().unwrap().text,
            format!("line {}", MAX_ENTRIES + 4)
        );
        match receiver.try_recv().unwrap() {
            AppEvent::Chat(entry) => {
                assert_eq!(entry.kind, "user");
                assert_eq!(entry.who.as_deref(), Some("Alex"));
                assert_eq!(entry.text, "hello");
            }
            other => panic!("unexpected {other:?}"),
        }
        let file = std::fs::read_dir(dir.path().join("logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let name = file.file_name().to_string_lossy().into_owned();
        assert!(
            name.starts_with("codex-chat-") && name.ends_with(".log"),
            "{name}"
        );
        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("[7] user Alex: hello"));
    }
}
