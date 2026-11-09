//! The `codex app-server` child process and its JSON-RPC protocol over stdio: one JSON
//! object per line, requests with numeric ids and a timeout, notifications routed to the
//! session of their thread, and every server-initiated request answered with a decline so
//! Codex can never act on this computer.

use super::session::CodexSession;
use crate::events::EventBus;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Distinguishes app-server processes, since thread ids only live as long as theirs.
static GENERATION: AtomicU64 = AtomicU64::new(1);

/// Finds the native `codex.exe`: the configured path, then PATH, then the npm vendor
/// folder, then the `codex.cmd` shim as a last resort (it runs through node and
/// `kill_on_drop` would only kill the shim).
pub fn find_codex(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return path.is_file().then_some(path);
    }
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(found) = path_dirs
        .iter()
        .map(|dir| dir.join("codex.exe"))
        .find(|candidate| candidate.is_file())
    {
        return Some(found);
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        let native = PathBuf::from(app_data)
            .join("npm/node_modules/@openai/codex/node_modules/@openai/codex-win32-x64")
            .join("vendor/x86_64-pc-windows-msvc/bin/codex.exe");
        if native.is_file() {
            return Some(native);
        }
    }
    path_dirs
        .iter()
        .map(|dir| dir.join("codex.cmd"))
        .find(|candidate| candidate.is_file())
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = &command;
}

pub fn codex_version(executable: &Path) -> Option<String> {
    let mut command = std::process::Command::new(executable);
    command.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The automatic answer to a request the app-server sends to its client. Everything that
/// would let Codex act on this computer is declined.
pub fn server_request_reply(method: &str) -> Result<Value, String> {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            Ok(json!({ "decision": "decline" }))
        }
        "item/permissions/requestApproval" => Ok(json!({ "permissions": {}, "scope": "turn" })),
        "execCommandApproval" | "applyPatchApproval" => Ok(json!({ "decision": "abort" })),
        "mcpServer/elicitation/request" => Ok(json!({ "action": "decline" })),
        "item/tool/requestUserInput" => Ok(json!({ "answers": {} })),
        "item/tool/call" => Ok(json!({ "contentItems": [], "success": false })),
        "currentTime/read" => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            Ok(json!({ "currentTimeAt": now }))
        }
        other => Err(format!("VoiceBro does not support {other}")),
    }
}

/// Whether a server request is one that would act on the computer (and is declined).
pub fn is_action_request(method: &str) -> bool {
    method.contains("pproval") || method.starts_with("item/tool/") || method.contains("elicitation")
}

type Pending = Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>;

pub struct AppServer {
    pub generation: u64,
    pub version: Option<String>,
    handle: Handle,
    events: EventBus,
    writer: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    sessions: Mutex<HashMap<String, Weak<CodexSession>>>,
    alive: AtomicBool,
    pub account_dirty: AtomicBool,
    /// Set when `account/login/completed` arrives, whatever its outcome.
    sign_in_done: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl AppServer {
    /// Spawns `codex app-server --disable shell_tool` and completes the `initialize`
    /// handshake with `experimentalApi: true`.
    pub async fn spawn(
        handle: Handle,
        events: EventBus,
        executable: &Path,
    ) -> Result<Arc<Self>, String> {
        let version = codex_version(executable);
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--disable", "shell_tool"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        hide_window(&mut command);
        let mut child = command.spawn().map_err(|error| {
            format!(
                "could not start {} app-server: {error}",
                executable.display()
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Codex app-server has no stdin".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Codex app-server has no stdout".to_owned())?;
        let stderr = child.stderr.take();
        let (writer, mut outgoing) = mpsc::unbounded_channel::<String>();
        let server = Arc::new(Self {
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            version,
            handle: handle.clone(),
            events,
            writer,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            account_dirty: AtomicBool::new(true),
            sign_in_done: AtomicBool::new(false),
            child: Mutex::new(Some(child)),
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
        let weak = Arc::downgrade(&server);
        handle.spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let Some(server) = weak.upgrade() else {
                    return;
                };
                server.handle_line(&line);
            }
            if let Some(server) = weak.upgrade() {
                server.on_exit();
            }
        });
        if let Some(stderr) = stderr {
            handle.spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    log::debug!("[codex app-server] {line}");
                }
            });
        }

        server
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "voicebro",
                        "title": "VoiceBro",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": { "experimentalApi": true },
                }),
            )
            .await?;
        server.send(json!({ "method": "initialized" }))?;
        Ok(server)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// A ChatGPT sign-in started on this server has reported its end.
    pub fn sign_in_completed(&self) -> bool {
        self.sign_in_done.load(Ordering::Acquire)
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    fn send(&self, message: Value) -> Result<(), String> {
        self.writer
            .send(message.to_string())
            .map_err(|_| "Codex app-server is not running".to_owned())
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        if !self.is_alive() {
            return Err("Codex app-server is not running".to_owned());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(id, sender);
        self.send(json!({ "id": id, "method": method, "params": params }))?;
        match tokio::time::timeout(REQUEST_TIMEOUT, receiver).await {
            Ok(Ok(result)) => result.map_err(|error| format!("{method}: {error}")),
            Ok(Err(_)) => Err(format!("{method}: Codex app-server closed the request")),
            Err(_) => {
                self.pending.lock().expect("pending lock").remove(&id);
                Err(format!("{method} timed out"))
            }
        }
    }

    /// Sends a request whose response is not needed.
    pub fn request_without_reply(&self, method: &str, params: Value) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.send(json!({ "id": id, "method": method, "params": params }));
    }

    /// Routes the notifications of `thread_id` to `session`.
    pub fn map_thread(&self, thread_id: &str, session: Weak<CodexSession>) {
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(thread_id.to_owned(), session);
    }

    pub fn unmap_thread(&self, thread_id: &str) {
        self.sessions
            .lock()
            .expect("sessions lock")
            .remove(thread_id);
    }

    fn session_for(&self, params: &Value) -> Option<Arc<CodexSession>> {
        let thread_id = params.get("threadId").and_then(Value::as_str)?;
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(thread_id)
            .and_then(Weak::upgrade)
    }

    fn handle_line(&self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").filter(|id| !id.is_null()).cloned();
        match (method, id) {
            (Some(method), Some(id)) => {
                let reply = match server_request_reply(method) {
                    Ok(result) => json!({ "id": id, "result": result }),
                    Err(message) => {
                        json!({ "id": id, "error": { "code": -32601, "message": message } })
                    }
                };
                let _ = self.send(reply);
                if is_action_request(method) {
                    let params = message.get("params").cloned().unwrap_or(Value::Null);
                    match self.session_for(&params) {
                        Some(session) => session.on_declined(method),
                        None => self.events.log(format!(
                            "Codex: declined {method} (nothing may act on this computer)"
                        )),
                    }
                }
            }
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else {
                    return;
                };
                let sender = self.pending.lock().expect("pending lock").remove(&id);
                if let Some(sender) = sender {
                    let result = match message.get("error") {
                        Some(error) if !error.is_null() => Err(error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Codex request failed")
                            .to_owned()),
                        _ => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = sender.send(result);
                }
            }
            (Some(method), None) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                self.handle_notification(method, &params);
            }
            (None, None) => {}
        }
    }

    fn handle_notification(&self, method: &str, params: &Value) {
        if method.starts_with("account/") {
            self.account_dirty.store(true, Ordering::Release);
            if method == "account/login/completed" {
                self.sign_in_done.store(true, Ordering::Release);
                let success = params
                    .get("success")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if success {
                    self.events.log("Codex: sign-in completed");
                } else {
                    self.events.log(format!(
                        "Codex: sign-in did not complete: {}",
                        params
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error")
                    ));
                }
            }
            return;
        }
        if let Some(session) = self.session_for(params) {
            session.on_notification(method, params);
        }
    }

    fn on_exit(&self) {
        self.alive.store(false, Ordering::Release);
        let pending = std::mem::take(&mut *self.pending.lock().expect("pending lock"));
        for (_, sender) in pending {
            let _ = sender.send(Err("Codex app-server exited".to_owned()));
        }
        let sessions = self
            .sessions
            .lock()
            .expect("sessions lock")
            .values()
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        for session in sessions {
            session.on_server_exit();
        }
        self.events.log("Codex: app-server stopped");
    }

    pub fn kill(&self) {
        self.alive.store(false, Ordering::Release);
        if let Some(mut child) = self.child.lock().expect("child lock").take() {
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approvals_tool_calls_and_elicitations_are_declined() {
        assert_eq!(
            server_request_reply("item/commandExecution/requestApproval").unwrap(),
            json!({ "decision": "decline" })
        );
        assert_eq!(
            server_request_reply("item/fileChange/requestApproval").unwrap(),
            json!({ "decision": "decline" })
        );
        assert_eq!(
            server_request_reply("item/permissions/requestApproval").unwrap()["scope"],
            json!("turn")
        );
        assert_eq!(
            server_request_reply("execCommandApproval").unwrap(),
            json!({ "decision": "abort" })
        );
        assert_eq!(
            server_request_reply("item/tool/call").unwrap()["success"],
            json!(false)
        );
        assert_eq!(
            server_request_reply("mcpServer/elicitation/request").unwrap()["action"],
            json!("decline")
        );
        assert!(server_request_reply("something/new").is_err());
        assert!(
            server_request_reply("currentTime/read").unwrap()["currentTimeAt"]
                .as_i64()
                .unwrap()
                > 0
        );
        assert!(is_action_request("item/commandExecution/requestApproval"));
        assert!(is_action_request("item/tool/call"));
        assert!(!is_action_request("currentTime/read"));
    }

    #[test]
    fn find_codex_prefers_the_configured_path_only_when_it_exists() {
        assert_eq!(find_codex("Z:/nowhere/codex.exe"), None);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("codex.exe");
        std::fs::write(&file, b"").unwrap();
        assert_eq!(find_codex(&file.to_string_lossy()), Some(file));
    }

    /// Live handshake when `VOICEBRO_CODEX_SMOKE=1`: initialize, account/read, a thread.
    #[test]
    fn app_server_smoke_when_enabled() {
        if std::env::var("VOICEBRO_CODEX_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let executable = find_codex("").expect("codex is installed");
            let server = AppServer::spawn(Handle::current(), EventBus::new(16), &executable)
                .await
                .expect("app-server initializes");
            eprintln!("codex version {:?}", server.version);
            let account = server
                .request("account/read", json!({ "refreshToken": false }))
                .await
                .expect("account/read answers");
            assert!(account.get("requiresOpenaiAuth").is_some());
            let voices = server
                .request("thread/realtime/listVoices", json!({}))
                .await
                .expect("listVoices answers");
            assert!(voices.get("voices").is_some());
            server.kill();
        });
    }
}
