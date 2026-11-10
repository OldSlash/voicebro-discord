//! P0.1: the Codex app-server protocol as installed.
//!
//! `AppServer` is the minimal JSON-RPC-over-stdio client the probes share: one JSON object
//! per line, requests with numeric ids, notifications broadcast to subscribers, and every
//! server-initiated request answered with a decline so Codex can never act on this
//! computer from a probe.

use crate::Outcome;
use anyhow::{anyhow, bail, Context};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Finds the native `codex.exe`: `CODEX_EXECUTABLE`, then PATH, then the npm vendor folder,
/// then the `codex.cmd` shim as a last resort (it runs through node and `kill_on_drop`
/// would only kill the shim).
pub fn find_codex() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("CODEX_EXECUTABLE") {
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

pub fn codex_version(executable: &Path) -> anyhow::Result<String> {
    let output = std::process::Command::new(executable)
        .arg("--version")
        .output()
        .with_context(|| format!("running {} --version", executable.display()))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The automatic answer to a request the app-server sends to its client. Everything that
/// would let Codex act on this computer is declined.
fn server_request_reply(method: &str) -> Option<Value> {
    Some(match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            json!({ "decision": "decline" })
        }
        "item/permissions/requestApproval" => json!({ "permissions": {}, "scope": "turn" }),
        "execCommandApproval" | "applyPatchApproval" => json!({ "decision": "abort" }),
        "mcpServer/elicitation/request" => json!({ "action": "decline" }),
        "item/tool/requestUserInput" => json!({ "answers": {} }),
        "item/tool/call" => json!({ "contentItems": [], "success": false }),
        "currentTime/read" => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            json!({ "currentTimeAt": now })
        }
        _ => return None,
    })
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct AppServer {
    _child: Child,
    writer: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    notifications: broadcast::Sender<(String, Value)>,
    pub stderr_lines: Arc<Mutex<Vec<String>>>,
}

impl AppServer {
    /// Spawns `codex app-server --disable shell_tool` and completes the `initialize`
    /// handshake with `experimentalApi: true`.
    pub async fn spawn(executable: &Path) -> anyhow::Result<Self> {
        let mut child = Command::new(executable)
            .args(["app-server", "--disable", "shell_tool"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {}", executable.display()))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;

        let (writer, mut outgoing) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
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

        let stderr_lines = Arc::new(Mutex::new(Vec::new()));
        {
            let stderr_lines = Arc::clone(&stderr_lines);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(mut store) = stderr_lines.lock() {
                        store.push(line);
                    }
                }
            });
        }

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (notifications, _) = broadcast::channel(256);
        {
            let pending = Arc::clone(&pending);
            let writer = writer.clone();
            let notifications = notifications.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    let id = message.get("id").cloned();
                    let method = message.get("method").and_then(Value::as_str);
                    match (id, method) {
                        (Some(id), Some(method)) => {
                            // A request from the server to its client.
                            let reply = match server_request_reply(method) {
                                Some(result) => json!({ "id": id, "result": result }),
                                None => json!({ "id": id, "error": { "code": -32601,
                                    "message": format!("probe does not support {method}") } }),
                            };
                            let _ = writer.send(reply.to_string());
                            let _ = notifications.send((
                                format!("server-request:{method}"),
                                message.get("params").cloned().unwrap_or(Value::Null),
                            ));
                        }
                        (Some(id), None) => {
                            let Some(id) = id.as_u64() else { continue };
                            let sender = pending.lock().ok().and_then(|mut map| map.remove(&id));
                            if let Some(sender) = sender {
                                let outcome = match message.get("error") {
                                    Some(error) if !error.is_null() => Err(error.to_string()),
                                    _ => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                                };
                                let _ = sender.send(outcome);
                            }
                        }
                        (None, Some(method)) => {
                            let _ = notifications.send((
                                method.to_owned(),
                                message.get("params").cloned().unwrap_or(Value::Null),
                            ));
                        }
                        (None, None) => {}
                    }
                }
                // The process ended: fail every pending request.
                if let Ok(mut map) = pending.lock() {
                    for (_, sender) in map.drain() {
                        let _ = sender.send(Err("app-server exited".to_owned()));
                    }
                }
            });
        }

        let server = Self {
            _child: child,
            writer,
            pending,
            next_id: AtomicU64::new(1),
            notifications,
            stderr_lines,
        };
        server
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "voicebro-probe",
                        "title": "VoiceBro probe",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": { "experimentalApi": true },
                }),
            )
            .await
            .context("initialize")?;
        server.notify("initialized", Value::Null)?;
        Ok(server)
    }

    pub fn notify(&self, method: &str, params: Value) -> anyhow::Result<()> {
        let mut message = json!({ "method": method });
        if !params.is_null() {
            message["params"] = params;
        }
        self.writer
            .send(message.to_string())
            .map_err(|_| anyhow!("app-server is not running"))
    }

    pub async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| anyhow!("pending map poisoned"))?
            .insert(id, sender);
        let message = json!({ "id": id, "method": method, "params": params });
        self.writer
            .send(message.to_string())
            .map_err(|_| anyhow!("app-server is not running"))?;
        match tokio::time::timeout(REQUEST_TIMEOUT, receiver).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(error))) => bail!("{method} failed: {error}"),
            Ok(Err(_)) => bail!("{method}: app-server went away"),
            Err(_) => bail!("{method}: no answer within {REQUEST_TIMEOUT:?}"),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
        self.notifications.subscribe()
    }

    pub fn stderr_tail(&self, count: usize) -> Vec<String> {
        self.stderr_lines
            .lock()
            .map(|lines| lines.iter().rev().take(count).rev().cloned().collect())
            .unwrap_or_default()
    }
}

/// `thread/start` for a disposable, read-only thread that can never act on the computer.
pub fn thread_start_params(workspace: &Path, developer_instructions: &str) -> Value {
    json!({
        "cwd": workspace,
        "sandbox": "read-only",
        "approvalPolicy": "never",
        "ephemeral": true,
        "developerInstructions": developer_instructions,
    })
}

pub fn thread_id(result: &Value) -> anyhow::Result<String> {
    result
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("thread/start returned no thread id: {result}"))
}

/// Creates the empty workspace directory Codex threads use as `cwd`.
pub fn probe_workspace() -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join("voicebro-probe-workspace");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn locate() -> anyhow::Result<(PathBuf, String)> {
    let executable = find_codex()
        .ok_or_else(|| anyhow!("codex not found: put codex.exe on PATH or set CODEX_EXECUTABLE"))?;
    let version = codex_version(&executable)?;
    Ok((executable, version))
}

/// P0.1: dump the schema and check that the realtime types are there with the fields the
/// plan relies on.
pub fn schema(args: &[String]) -> anyhow::Result<Outcome> {
    let out = args
        .first()
        .ok_or_else(|| anyhow!("usage: probe codex-schema <out-dir>"))?;
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let (executable, version) = locate()?;
    let mut details = vec![format!("codex: {} ({version})", executable.display())];
    let status = std::process::Command::new(&executable)
        .args([
            "app-server",
            "generate-json-schema",
            "--experimental",
            "--out",
        ])
        .arg(&out)
        .status()
        .context("generate-json-schema")?;
    if !status.success() {
        details.push(format!("generate-json-schema exited with {status}"));
        return Ok(Outcome::fail(details));
    }
    let v2 = out.join("v2");
    let count = std::fs::read_dir(&v2).map(|dir| dir.count()).unwrap_or(0);
    details.push(format!("v2 types: {count}"));

    let mut pass = true;
    let mut check = |file: &str, needles: &[&str]| {
        let path = v2.join(file);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let missing: Vec<&str> = needles
                    .iter()
                    .copied()
                    .filter(|needle| !text.contains(needle))
                    .collect();
                if missing.is_empty() {
                    details.push(format!("{file}: all expected fields present"));
                } else {
                    pass = false;
                    details.push(format!("{file}: missing {missing:?}"));
                }
            }
            Err(error) => {
                pass = false;
                details.push(format!("{file}: {error}"));
            }
        }
    };
    check(
        "ThreadRealtimeStartParams.json",
        &[
            "\"threadId\"",
            "\"outputModality\"",
            "\"webrtc\"",
            "\"sdp\"",
            "\"v3\"",
            "\"initialItems\"",
            "\"voiceSessionId\"",
            "\"realtimeSessionId\"",
            "\"voice\"",
            "\"prompt\"",
            "\"includeStartupContext\"",
        ],
    );
    check(
        "ThreadRealtimeSdpNotification.json",
        &["\"sdp\"", "\"threadId\""],
    );
    check(
        "ThreadRealtimeStartedNotification.json",
        &["\"realtimeSessionId\""],
    );
    check(
        "ThreadRealtimeTranscriptDoneNotification.json",
        &["\"role\"", "\"text\""],
    );
    check(
        "ThreadStartParams.json",
        &[
            "\"read-only\"",
            "\"never\"",
            "\"ephemeral\"",
            "\"developerInstructions\"",
            "\"model\"",
        ],
    );
    check(
        "ModelListResponse.json",
        &[
            "\"supportedReasoningEfforts\"",
            "\"defaultReasoningEffort\"",
        ],
    );
    check("ThreadRealtimeStopParams.json", &["\"threadId\""]);
    Ok(Outcome { pass, details })
}

/// P0.1 live: handshake, account, models, a read-only thread.
pub fn smoke(_args: &[String]) -> anyhow::Result<Outcome> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (executable, version) = locate()?;
        let mut details = vec![format!("codex: {} ({version})", executable.display())];
        let server = AppServer::spawn(&executable).await?;
        details.push("initialize: ok (experimentalApi)".to_owned());

        let account = server
            .request("account/read", json!({ "refreshToken": false }))
            .await?;
        let signed_in = account.get("account").map(|a| !a.is_null()).unwrap_or(false);
        let requires_auth = account
            .get("requiresOpenaiAuth")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let account_type = account
            .get("account")
            .and_then(|a| a.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("none");
        details.push(format!(
            "account/read: signed_in={signed_in} type={account_type} requiresOpenaiAuth={requires_auth}"
        ));

        let models = server.request("model/list", json!({})).await?;
        let list = models
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| models.get("models").and_then(Value::as_array).cloned())
            .unwrap_or_default();
        details.push(format!("model/list: {} models", list.len()));
        for model in list.iter().take(12) {
            let id = model.get("id").and_then(Value::as_str).unwrap_or("?");
            let default = model
                .get("defaultReasoningEffort")
                .and_then(Value::as_str)
                .unwrap_or("-");
            let efforts: Vec<&str> = model
                .get("supportedReasoningEfforts")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("reasoningEffort").and_then(Value::as_str))
                        .collect()
                })
                .unwrap_or_default();
            let is_default = model.get("isDefault").and_then(Value::as_bool).unwrap_or(false);
            details.push(format!(
                "  {id}{} efforts={efforts:?} default={default}",
                if is_default { " (default)" } else { "" }
            ));
        }

        let voices = server
            .request("thread/realtime/listVoices", json!({}))
            .await?;
        let v1_voices = voices
            .pointer("/voices/v1")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        let v3_voices = voices
            .pointer("/voices/v3")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        details.push(format!(
            "listVoices: v1={v1_voices:?} v3 count={v3_voices} defaultV1={}",
            voices.pointer("/voices/defaultV1").and_then(Value::as_str).unwrap_or("?")
        ));

        let workspace = probe_workspace()?;
        let thread = server
            .request(
                "thread/start",
                thread_start_params(&workspace, "Probe thread. Do nothing."),
            )
            .await?;
        let id = thread_id(&thread)?;
        let policy = thread
            .get("approvalPolicy")
            .map(Value::to_string)
            .unwrap_or_default();
        details.push(format!(
            "thread/start: id={id} approvalPolicy={policy} sandbox={}",
            thread.get("sandbox").map(Value::to_string).unwrap_or_default()
        ));
        drop(server);
        details.push("app-server stopped (kill_on_drop)".to_owned());
        Ok(Outcome::pass(details))
    })
}
