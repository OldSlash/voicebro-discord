//! OpenCode's local server (`opencode serve`, the V2 API of OpenCode 2.x) as the app runs it
//! (change `claude-and-opencode-engines`, design D7): listening on 127.0.0.1 on a free port,
//! with a password made for each start (HTTP Basic, user `opencode`), its JSON calls and
//! its event stream (`GET /api/event`), whose events go to the listener of their session.

use crate::assistant::claude::process::hide_window;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

/// How long the server may take to answer `/api/info` after its start.
pub const READY_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a call may take.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// The user of OpenCode's Basic authentication.
pub const USER: &str = "opencode";
/// The session name under which a listener hears every event whole (`{id, type, data}`),
/// with or without a session; the probe uses it to see what the server sends.
pub const EVERY_EVENT: &str = "*";

/// What hears the events of one session.
pub trait OpencodeListener: Send + Sync {
    /// One event (`type`, `data`) of the session.
    fn on_event(&self, kind: &str, data: &Value);
    /// The server stopped.
    fn on_exit(&self);
}

pub struct OpencodeServer {
    pub port: u16,
    pub version: Option<String>,
    password: String,
    base: String,
    client: reqwest::Client,
    alive: Arc<AtomicBool>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
    listeners: Arc<Mutex<HashMap<String, Weak<dyn OpencodeListener>>>>,
}

/// A port nobody listens on now: bound and released at once.
fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("no free port: {error}"))
}

/// A password for one start: 32 random bytes as hex.
fn new_password() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl OpencodeServer {
    /// Starts `executable serve` in `cwd` with `env`, on a free port; tries a second port
    /// when the first start does not become ready.
    pub async fn start(
        handle: &Handle,
        executable: &Path,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Result<Arc<Self>, String> {
        match Self::start_once(handle, executable, cwd, env).await {
            Ok(server) => Ok(server),
            Err(first) => {
                log::warn!("OpenCode: the server did not start ({first}); trying another port");
                Self::start_once(handle, executable, cwd, env).await
            }
        }
    }

    async fn start_once(
        handle: &Handle,
        executable: &Path,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Result<Arc<Self>, String> {
        let port = free_port()?;
        let password = new_password();
        let mut command = Command::new(executable);
        command
            .args([
                "serve",
                "--hostname",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ])
            .current_dir(cwd)
            .env("OPENCODE_PASSWORD", &password)
            .envs(
                env.iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        hide_window(&mut command);
        crate::process_group::own_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", executable.display()))?;
        for stream in [
            child
                .stdout
                .take()
                .map(|out| Box::new(out) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
            child
                .stderr
                .take()
                .map(|err| Box::new(err) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        ]
        .into_iter()
        .flatten()
        {
            handle.spawn(async move {
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // The server prints a password of its own only when none was given.
                    if !line.contains("password") {
                        log::debug!("[opencode] {line}");
                    }
                }
            });
        }
        let alive = Arc::new(AtomicBool::new(true));
        let listeners: Arc<Mutex<HashMap<String, Weak<dyn OpencodeListener>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        {
            let alive = Arc::clone(&alive);
            let listeners = Arc::clone(&listeners);
            handle.spawn(async move {
                tokio::select! {
                    _ = child.wait() => {}
                    _ = kill_rx => {
                        crate::process_group::stop_group(child.id());
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                    }
                }
                alive.store(false, Ordering::Release);
                let listeners: Vec<_> = listeners
                    .lock()
                    .expect("opencode listeners lock")
                    .values()
                    .filter_map(Weak::upgrade)
                    .collect();
                for listener in listeners {
                    listener.on_exit();
                }
                log::info!("OpenCode: the server stopped");
            });
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|error| format!("no HTTP client: {error}"))?;
        let mut server = Self {
            port,
            version: None,
            password,
            base: format!("http://127.0.0.1:{port}"),
            client,
            alive,
            kill: Mutex::new(Some(kill_tx)),
            listeners,
        };
        let started = std::time::Instant::now();
        let info = loop {
            if !server.is_alive() {
                return Err("the server ended right after its start".to_owned());
            }
            match server.get("/api/info").await {
                Ok(info) => break info,
                Err(error) if started.elapsed() > READY_TIMEOUT => {
                    server.kill();
                    return Err(format!("the server did not answer within 20 s: {error}"));
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        };
        server.version = info
            .get("version")
            .or_else(|| info.get("data").and_then(|data| data.get("version")))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let server = Arc::new(server);
        server.read_events(handle);
        Ok(server)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub fn kill(&self) {
        if let Some(kill) = self.kill.lock().expect("opencode kill lock").take() {
            let _ = kill.send(());
        }
    }

    /// Where `session`'s events go from now on.
    pub fn listen(&self, session: &str, listener: Weak<dyn OpencodeListener>) {
        self.listeners
            .lock()
            .expect("opencode listeners lock")
            .insert(session.to_owned(), listener);
    }

    pub fn unlisten(&self, session: &str) {
        self.listeners
            .lock()
            .expect("opencode listeners lock")
            .remove(session);
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base))
            .basic_auth(USER, Some(&self.password))
    }

    async fn send(&self, request: reqwest::RequestBuilder, what: &str) -> Result<Value, String> {
        let response = request
            .timeout(CALL_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("{what}: {error}"))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| format!("{what}: {error}"))?;
        if !status.is_success() {
            let detail = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|body| {
                    body.get("message")
                        .or_else(|| body.get("error").and_then(|error| error.get("message")))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(text);
            return Err(format!("{what}: HTTP {status}: {}", detail.trim()));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|error| format!("{what}: {error}"))
    }

    pub async fn get(&self, path: &str) -> Result<Value, String> {
        self.send(self.request(reqwest::Method::GET, path), path)
            .await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        self.send(
            self.request(reqwest::Method::POST, path)
                .header("content-type", "application/json")
                .body(body.to_string()),
            path,
        )
        .await
    }

    pub async fn delete(&self, path: &str) -> Result<(), String> {
        self.send(self.request(reqwest::Method::DELETE, path), path)
            .await
            .map(|_| ())
    }

    /// Reads `/api/event` while the server runs and hands each event to the listener of its
    /// session; opens the stream again a second after it ends.
    fn read_events(self: &Arc<Self>, handle: &Handle) {
        let weak = Arc::downgrade(self);
        handle.spawn(async move {
            loop {
                let Some(server) = weak.upgrade() else {
                    return;
                };
                if !server.is_alive() {
                    return;
                }
                if let Err(error) = server.read_stream().await {
                    log::debug!("OpenCode: the event stream ended: {error}");
                }
                drop(server);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    async fn read_stream(&self) -> Result<(), String> {
        let mut response = self
            .request(reqwest::Method::GET, "/api/event")
            .header("accept", "text/event-stream")
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }
        let mut buffer = String::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if buffer.contains("\r\n") {
                buffer = buffer.replace("\r\n", "\n");
            }
            while let Some(end) = buffer.find("\n\n") {
                let frame: String = buffer.drain(..end + 2).collect();
                self.dispatch(&frame);
            }
        }
        Ok(())
    }

    /// One server-sent frame: its `data:` lines are one JSON event `{type, data}`.
    fn dispatch(&self, frame: &str) {
        let data: String = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            return;
        }
        let Ok(event) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        let every = self
            .listeners
            .lock()
            .expect("opencode listeners lock")
            .get(EVERY_EVENT)
            .and_then(Weak::upgrade);
        if let Some(every) = every {
            every.on_event(kind, &event);
        }
        let payload = event.get("data").unwrap_or(&Value::Null);
        let Some(session) = payload.get("sessionID").and_then(Value::as_str) else {
            return;
        };
        let listener = self
            .listeners
            .lock()
            .expect("opencode listeners lock")
            .get(session)
            .and_then(Weak::upgrade);
        if let Some(listener) = listener {
            listener.on_event(kind, payload);
        }
    }
}
