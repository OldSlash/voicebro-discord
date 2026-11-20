//! OpenCode as the assistant engine of the Whisper mode (change
//! `claude-and-opencode-engines`, designs D7 and D8). See `docs/ARCHITECTURE.md` 4.5.
//!
//! The app runs the user's own `opencode.exe` as a local server (`opencode serve`) with a
//! configuration it writes into its own folder: one agent that may only fetch web pages and
//! search the web. One server for the app, started at the first join's preparation and
//! stopped 0.5 s after the last leave; one OpenCode session per guild, deleted at the leave.
//! Providers and their keys are the user's, set up in OpenCode itself: the app never reads
//! them and only lists which are set up.

pub mod server;
pub mod turns;

use self::server::OpencodeServer;
use self::turns::OpencodeTurns;
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;

/// How long after the last leave the server stops, unless the bot joins again.
const LEAVE_STOP_DELAY: Duration = Duration::from_millis(500);
/// The connection error key of this engine.
const ERROR_SOURCE: &str = "opencode";
/// The app's agent in OpenCode's configuration.
pub const AGENT: &str = "voicebro";
/// The web search providers of OpenCode 2.3.x and the variables of their keys, in the
/// order the app prefers them (design D8).
pub const SEARCH_KEYS: [(&str, &str); 5] = [
    ("exa", "EXA_API_KEY"),
    ("tavily", "TAVILY_API_KEY"),
    ("firecrawl", "FIRECRAWL_API_KEY"),
    ("parallel", "PARALLEL_API_KEY"),
    ("tinyfish", "TINYFISH_API_KEY"),
];

/// Why a join is refused; the message is the Discord reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrepareError {
    #[error("OpenCode is not installed: press Install OpenCode on the VoiceBro Status tab, then run /join again")]
    NotFound,
    #[error("OpenCode answers in the Whisper mode only: choose Whisper in VoiceBro's Config tab, then run /join again")]
    NotWhisper,
    #[error("OpenCode did not start: {0}")]
    Start(String),
    #[error("OpenCode offers no model to answer with{0}: set up a provider in OpenCode (opencode auth login in a terminal) or pick a model it offers in VoiceBro's OpenCode card, then run /join again")]
    NoModel(String),
}

/// What Check read from OpenCode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OpencodeCheck {
    /// The integrations (providers) with a sign-in or a key, by id.
    pub providers: Vec<String>,
    /// The enabled models, as `provider/model`.
    pub models: Vec<String>,
    /// OpenCode's default model, as `provider/model`.
    pub default_model: Option<String>,
}

/// Finds `opencode.exe` with this process's environment; see [`find_opencode_in`].
pub fn find_opencode(configured: &str) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    find_opencode_in(
        configured,
        &path_dirs,
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .as_deref(),
        std::env::var_os("APPDATA").map(PathBuf::from).as_deref(),
    )
}

/// The configured path (only that one when set), then `opencode.exe` on the PATH, then
/// winget's links folder, then winget's own folder of the `Anomaly.OpenCode` package (which
/// adds that folder to the user's PATH instead of a link, so a process started before the
/// install does not see it), then the native program inside npm's `opencode-ai` package
/// (never its `opencode.cmd` shim, which runs through node).
pub fn find_opencode_in(
    configured: &str,
    path_dirs: &[PathBuf],
    local_app_data: Option<&Path>,
    app_data: Option<&Path>,
) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return path.is_file().then_some(path);
    }
    if let Some(found) = path_dirs
        .iter()
        .map(|dir| dir.join("opencode.exe"))
        .find(|candidate| candidate.is_file())
    {
        return Some(found);
    }
    if let Some(local) = local_app_data {
        let link = local.join("Microsoft/WinGet/Links/opencode.exe");
        if link.is_file() {
            return Some(link);
        }
        let packages = local.join("Microsoft/WinGet/Packages");
        let mut folders: Vec<PathBuf> = std::fs::read_dir(&packages)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("Anomaly.OpenCode_")
                    })
                    .map(|entry| entry.path().join("opencode.exe"))
                    .collect()
            })
            .unwrap_or_default();
        folders.sort();
        if let Some(found) = folders.into_iter().find(|candidate| candidate.is_file()) {
            return Some(found);
        }
    }
    let app_data = app_data?;
    let package = app_data.join("npm/node_modules/opencode-ai/node_modules");
    ["opencode-windows-x64", "opencode-windows-x64-baseline"]
        .into_iter()
        .map(|folder| package.join(folder).join("bin/opencode.exe"))
        .find(|candidate| candidate.is_file())
}

/// The search provider the configuration names (design D8): the first with a key in
/// `env`, else Exa, which answers without one.
pub fn search_provider(env: impl Fn(&str) -> Option<String>) -> &'static str {
    SEARCH_KEYS
        .iter()
        .find(|(_, key)| env(key).is_some_and(|value| !value.trim().is_empty()))
        .map(|(provider, _)| *provider)
        .unwrap_or("exa")
}

/// The configuration the app gives OpenCode (design D7), in OpenCode 2.x's format.
pub fn config_document(instructions: &str, search: &str) -> Value {
    let mut agent = json!({
        "mode": "primary",
        "description": "VoiceBro's voice assistant: answers questions, may only fetch web pages and search the web",
        "permissions": [
            { "action": "*", "resource": "*", "effect": "deny" },
            { "action": "webfetch", "resource": "*", "effect": "allow" },
            { "action": "websearch", "resource": "*", "effect": "allow" },
        ],
    });
    let instructions = instructions.trim();
    if !instructions.is_empty() {
        agent["system"] = Value::String(instructions.to_owned());
    }
    json!({
        "share": "disabled",
        "update": "disable",
        "default_agent": AGENT,
        "websearch": { "provider": search },
        "agents": { AGENT: agent },
    })
}

/// The variables of the server's process besides its password (design D7).
pub fn server_env(config_file: &Path) -> Vec<(String, String)> {
    vec![
        (
            "OPENCODE_CONFIG".to_owned(),
            config_file.display().to_string(),
        ),
        ("OPENCODE_DISABLE_CLAUDE_CODE".to_owned(), "1".to_owned()),
        ("OPENCODE_DISABLE_PROJECT_CONFIG".to_owned(), "1".to_owned()),
        ("OPENCODE_DISABLE_AUTOUPDATE".to_owned(), "1".to_owned()),
    ]
}

/// `provider/model` of a model object of `GET /api/model` or `/api/model/default`. Its `id`
/// names it (`gpt-6.2-sol-fast`); `modelID` is the provider's model under it, which several
/// entries share (`gpt-6.2-sol`), as OpenCode 2.3.2 answered on 2026-11-19.
pub fn model_name(model: &Value) -> Option<String> {
    let provider = model.get("providerID").and_then(Value::as_str)?;
    let id = model
        .get("id")
        .or_else(|| model.get("modelID"))
        .and_then(Value::as_str)?;
    let id = id.strip_prefix(&format!("{provider}/")).unwrap_or(id);
    Some(format!("{provider}/{id}"))
}

/// The list in a reply: `{data: [...]}` or the array itself.
pub fn list_of(reply: &Value) -> Vec<Value> {
    reply
        .get("data")
        .unwrap_or(reply)
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The route that lists the providers OpenCode can use now: its own (`opencode`, with its
/// free models) and every provider with a sign-in or a key. `GET /api/integration` lists
/// only integrations set up in a configuration file, and answered an empty list with an
/// OpenAI sign-in stored (OpenCode 2.3.2, 2026-11-19).
pub const PROVIDERS_ROUTE: &str = "/api/provider";

/// The provider ids in a `GET /api/provider` reply, leaving out a disabled one. Only the ids
/// are read: an entry also carries the provider's settings and headers.
pub fn connected_providers(reply: &Value) -> Vec<String> {
    let mut providers: Vec<String> = Vec::new();
    for item in list_of(reply) {
        if item.get("activation").and_then(Value::as_str) == Some("disabled") {
            continue;
        }
        let id = item
            .get("id")
            .or_else(|| item.get("integrationID"))
            .and_then(Value::as_str);
        if let Some(id) = id.filter(|id| !providers.iter().any(|known| known == id)) {
            providers.push(id.to_owned());
        }
    }
    providers
}

/// The model names of a `GET /api/model` reply, each once, in OpenCode's order, without a
/// model OpenCode marks as not enabled.
pub fn model_names(reply: &Value) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for model in list_of(reply) {
        if model.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        if let Some(name) = model_name(&model).filter(|name| !names.contains(name)) {
            names.push(name);
        }
    }
    names
}

/// The settings the engine reads, as a snapshot it keeps current.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpencodeConfig {
    pub executable: String,
    pub model: String,
    pub instructions: String,
    pub speaker_labels: SpeakerLabels,
    pub one_speaker_at_a_time: bool,
    pub mode: AssistantMode,
}

impl OpencodeConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            executable: settings.opencode.executable.clone(),
            model: settings.opencode.model.trim().to_owned(),
            instructions: settings.assistant.instructions.clone(),
            speaker_labels: settings.assistant.speaker_labels,
            one_speaker_at_a_time: settings.assistant.one_speaker_at_a_time,
            mode: settings.assistant.mode,
        }
    }
}

/// One `opencode --version` answer and the file it came from.
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
    config: Arc<RwLock<OpencodeConfig>>,
    server: tokio::sync::Mutex<Option<Arc<OpencodeServer>>>,
    /// The running server, for the calls that cannot wait for the async lock.
    current: Mutex<Option<Arc<OpencodeServer>>>,
    /// Each guild's OpenCode session and the model it was made with.
    guild_sessions: Mutex<HashMap<u64, (String, String)>>,
    sessions: Mutex<HashMap<u64, Arc<TextSession>>>,
    conversations: Mutex<HashMap<u64, Arc<Mutex<Conversation>>>>,
    tts: Mutex<Option<crate::tts::manager::TtsManager>>,
    checked: Mutex<Option<OpencodeCheck>>,
    version: Mutex<Option<VersionCache>>,
    /// Raised by every preparation and leave, so a stop a leave scheduled is cancelled by
    /// the next join.
    stop_epoch: AtomicU64,
    last_error: Mutex<Option<String>>,
}

#[derive(Clone)]
pub struct OpencodeAssistant {
    inner: Arc<Inner>,
}

impl OpencodeAssistant {
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
                workspace: crate::paths::opencode_workspace_dir(data_dir),
                config_dir: crate::paths::opencode_config_dir(data_dir),
                config: Arc::new(RwLock::new(OpencodeConfig::from_settings(settings))),
                server: tokio::sync::Mutex::new(None),
                current: Mutex::new(None),
                guild_sessions: Mutex::new(HashMap::new()),
                sessions: Mutex::new(HashMap::new()),
                conversations: Mutex::new(HashMap::new()),
                tts: Mutex::new(None),
                checked: Mutex::new(None),
                version: Mutex::new(None),
                stop_epoch: AtomicU64::new(0),
                last_error: Mutex::new(None),
            }),
        }
    }

    fn config(&self) -> OpencodeConfig {
        self.inner
            .config
            .read()
            .expect("opencode config lock")
            .clone()
    }

    pub fn executable(&self) -> Option<PathBuf> {
        find_opencode(&self.config().executable)
    }

    /// `opencode --version` of `path`, once per file (by size and time).
    fn executable_version(&self, path: &Path) -> Option<String> {
        let metadata = std::fs::metadata(path).ok();
        let key = VersionCache {
            path: path.to_owned(),
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata.and_then(|m| m.modified().ok()),
            version: None,
        };
        let mut cache = self.inner.version.lock().expect("opencode version lock");
        if let Some(cached) = cache.as_ref().filter(|cached| {
            cached.path == key.path && cached.size == key.size && cached.modified == key.modified
        }) {
            return cached.version.clone();
        }
        let mut command = std::process::Command::new(path);
        command.arg("--version");
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

    /// The configuration file, written before each start.
    fn write_config(&self) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.inner.config_dir)
            .and_then(|_| std::fs::create_dir_all(&self.inner.workspace))
            .map_err(|error| format!("could not create the OpenCode folders: {error}"))?;
        let file = self.inner.config_dir.join("opencode.json");
        let search = search_provider(|key| std::env::var(key).ok());
        let document = config_document(&self.config().instructions, search);
        std::fs::write(
            &file,
            serde_json::to_string_pretty(&document).expect("config serializes"),
        )
        .map_err(|error| format!("could not write {}: {error}", file.display()))?;
        Ok(file)
    }

    /// The running server, started when there is none.
    async fn ensure_server(&self) -> Result<Arc<OpencodeServer>, PrepareError> {
        let mut slot = self.inner.server.lock().await;
        if let Some(server) = slot.as_ref().filter(|server| server.is_alive()) {
            return Ok(Arc::clone(server));
        }
        let executable = self.executable().ok_or(PrepareError::NotFound)?;
        let _ = self.executable_version(&executable);
        let config = self.write_config().map_err(PrepareError::Start)?;
        let server = OpencodeServer::start(
            &self.inner.handle,
            &executable,
            &self.inner.workspace,
            &server_env(&config),
        )
        .await
        .map_err(PrepareError::Start)?;
        self.inner.events.log(format!(
            "OpenCode: the server runs on 127.0.0.1:{} ({})",
            server.port,
            server.version.as_deref().unwrap_or("version unknown")
        ));
        // A new server knows none of the old sessions.
        self.inner
            .guild_sessions
            .lock()
            .expect("opencode sessions lock")
            .clear();
        *slot = Some(Arc::clone(&server));
        *self.inner.current.lock().expect("opencode server lock") = Some(Arc::clone(&server));
        Ok(server)
    }

    /// The providers, the models and the default model, from the running server.
    async fn read_lists(&self, server: &OpencodeServer) -> OpencodeCheck {
        let providers = server
            .get(PROVIDERS_ROUTE)
            .await
            .map(|reply| connected_providers(&reply))
            .unwrap_or_default();
        let models = server
            .get("/api/model")
            .await
            .map(|reply| model_names(&reply))
            .unwrap_or_default();
        let default_model = server
            .get("/api/model/default")
            .await
            .ok()
            .and_then(|reply| model_name(reply.get("data").unwrap_or(&reply)));
        let check = OpencodeCheck {
            providers,
            models,
            default_model,
        };
        *self.inner.checked.lock().expect("opencode check lock") = Some(check.clone());
        check
    }

    fn publish(&self) {
        self.inner
            .events
            .publish(AppEvent::Assistant(Box::new(self.status())));
    }

    /// Check on the Status tab: the version, the providers set up and the models, from a
    /// server started for the check when the bot is in no channel (and stopped after it).
    pub async fn check(&self) -> Result<OpencodeCheck, String> {
        if self.executable().is_none() {
            self.publish();
            return Err("OpenCode is not installed".to_owned());
        }
        let server = self
            .ensure_server()
            .await
            .map_err(|error| error.to_string())?;
        let check = self.read_lists(&server).await;
        self.inner.events.log(format!(
            "OpenCode: {}; {} models",
            if check.providers.is_empty() {
                "no provider set up".to_owned()
            } else {
                format!("providers {}", check.providers.join(", "))
            },
            check.models.len()
        ));
        if self
            .inner
            .sessions
            .lock()
            .expect("opencode sessions lock")
            .is_empty()
            && self
                .inner
                .guild_sessions
                .lock()
                .expect("opencode sessions lock")
                .is_empty()
        {
            self.stop_server("Check is done").await;
        }
        self.publish();
        Ok(check)
    }

    pub fn checked(&self) -> Option<OpencodeCheck> {
        self.inner
            .checked
            .lock()
            .expect("opencode check lock")
            .clone()
    }

    async fn stop_server(&self, reason: &str) {
        let server = self.inner.server.lock().await.take();
        *self.inner.current.lock().expect("opencode server lock") = None;
        if let Some(server) = server {
            server.kill();
            self.inner
                .events
                .log(format!("OpenCode: the server stops ({reason})"));
        }
    }

    fn conversation(&self, guild: u64) -> Arc<Mutex<Conversation>> {
        Arc::clone(
            self.inner
                .conversations
                .lock()
                .expect("opencode conversations lock")
                .entry(guild)
                .or_insert_with(|| Arc::new(Mutex::new(Conversation::new(guild)))),
        )
    }

    async fn prepare_guild(&self, guild: u64) -> Result<(), PrepareError> {
        self.inner.stop_epoch.fetch_add(1, Ordering::AcqRel);
        let config = self.config();
        if !config.mode.hears_with_whisper() {
            return Err(PrepareError::NotWhisper);
        }
        let server = self.ensure_server().await?;
        let lists = self.read_lists(&server).await;
        let model = if config.model.is_empty() {
            lists.default_model.clone()
        } else {
            Some(config.model.clone())
        };
        match &model {
            None => return Err(PrepareError::NoModel(String::new())),
            Some(model) if !lists.models.is_empty() && !lists.models.contains(model) => {
                return Err(PrepareError::NoModel(format!(" named {model}")));
            }
            Some(_) if lists.models.is_empty() => {
                return Err(PrepareError::NoModel(String::new()));
            }
            Some(_) => {}
        }
        let known = self
            .inner
            .guild_sessions
            .lock()
            .expect("opencode sessions lock")
            .contains_key(&guild);
        if !known {
            let mut body = json!({
                "title": format!("VoiceBro {guild}"),
                "agent": AGENT,
                "location": { "directory": self.inner.workspace.display().to_string() },
            });
            if !config.model.is_empty() {
                if let Some(model) = OpencodeTurns::model_body(&config.model) {
                    body["model"] = model;
                }
            }
            let reply = server
                .post("/api/session", &body)
                .await
                .map_err(PrepareError::Start)?;
            let id = reply
                .get("data")
                .unwrap_or(&reply)
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| PrepareError::Start("the session has no id".to_owned()))?
                .to_owned();
            self.inner
                .guild_sessions
                .lock()
                .expect("opencode sessions lock")
                .insert(guild, (id, model.unwrap_or_default()));
        }
        *self.inner.last_error.lock().expect("opencode error lock") = None;
        self.inner.errors.report(ERROR_SOURCE, None);
        self.publish();
        Ok(())
    }

    /// Deletes `session` on the server and stops the server after the leave delay when no
    /// guild is left, unless the bot joined again.
    fn leave(&self, session: Option<String>) {
        let epoch = self.inner.stop_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let assistant = self.clone();
        self.inner.handle.spawn(async move {
            let server = assistant
                .inner
                .current
                .lock()
                .expect("opencode server lock")
                .clone();
            if let (Some(server), Some(session)) = (server, session) {
                if let Err(error) = server.delete(&format!("/api/session/{session}")).await {
                    log::warn!("OpenCode: the session {session} was not deleted: {error}");
                }
            }
            tokio::time::sleep(LEAVE_STOP_DELAY).await;
            let idle = assistant
                .inner
                .sessions
                .lock()
                .expect("opencode sessions lock")
                .is_empty();
            if idle && assistant.inner.stop_epoch.load(Ordering::Acquire) == epoch {
                assistant
                    .stop_server("the bot left its last voice channel")
                    .await;
                assistant.publish();
            }
        });
    }
}

impl Assistant for OpencodeAssistant {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn prepare(&self, guild: u64) -> PrepareFuture<'_> {
        Box::pin(async move {
            self.prepare_guild(guild).await.map_err(|error| {
                let message = error.to_string();
                *self.inner.last_error.lock().expect("opencode error lock") = Some(message.clone());
                AssistantError::Unavailable(message)
            })
        })
    }

    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError> {
        let mut sessions = self.inner.sessions.lock().expect("opencode sessions lock");
        if sessions.contains_key(&guild) {
            return Err(AssistantError::AlreadyAttached(guild));
        }
        let unavailable = || {
            AssistantError::Unavailable(
                "OpenCode stopped before the join finished; run /join again".to_owned(),
            )
        };
        let server = self
            .inner
            .current
            .lock()
            .expect("opencode server lock")
            .clone()
            .filter(|server| server.is_alive())
            .ok_or_else(unavailable)?;
        let (session_id, model) = self
            .inner
            .guild_sessions
            .lock()
            .expect("opencode sessions lock")
            .get(&guild)
            .cloned()
            .ok_or_else(unavailable)?;
        let tts = self
            .inner
            .tts
            .lock()
            .expect("opencode tts lock")
            .clone()
            .ok_or_else(|| {
                AssistantError::Unavailable("the Whisper mode's voice is not set up".to_owned())
            })?;
        let config = Arc::clone(&self.inner.config);
        let backend = OpencodeTurns::new(server, session_id, model, Arc::clone(&config));
        let session = TextSession::new(TextParts {
            guild,
            handle: self.inner.handle.clone(),
            backend,
            options: Arc::new(move || {
                let config = config.read().expect("opencode config lock");
                TextOptions {
                    speaker_labels: config.speaker_labels,
                    one_speaker_at_a_time: config.one_speaker_at_a_time,
                    text_answers: !config.mode.speaks_locally(),
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
            .expect("opencode sessions lock")
            .remove(&guild);
        if let Some(session) = session {
            session.shutdown();
        }
        let removed = self
            .inner
            .guild_sessions
            .lock()
            .expect("opencode sessions lock")
            .remove(&guild)
            .map(|(id, _)| id);
        self.leave(removed);
    }

    fn capabilities(&self) -> Capabilities {
        let checked = self.checked();
        Capabilities {
            models: checked
                .as_ref()
                .map(|check| {
                    check
                        .models
                        .iter()
                        .map(|model| ModelInfo {
                            id: model.clone(),
                            display_name: model.clone(),
                            description: String::new(),
                            reasoning_efforts: Vec::new(),
                            default_reasoning_effort: None,
                            is_default: check.default_model.as_deref() == Some(model.as_str()),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            signed_in: checked.is_some_and(|check| !check.providers.is_empty()),
            ..Capabilities::default()
        }
    }

    fn status(&self) -> AssistantStatus {
        let executable = self.executable();
        let version = executable
            .as_deref()
            .and_then(|path| self.executable_version(path));
        let checked = self.checked();
        let summary = match (&executable, &checked) {
            (None, _) => "OpenCode not found".to_owned(),
            (Some(_), None) => "providers not read yet: press Check".to_owned(),
            (Some(_), Some(check)) if check.providers.is_empty() => "no provider set up".to_owned(),
            (Some(_), Some(check)) => format!("providers: {}", check.providers.join(", ")),
        };
        let running = self
            .inner
            .current
            .lock()
            .expect("opencode server lock")
            .as_ref()
            .is_some_and(|server| server.is_alive());
        let config = self.config();
        AssistantStatus {
            engine: "opencode".to_owned(),
            summary,
            signed_in: checked
                .as_ref()
                .is_some_and(|check| !check.providers.is_empty()),
            executable: executable.map(|path| path.display().to_string()),
            version,
            running,
            error: self
                .inner
                .last_error
                .lock()
                .expect("opencode error lock")
                .clone(),
            model_in_use: if config.model.is_empty() {
                checked.and_then(|check| check.default_model)
            } else {
                Some(config.model)
            },
            calls: self
                .inner
                .sessions
                .lock()
                .expect("opencode sessions lock")
                .values()
                .map(|session| session.status())
                .collect(),
            ..AssistantStatus::default()
        }
    }

    fn configure(&self, settings: &Settings) {
        self.inner
            .chat
            .set_engine(settings.assistant.engine.display_name());
        let next = OpencodeConfig::from_settings(settings);
        let mut config = self.inner.config.write().expect("opencode config lock");
        if config.instructions != next.instructions {
            log::info!("OpenCode: the instruction applies from the next start of its server");
        }
        *config = next;
    }

    fn stop(&self) {
        if let Some(server) = self
            .inner
            .current
            .lock()
            .expect("opencode server lock")
            .take()
        {
            server.kill();
        }
    }

    fn use_tts(&self, tts: crate::tts::manager::TtsManager) {
        *self.inner.tts.lock().expect("opencode tts lock") = Some(tts);
    }
}

#[cfg(test)]
mod tests;
