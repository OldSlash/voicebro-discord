//! One headless Claude Code process (`claude -p` with stream-json input and output) and its
//! line protocol (change `claude-and-opencode-engines`, design D4).
//!
//! - Every stdout line is one JSON object. `control_response` lines answer the app's own
//!   control requests (`initialize`, `interrupt`) by `request_id`; a `control_request` from
//!   Claude Code is refused (nothing may act on this computer); everything else goes to the
//!   listener, the guild's [`super::turns::ClaudeTurns`].
//! - A question is one `{"type":"user","message":{"role":"user","content":…}}` line on
//!   stdin; Claude Code answers the lines in order.
//! - The process ends when the app kills it or stdin closes; the listener hears of it.

use crate::events::EventBus;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};

/// How long a control request waits for its answer.
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// Numbers the processes, so a conversation id names one process.
static PROCESS_NUMBER: AtomicU64 = AtomicU64::new(1);

/// What hears a process's messages.
pub trait ClaudeListener: Send + Sync {
    /// One message that is not a control response.
    fn on_message(&self, message: &Value);
    /// The process ended.
    fn on_exit(&self);
}

type Pending = Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>;

pub struct ClaudeProcess {
    pub number: u64,
    writer: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_request: AtomicU64,
    alive: AtomicBool,
    listener: Mutex<Option<Weak<dyn ClaudeListener>>>,
    child: Mutex<Option<Child>>,
    events: EventBus,
}

/// Hides the console window of a child process.
pub(crate) fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = &command;
}

/// The environment of a Claude Code child. When the app itself runs inside a Claude Code
/// session (`CLAUDECODE` is set, only in development), that session's variables would
/// route the child through the parent, so every `CLAUDE*`, `ANTHROPIC*` and `OTEL_*`
/// variable is removed; otherwise the environment is passed unchanged, so the user's own
/// API key or cloud-provider variables decide the sign-in as in a terminal.
pub fn clean_environment(command: &mut Command) {
    if std::env::var_os("CLAUDECODE").is_none() {
        return;
    }
    for (name, _) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("CLAUDE")
            || upper.starts_with("ANTHROPIC")
            || upper.starts_with("OTEL_")
        {
            command.env_remove(&name);
        }
    }
}

/// The same for a blocking command (the version and the sign-in reads).
pub fn clean_std_environment(command: &mut std::process::Command) {
    if std::env::var_os("CLAUDECODE").is_none() {
        return;
    }
    for (name, _) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("CLAUDE")
            || upper.starts_with("ANTHROPIC")
            || upper.starts_with("OTEL_")
        {
            command.env_remove(&name);
        }
    }
}

impl ClaudeProcess {
    /// Starts `executable` with `args` in `cwd`.
    pub fn spawn(
        handle: &Handle,
        events: EventBus,
        executable: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<Arc<Self>, String> {
        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        clean_environment(&mut command);
        hide_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", executable.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Claude Code has no stdin".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Claude Code has no stdout".to_owned())?;
        let stderr = child.stderr.take();
        let (writer, mut outgoing) = mpsc::unbounded_channel::<String>();
        let process = Arc::new(Self {
            number: PROCESS_NUMBER.fetch_add(1, Ordering::Relaxed),
            writer,
            pending: Mutex::new(HashMap::new()),
            next_request: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            listener: Mutex::new(None),
            child: Mutex::new(Some(child)),
            events,
        });
        handle.spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = outgoing.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    break;
                }
            }
        });
        let weak = Arc::downgrade(&process);
        handle.spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let Some(process) = weak.upgrade() else {
                    return;
                };
                process.handle_line(&line);
            }
            if let Some(process) = weak.upgrade() {
                process.on_exit();
            }
        });
        if let Some(stderr) = stderr {
            handle.spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    log::debug!("[claude] {line}");
                }
            });
        }
        Ok(process)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Where the process's messages go from now on.
    pub fn listen(&self, listener: Option<Weak<dyn ClaudeListener>>) {
        *self.listener.lock().expect("claude listener lock") = listener;
    }

    fn listener(&self) -> Option<Arc<dyn ClaudeListener>> {
        self.listener
            .lock()
            .expect("claude listener lock")
            .as_ref()
            .and_then(Weak::upgrade)
    }

    fn send(&self, message: Value) -> Result<(), String> {
        if !self.is_alive() {
            return Err("Claude Code is not running".to_owned());
        }
        self.writer
            .send(message.to_string())
            .map_err(|_| "Claude Code is not running".to_owned())
    }

    /// One question, as Claude Code's stream-json input takes it.
    pub fn send_user(&self, text: &str) -> Result<(), String> {
        self.send(json!({
            "type": "user",
            "message": { "role": "user", "content": text },
            "parent_tool_use_id": null,
            "session_id": "",
        }))
    }

    /// A control request (`{"subtype": "initialize"}`, `{"subtype": "interrupt"}`) and its
    /// answer's `response`.
    pub async fn control(&self, request: Value) -> Result<Value, String> {
        let id = format!(
            "voicebro_{}",
            self.next_request.fetch_add(1, Ordering::Relaxed)
        );
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("claude pending lock")
            .insert(id.clone(), sender);
        let subtype = request
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or("request")
            .to_owned();
        if let Err(error) = self.send(json!({
            "type": "control_request",
            "request_id": id,
            "request": request,
        })) {
            self.pending
                .lock()
                .expect("claude pending lock")
                .remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(CONTROL_TIMEOUT, receiver).await {
            Ok(Ok(result)) => result.map_err(|error| format!("{subtype}: {error}")),
            Ok(Err(_)) => Err(format!("{subtype}: Claude Code closed the request")),
            Err(_) => {
                self.pending
                    .lock()
                    .expect("claude pending lock")
                    .remove(&id);
                Err(format!("{subtype} timed out"))
            }
        }
    }

    fn handle_line(&self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match message.get("type").and_then(Value::as_str) {
            Some("control_response") => {
                let response = message.get("response").cloned().unwrap_or(Value::Null);
                let Some(id) = response.get("request_id").and_then(Value::as_str) else {
                    return;
                };
                let sender = self.pending.lock().expect("claude pending lock").remove(id);
                if let Some(sender) = sender {
                    let result = if response.get("subtype").and_then(Value::as_str) == Some("error")
                    {
                        Err(response
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Claude Code refused the request")
                            .to_owned())
                    } else {
                        Ok(response.get("response").cloned().unwrap_or(Value::Null))
                    };
                    let _ = sender.send(result);
                }
            }
            // Claude Code asking the app for something (a permission): refused, nothing may
            // act on this computer.
            Some("control_request") => {
                let id = message.get("request_id").cloned().unwrap_or(Value::Null);
                let subtype = message
                    .get("request")
                    .and_then(|request| request.get("subtype"))
                    .and_then(Value::as_str)
                    .unwrap_or("request")
                    .to_owned();
                let _ = self.send(json!({
                    "type": "control_response",
                    "response": {
                        "subtype": "error",
                        "request_id": id,
                        "error": "VoiceBro allows nothing that acts on this computer",
                    },
                }));
                self.events.log(format!(
                    "Claude Code: declined {subtype} (nothing may act on this computer)"
                ));
            }
            _ => {
                if let Some(listener) = self.listener() {
                    listener.on_message(&message);
                }
            }
        }
    }

    fn on_exit(&self) {
        self.alive.store(false, Ordering::Release);
        let pending = std::mem::take(&mut *self.pending.lock().expect("claude pending lock"));
        for (_, sender) in pending {
            let _ = sender.send(Err("Claude Code exited".to_owned()));
        }
        if let Some(listener) = self.listener() {
            listener.on_exit();
        }
        log::info!("Claude Code process {} stopped", self.number);
    }

    pub fn kill(&self) {
        self.alive.store(false, Ordering::Release);
        if let Some(mut child) = self.child.lock().expect("claude child lock").take() {
            let _ = child.start_kill();
        }
    }
}
