//! Claude Code as the assistant engine of the Whisper mode (change
//! `claude-and-opencode-engines`, designs D4 and D5). See `docs/ARCHITECTURE.md` 4.5.
//!
//! The app runs the user's own, unmodified `claude.exe` headless: one stream-json process
//! per guild, started at the join's preparation and stopped 0.5 s after the guild's leave.
//! It may only search and fetch the web; the user's Claude Code settings, CLAUDE.md files,
//! hooks and MCP servers are not loaded, and no conversation is written to disk. The app
//! never reads, stores or passes Claude credentials: the sign-in is Claude Code's own, and
//! the app only reads what kind it is (`claude auth status`).

pub mod process;
pub mod turns;

use self::process::{clean_environment, clean_std_environment, hide_window, ClaudeProcess};
use self::turns::ClaudeTurns;
use super::chat_log::ChatLog;
use super::codex::conversation::Conversation;
use super::text::{TextOptions, TextParts, TextSession};
use super::{
    Assistant, AssistantError, AssistantSession, AssistantStatus, Capabilities, ModelInfo,
    PrepareFuture,
};
use crate::audio::AudioPorts;
use crate::events::{AppEvent, EventBus};
use crate::settings::{AssistantMode, Settings, SpeakerLabels};
use crate::status::ConnectionErrors;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;

/// How long after a guild's leave its process stops, unless the bot joins again.
const LEAVE_STOP_DELAY: Duration = Duration::from_millis(500);
/// How long the sign-in read may take.
const AUTH_TIMEOUT: Duration = Duration::from_secs(20);
/// The connection error key of this engine.
const ERROR_SOURCE: &str = "claude";
/// The tools Claude Code may use: the web only. They are both the only tools offered
/// (`--tools`) and allowed in advance (`--allowedTools`): with `--permission-mode dontAsk`
/// a tool that is only offered is denied at its first use ("Permission to use WebSearch has
/// been denied because Claude Code is running in don't ask mode", Claude Code 2.1.317,
/// 2026-11-20).
pub const TOOLS: &str = "WebSearch,WebFetch";

/// Why a join is refused; the message is the Discord reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrepareError {
    #[error("Claude Code is not installed: press Install Claude Code on the VoiceBro Status tab, then run /join again")]
    NotFound,
    #[error("Claude Code is not signed in: press Sign in to Claude Code on the VoiceBro Status tab, then run /join again")]
    SignedOut,
    #[error("Claude Code answers in the Whisper mode only: choose Whisper in VoiceBro's Config tab, then run /join again")]
    NotWhisper,
    #[error("Claude Code did not start: {0}")]
    Start(String),
}

/// What kind of sign-in Claude Code uses (design D5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInKind {
    /// A Claude subscription through claude.ai (Free, Pro, Max, Ultra, Team, Enterprise).
    Subscription,
    ApiKey,
    /// Bedrock, Vertex or Foundry.
    Cloud,
    None,
    /// Not read yet, or not understood.
    #[default]
    Unknown,
}

impl SignInKind {
    /// The value the status carries (`AssistantStatus::sign_in_kind`).
    pub const fn name(self) -> &'static str {
        match self {
            SignInKind::Subscription => "subscription",
            SignInKind::ApiKey => "api_key",
            SignInKind::Cloud => "cloud",
            SignInKind::None => "none",
            SignInKind::Unknown => "unknown",
        }
    }
}

/// Claude Code's sign-in, as `claude auth status` reports it. Never a credential.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ClaudeSignIn {
    pub kind: SignInKind,
    /// `authMethod`: `claude.ai`, `api_key`, …
    pub method: Option<String>,
    /// `apiProvider`: `firstParty`, `bedrock`, `vertex`, `foundry`.
    pub provider: Option<String>,
    /// The subscription's plan as the status names it: "Max", "Pro", …
    pub plan: Option<String>,
    /// The claude.ai account, for the owner link's fingerprint (never stored as it is).
    pub email: Option<String>,
}

impl ClaudeSignIn {
    pub fn signed_in(&self) -> bool {
        matches!(
            self.kind,
            SignInKind::Subscription | SignInKind::ApiKey | SignInKind::Cloud
        )
    }

    /// The line the Status tab shows.
    pub fn summary(&self) -> String {
        match self.kind {
            SignInKind::Subscription => match &self.plan {
                Some(plan) => format!("Claude subscription ({plan})"),
                None => "Claude subscription".to_owned(),
            },
            SignInKind::ApiKey => "API key".to_owned(),
            SignInKind::Cloud => match self.provider.as_deref() {
                Some("bedrock") => "Amazon Bedrock".to_owned(),
                Some("vertex") => "Google Vertex AI".to_owned(),
                Some("foundry") => "Microsoft Foundry".to_owned(),
                Some(other) => other.to_owned(),
                None => "cloud provider".to_owned(),
            },
            SignInKind::None => "not signed in".to_owned(),
            SignInKind::Unknown => "sign-in not known".to_owned(),
        }
    }
}

/// Reads `claude auth status` (JSON by default since Claude Code 2.1).
pub fn parse_auth_status(status: &Value) -> ClaudeSignIn {
    let text = |key: &str| {
        status
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let method = text("authMethod");
    let provider = text("apiProvider");
    let logged_in = status.get("loggedIn").and_then(Value::as_bool);
    let kind = if logged_in == Some(false) {
        SignInKind::None
    } else if provider
        .as_deref()
        .is_some_and(|provider| matches!(provider, "bedrock" | "vertex" | "foundry"))
    {
        SignInKind::Cloud
    } else {
        match method.as_deref() {
            Some("claude.ai") => SignInKind::Subscription,
            Some(method) if method.contains("oauth") => SignInKind::Subscription,
            Some("api_key") | Some("apiKey") => SignInKind::ApiKey,
            _ if logged_in == Some(true) => SignInKind::Unknown,
            _ => SignInKind::Unknown,
        }
    };
    let plan = text("subscriptionType").map(|plan| {
        let mut chars = plan.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => plan,
        }
    });
    ClaudeSignIn {
        kind,
        method,
        provider,
        plan: (kind == SignInKind::Subscription).then_some(plan).flatten(),
        email: text("email"),
    }
}

/// Finds `claude.exe` with this process's environment; see [`find_claude_in`].
pub fn find_claude(configured: &str) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    find_claude_in(
        configured,
        &path_dirs,
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .as_deref(),
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .as_deref(),
    )
}

/// The configured path (only that one when set), then `claude.exe` on the PATH, then
/// winget's links folder, then `%USERPROFILE%\.local\bin`, where Claude Code's own
/// installer puts it.
pub fn find_claude_in(
    configured: &str,
    path_dirs: &[PathBuf],
    local_app_data: Option<&Path>,
    profile: Option<&Path>,
) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return path.is_file().then_some(path);
    }
    if let Some(found) = path_dirs
        .iter()
        .map(|dir| dir.join("claude.exe"))
        .find(|candidate| candidate.is_file())
    {
        return Some(found);
    }
    if let Some(local) = local_app_data {
        let link = local.join("Microsoft/WinGet/Links/claude.exe");
        if link.is_file() {
            return Some(link);
        }
    }
    profile
        .map(|profile| profile.join(".local/bin/claude.exe"))
        .filter(|path| path.is_file())
}

/// The arguments of a guild's process (design D4).
pub fn claude_args(model: &str, instructions_file: Option<&Path>, mcp_file: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        "dontAsk",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--no-session-persistence",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    // `--tools`, `--allowedTools` and `--mcp-config` take several values: each is followed
    // by another flag.
    args.push("--tools".to_owned());
    args.push(TOOLS.to_owned());
    args.push("--allowedTools".to_owned());
    args.push(TOOLS.to_owned());
    args.push("--mcp-config".to_owned());
    args.push(mcp_file.display().to_string());
    let model = model.trim();
    if !model.is_empty() {
        args.push("--model".to_owned());
        args.push(model.to_owned());
    }
    if let Some(file) = instructions_file {
        args.push("--append-system-prompt-file".to_owned());
        args.push(file.display().to_string());
    }
    args
}

/// The models of an `initialize` answer.
pub fn parse_models(response: &Value) -> Vec<ModelInfo> {
    response
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| {
                    let id = model.get("value").and_then(Value::as_str)?.to_owned();
                    let name = model
                        .get("displayName")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_owned();
                    Some(ModelInfo {
                        is_default: id == "default",
                        description: model
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        display_name: name,
                        id,
                        reasoning_efforts: Vec::new(),
                        default_reasoning_effort: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The settings the engine reads, as a snapshot it keeps current.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClaudeConfig {
    pub executable: String,
    pub model: String,
    pub instructions: String,
    pub speaker_labels: SpeakerLabels,
    pub one_speaker_at_a_time: bool,
    pub mode: AssistantMode,
}

impl ClaudeConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            executable: settings.claude.executable.clone(),
            model: settings.claude.model.clone(),
            instructions: settings.assistant.instructions.clone(),
            speaker_labels: settings.assistant.speaker_labels,
            one_speaker_at_a_time: settings.assistant.one_speaker_at_a_time,
            mode: settings.assistant.mode,
        }
    }
}

/// One `claude --version` answer and the file it came from.
#[derive(Clone, PartialEq)]
struct VersionCache {
    path: PathBuf,
    size: u64,
    modified: Option<std::time::SystemTime>,
    version: Option<String>,
}

struct Inner {
    handle: Handle,
    events: EventBus,
    errors: Arc<ConnectionErrors>,
    chat: Arc<ChatLog>,
    workspace: PathBuf,
    config_dir: PathBuf,
    config: Arc<RwLock<ClaudeConfig>>,
    processes: Mutex<HashMap<u64, Arc<ClaudeProcess>>>,
    sessions: Mutex<HashMap<u64, Arc<TextSession>>>,
    conversations: Mutex<HashMap<u64, Arc<Mutex<Conversation>>>>,
    tts: Mutex<Option<crate::tts::manager::TtsManager>>,
    sign_in: Mutex<Option<ClaudeSignIn>>,
    models: Mutex<Vec<ModelInfo>>,
    models_read_at_ms: Mutex<Option<u64>>,
    version: Mutex<Option<VersionCache>>,
    /// Raised by every leave and preparation of a guild, so a stop scheduled by a leave is
    /// cancelled by the next join.
    epochs: Mutex<HashMap<u64, u64>>,
    last_error: Mutex<Option<String>>,
}

#[derive(Clone)]
pub struct ClaudeAssistant {
    inner: Arc<Inner>,
}

impl ClaudeAssistant {
    pub fn new(
        handle: Handle,
        events: EventBus,
        errors: Arc<ConnectionErrors>,
        chat: Arc<ChatLog>,
        data_dir: &Path,
        settings: &Settings,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                handle,
                events,
                errors,
                chat,
                workspace: crate::paths::claude_workspace_dir(data_dir),
                config_dir: crate::paths::claude_config_dir(data_dir),
                config: Arc::new(RwLock::new(ClaudeConfig::from_settings(settings))),
                processes: Mutex::new(HashMap::new()),
                sessions: Mutex::new(HashMap::new()),
                conversations: Mutex::new(HashMap::new()),
                tts: Mutex::new(None),
                sign_in: Mutex::new(None),
                models: Mutex::new(Vec::new()),
                models_read_at_ms: Mutex::new(None),
                version: Mutex::new(None),
                epochs: Mutex::new(HashMap::new()),
                last_error: Mutex::new(None),
            }),
        }
    }

    fn config(&self) -> ClaudeConfig {
        self.inner
            .config
            .read()
            .expect("claude config lock")
            .clone()
    }

    pub fn executable(&self) -> Option<PathBuf> {
        find_claude(&self.config().executable)
    }

    /// `claude --version` of `path`, once per file (by size and time).
    fn executable_version(&self, path: &Path) -> Option<String> {
        let metadata = std::fs::metadata(path).ok();
        let key = VersionCache {
            path: path.to_owned(),
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata.and_then(|m| m.modified().ok()),
            version: None,
        };
        let mut cache = self.inner.version.lock().expect("claude version lock");
        if let Some(cached) = cache.as_ref().filter(|cached| {
            cached.path == key.path && cached.size == key.size && cached.modified == key.modified
        }) {
            return cached.version.clone();
        }
        let mut command = std::process::Command::new(path);
        command.arg("--version");
        clean_std_environment(&mut command);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let version = command
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|text| !text.is_empty());
        *cache = Some(VersionCache {
            version: version.clone(),
            ..key
        });
        version
    }

    /// Reads the sign-in kind (`claude auth status`), keeps it and returns it.
    pub async fn read_sign_in(&self, executable: &Path) -> ClaudeSignIn {
        let mut command = tokio::process::Command::new(executable);
        command
            .args(["auth", "status"])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        clean_environment(&mut command);
        hide_window(&mut command);
        let read = tokio::time::timeout(AUTH_TIMEOUT, command.output()).await;
        let sign_in = match read {
            Ok(Ok(output)) => serde_json::from_slice::<Value>(&output.stdout)
                .map(|status| parse_auth_status(&status))
                .unwrap_or_default(),
            Ok(Err(error)) => {
                log::warn!("Claude Code: could not read the sign-in: {error}");
                ClaudeSignIn::default()
            }
            Err(_) => {
                log::warn!("Claude Code: the sign-in read timed out");
                ClaudeSignIn::default()
            }
        };
        *self.inner.sign_in.lock().expect("claude sign-in lock") = Some(sign_in.clone());
        sign_in
    }

    /// The files the process reads: the empty MCP configuration and the user's instruction
    /// (none when it is empty).
    fn write_files(&self, instructions: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
        let dir = &self.inner.config_dir;
        std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::create_dir_all(&self.inner.workspace))
            .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
        let mcp = dir.join("mcp.json");
        std::fs::write(&mcp, json!({ "mcpServers": {} }).to_string())
            .map_err(|error| format!("could not write {}: {error}", mcp.display()))?;
        let instructions = instructions.trim();
        if instructions.is_empty() {
            return Ok((mcp, None));
        }
        let file = dir.join("instructions.txt");
        std::fs::write(&file, instructions)
            .map_err(|error| format!("could not write {}: {error}", file.display()))?;
        Ok((mcp, Some(file)))
    }

    /// Starts a process with the current settings and waits for its `initialize` answer,
    /// whose models it keeps.
    async fn start_process(&self, executable: &Path) -> Result<Arc<ClaudeProcess>, String> {
        let config = self.config();
        let (mcp, instructions) = self.write_files(&config.instructions)?;
        let args = claude_args(&config.model, instructions.as_deref(), &mcp);
        let process = ClaudeProcess::spawn(
            &self.inner.handle,
            self.inner.events.clone(),
            executable,
            &args,
            &self.inner.workspace,
        )?;
        match process.control(json!({ "subtype": "initialize" })).await {
            Ok(response) => {
                self.keep_models(&response);
                Ok(process)
            }
            Err(error) => {
                process.kill();
                Err(error)
            }
        }
    }

    fn keep_models(&self, response: &Value) {
        let models = parse_models(response);
        if models.is_empty() {
            return;
        }
        *self.inner.models.lock().expect("claude models lock") = models;
        *self
            .inner
            .models_read_at_ms
            .lock()
            .expect("claude models lock") = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or_default(),
        );
    }

    fn publish(&self) {
        self.inner
            .events
            .publish(AppEvent::Assistant(Box::new(self.status())));
    }

    /// Check on the Status tab: the version, the sign-in kind and, when signed in, the
    /// models through a short-lived process (no model call).
    pub async fn check(&self) -> Result<ClaudeSignIn, String> {
        let Some(executable) = self.executable() else {
            self.publish();
            return Err("Claude Code is not installed".to_owned());
        };
        let _ = self.executable_version(&executable);
        let sign_in = self.read_sign_in(&executable).await;
        if sign_in.signed_in() {
            match self.start_process(&executable).await {
                Ok(process) => process.kill(),
                Err(error) => log::warn!("Claude Code: the model list was not read: {error}"),
            }
        }
        self.inner
            .events
            .log(format!("Claude Code: {}", sign_in.summary()));
        self.publish();
        Ok(sign_in)
    }

    /// The sign-in last read, if any.
    pub fn sign_in(&self) -> Option<ClaudeSignIn> {
        self.inner
            .sign_in
            .lock()
            .expect("claude sign-in lock")
            .clone()
    }

    fn conversation(&self, guild: u64) -> Arc<Mutex<Conversation>> {
        Arc::clone(
            self.inner
                .conversations
                .lock()
                .expect("claude conversations lock")
                .entry(guild)
                .or_insert_with(|| Arc::new(Mutex::new(Conversation::new(guild)))),
        )
    }

    fn bump_epoch(&self, guild: u64) -> u64 {
        let mut epochs = self.inner.epochs.lock().expect("claude epochs lock");
        let epoch = epochs.entry(guild).or_default();
        *epoch += 1;
        *epoch
    }

    async fn prepare_guild(&self, guild: u64) -> Result<(), PrepareError> {
        self.bump_epoch(guild);
        if self.config().mode != AssistantMode::Whisper {
            return Err(PrepareError::NotWhisper);
        }
        let executable = self.executable().ok_or(PrepareError::NotFound)?;
        let _ = self.executable_version(&executable);
        let sign_in = self.read_sign_in(&executable).await;
        if !sign_in.signed_in() {
            self.publish();
            return Err(PrepareError::SignedOut);
        }
        let running = self
            .inner
            .processes
            .lock()
            .expect("claude processes lock")
            .get(&guild)
            .filter(|process| process.is_alive())
            .cloned();
        if running.is_none() {
            let process = self
                .start_process(&executable)
                .await
                .map_err(PrepareError::Start)?;
            if let Some(old) = self
                .inner
                .processes
                .lock()
                .expect("claude processes lock")
                .insert(guild, process)
            {
                old.kill();
            }
        }
        *self.inner.last_error.lock().expect("claude error lock") = None;
        self.inner.errors.report(ERROR_SOURCE, None);
        self.publish();
        Ok(())
    }

    /// Stops `guild`'s process after the leave delay, unless the bot joined again.
    fn schedule_stop(&self, guild: u64) {
        let epoch = self.bump_epoch(guild);
        let assistant = self.clone();
        self.inner.handle.spawn(async move {
            tokio::time::sleep(LEAVE_STOP_DELAY).await;
            let current = assistant
                .inner
                .epochs
                .lock()
                .expect("claude epochs lock")
                .get(&guild)
                .copied();
            if current != Some(epoch) {
                return;
            }
            let process = assistant
                .inner
                .processes
                .lock()
                .expect("claude processes lock")
                .remove(&guild);
            if let Some(process) = process {
                process.kill();
                assistant
                    .inner
                    .events
                    .log(format!("Claude Code {guild}: stopped after the leave"));
                assistant.publish();
            }
        });
    }
}

impl Assistant for ClaudeAssistant {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn prepare(&self, guild: u64) -> PrepareFuture<'_> {
        Box::pin(async move {
            self.prepare_guild(guild).await.map_err(|error| {
                let message = error.to_string();
                *self.inner.last_error.lock().expect("claude error lock") = Some(message.clone());
                AssistantError::Unavailable(message)
            })
        })
    }

    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError> {
        let mut sessions = self.inner.sessions.lock().expect("claude sessions lock");
        if sessions.contains_key(&guild) {
            return Err(AssistantError::AlreadyAttached(guild));
        }
        let process = self
            .inner
            .processes
            .lock()
            .expect("claude processes lock")
            .get(&guild)
            .filter(|process| process.is_alive())
            .cloned()
            .ok_or_else(|| {
                AssistantError::Unavailable(
                    "Claude Code stopped before the join finished; run /join again".to_owned(),
                )
            })?;
        let tts = self
            .inner
            .tts
            .lock()
            .expect("claude tts lock")
            .clone()
            .ok_or_else(|| {
                AssistantError::Unavailable("the Whisper mode's voice is not set up".to_owned())
            })?;
        let config = Arc::clone(&self.inner.config);
        let session = TextSession::new(TextParts {
            guild,
            handle: self.inner.handle.clone(),
            backend: ClaudeTurns::new(process),
            options: Arc::new(move || {
                let config = config.read().expect("claude config lock");
                TextOptions {
                    speaker_labels: config.speaker_labels,
                    one_speaker_at_a_time: config.one_speaker_at_a_time,
                }
            }),
            conversation: self.conversation(guild),
            ports,
            chat: Arc::clone(&self.inner.chat),
            events: self.inner.events.clone(),
            errors: Arc::clone(&self.inner.errors),
            tts,
        });
        sessions.insert(guild, Arc::clone(&session));
        Ok(session)
    }

    fn detach(&self, guild: u64) {
        let session = self
            .inner
            .sessions
            .lock()
            .expect("claude sessions lock")
            .remove(&guild);
        if let Some(session) = session {
            session.shutdown();
        }
        self.schedule_stop(guild);
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            models: self
                .inner
                .models
                .lock()
                .expect("claude models lock")
                .clone(),
            signed_in: self.sign_in().is_some_and(|sign_in| sign_in.signed_in()),
            lists_read_at_ms: *self
                .inner
                .models_read_at_ms
                .lock()
                .expect("claude models lock"),
            ..Capabilities::default()
        }
    }

    fn status(&self) -> AssistantStatus {
        let executable = self.executable();
        let version = executable
            .as_deref()
            .and_then(|path| self.executable_version(path));
        let sign_in = self.sign_in();
        let summary = match (&executable, &sign_in) {
            (None, _) => "Claude Code not found".to_owned(),
            (Some(_), Some(sign_in)) => sign_in.summary(),
            (Some(_), None) => "sign-in not read yet: press Check".to_owned(),
        };
        let config = self.config();
        AssistantStatus {
            engine: "claude".to_owned(),
            summary,
            signed_in: sign_in.as_ref().is_some_and(ClaudeSignIn::signed_in),
            account_type: sign_in.as_ref().and_then(|sign_in| sign_in.method.clone()),
            email: sign_in.as_ref().and_then(|sign_in| sign_in.email.clone()),
            plan: sign_in.as_ref().and_then(|sign_in| sign_in.plan.clone()),
            sign_in_kind: Some(
                sign_in
                    .as_ref()
                    .map(|sign_in| sign_in.kind)
                    .unwrap_or_default()
                    .name()
                    .to_owned(),
            ),
            executable: executable.map(|path| path.display().to_string()),
            version,
            running: self
                .inner
                .processes
                .lock()
                .expect("claude processes lock")
                .values()
                .any(|process| process.is_alive()),
            error: self
                .inner
                .last_error
                .lock()
                .expect("claude error lock")
                .clone(),
            model_in_use: Some(config.model.clone()).filter(|model| !model.is_empty()),
            reasoning_effort_in_use: None,
            calls: self
                .inner
                .sessions
                .lock()
                .expect("claude sessions lock")
                .values()
                .map(|session| session.status())
                .collect(),
        }
    }

    fn configure(&self, settings: &Settings) {
        // The chat log is shared by the engines; its entries name the selected one.
        self.inner
            .chat
            .set_engine(settings.assistant.engine.display_name());
        let next = ClaudeConfig::from_settings(settings);
        let mut config = self.inner.config.write().expect("claude config lock");
        if config.model != next.model || config.instructions != next.instructions {
            log::info!("Claude Code: the model or the instruction applies from the next /join");
        }
        *config = next;
    }

    fn stop(&self) {
        let processes =
            std::mem::take(&mut *self.inner.processes.lock().expect("claude processes lock"));
        for process in processes.values() {
            process.kill();
        }
    }

    fn use_tts(&self, tts: crate::tts::manager::TtsManager) {
        *self.inner.tts.lock().expect("claude tts lock") = Some(tts);
    }
}

#[cfg(test)]
mod tests;
