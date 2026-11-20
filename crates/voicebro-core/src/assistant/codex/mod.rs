//! The Codex assistant: ChatGPT voice through `codex app-server`, the realtime call over
//! WebRTC inside the process. See `docs/ARCHITECTURE.md` 4.5.

pub mod account;
pub mod app_server;
pub mod conversation;
pub mod jitter;
pub mod lists;
pub mod pacing;
pub mod playout;
pub mod rtc;
pub mod session;
pub mod text_session;
pub mod thread;

use self::account::{next_cursor, parse_account, parse_models, AccountInfo, Voices};
use self::app_server::{codex_version, find_codex, AppServer};
use self::conversation::Conversation;
use self::session::{CodexSession, SessionParts};
use self::text_session::CodexTurns;
use self::thread::{open_thread, ThreadSettings};
use super::chat_log::ChatLog;
use super::text::{TextOptions, TextParts, TextSession};
use super::{
    Assistant, AssistantError, AssistantSession, AssistantStatus, Capabilities, ModelInfo,
    PrepareFuture,
};
use crate::audio::codec::OpusCodec;
use crate::audio::AudioPorts;
use crate::events::{AppEvent, EventBus};
use crate::settings::{AssistantMode, Settings, SpeakerLabels};
use crate::status::ConnectionErrors;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::Mutex as AsyncMutex;

/// How long the app-server stays up after an on-demand Sign in or Check while the bot is in
/// no voice channel.
const IDLE_STOP_DELAY: Duration = Duration::from_secs(60);
/// After the last guild detaches: long enough for the stop requests of its call to go out.
const LAST_LEAVE_STOP_DELAY: Duration = Duration::from_millis(500);
/// A sign-in that never completes stops holding the app-server after this long.
const SIGN_IN_WAIT_LIMIT: Duration = Duration::from_secs(600);

/// The status and start error when no Codex executable is found.
pub const NOT_FOUND_STATUS: &str =
    "codex not found: press Install Codex on the Status tab, or set the path of codex.exe in Config";

/// Why a join was refused. The message is the reply the caller sees in Discord.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrepareError {
    #[error("Codex CLI is not installed: press Install Codex on the VoiceBro Status tab (or set the path of codex.exe in Config), then run /join again")]
    NotFound,
    #[error("Codex did not start: {0}")]
    Start(String),
    #[error(
        "Codex is not signed in: use Sign in on the VoiceBro Status tab, then run /join again"
    )]
    SignedOut,
    #[error("Codex could not open a thread: {0}")]
    Thread(String),
}

/// The settings the Codex engine reads, as a snapshot it keeps current.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexConfig {
    pub executable: String,
    pub model: String,
    pub reasoning_effort: String,
    pub voice: String,
    pub instructions: String,
    pub speaker_labels: SpeakerLabels,
    /// `codex.handoff_mode`, read when a call starts.
    pub handoff_mode: String,
    /// `diagnostics.opus_codec`, read when a call starts.
    pub opus_codec: OpusCodec,
    /// `assistant.mode`: a realtime call or text turns (change `whisper-mode`), read at
    /// each attach.
    pub mode: AssistantMode,
    /// `assistant.one_speaker_at_a_time` (change `speaker-turns`, D6'): in the Whisper mode
    /// a question joins the running turn or waits for the next one instead of interrupting;
    /// read at each question.
    pub one_speaker_at_a_time: bool,
}

impl CodexConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            executable: settings.codex.executable.clone(),
            model: settings.codex.model.clone(),
            reasoning_effort: settings.codex.reasoning_effort.clone(),
            voice: settings.codex.voice.clone(),
            instructions: settings.assistant.instructions.clone(),
            speaker_labels: settings.assistant.speaker_labels,
            handoff_mode: settings.codex.handoff_mode.clone(),
            opus_codec: settings.diagnostics.codec(),
            mode: settings.assistant.mode,
            one_speaker_at_a_time: settings.assistant.one_speaker_at_a_time,
        }
    }
}

struct Inner {
    handle: Handle,
    events: EventBus,
    errors: Arc<ConnectionErrors>,
    chat: Arc<ChatLog>,
    workspace: PathBuf,
    config: Arc<RwLock<CodexConfig>>,
    server: AsyncMutex<Option<Arc<AppServer>>>,
    sessions: Mutex<HashMap<u64, Attached>>,
    /// The local voice the text sessions of the Whisper mode speak with.
    tts: Mutex<Option<crate::tts::manager::TtsManager>>,
    /// Survives leaving and rejoining voice; lives as long as the app runs.
    conversations: Mutex<HashMap<u64, Arc<Mutex<Conversation>>>>,
    /// What each guild's calls learnt about the server's lateness; the next call starts from
    /// it. Lives as long as the app runs.
    learned_depths: Mutex<HashMap<u64, Arc<Mutex<Option<playout::DepthEstimator>>>>>,
    account: Arc<Mutex<Option<AccountInfo>>>,
    voices: Arc<Mutex<Option<Voices>>>,
    models: Mutex<Vec<ModelInfo>>,
    /// `codex-lists.json`: the models and voices of the last read, loaded at start and
    /// written after every successful `model/list`.
    lists_path: PathBuf,
    /// When the models held were read from Codex (Unix ms); from the file until a read in
    /// this run replaces it.
    lists_read_at_ms: Mutex<Option<u64>>,
    /// The models were read from Codex in this run; until then a join reads them again.
    lists_fresh: AtomicBool,
    /// The file held at least one model at start: the read at start then depends on the
    /// setting only.
    lists_saved_at_start: bool,
    /// What loading the file found, logged by `read_lists_at_start` once the log is up.
    lists_load_note: Mutex<Option<(log::Level, String)>>,
    last_error: Mutex<Option<String>>,
    /// Joins being prepared right now; the app-server is not stopped under them.
    preparing: AtomicUsize,
    /// Bumped by every scheduled stop and every preparation; a stop timer that finds a
    /// newer value does nothing.
    stop_epoch: AtomicU64,
    /// When the last ChatGPT sign-in was started, until its server reports completion.
    sign_in_started: Mutex<Option<Instant>>,
    /// The version of the executable found last, keyed by its path, size and modification
    /// time, so a status read starts no `codex --version` unless the file changed.
    version_cache: Mutex<Option<VersionCache>>,
}

/// A guild's session: a realtime call or the Whisper mode's text turns.
#[derive(Clone)]
enum Attached {
    Realtime(Arc<CodexSession>),
    Text(Arc<TextSession>),
}

impl Attached {
    fn shutdown(&self) {
        match self {
            Self::Realtime(session) => session.shutdown(),
            Self::Text(session) => session.shutdown(),
        }
    }

    fn status(&self) -> super::CallStatus {
        match self {
            Self::Realtime(session) => session.status(),
            Self::Text(session) => session.status(),
        }
    }
}

/// One `codex --version` answer and the file it came from.
#[derive(Clone, PartialEq)]
struct VersionCache {
    path: PathBuf,
    size: u64,
    modified: Option<std::time::SystemTime>,
    version: Option<String>,
}

#[derive(Clone)]
pub struct CodexAssistant {
    inner: Arc<Inner>,
}

impl CodexAssistant {
    pub fn new(
        handle: Handle,
        events: EventBus,
        errors: Arc<ConnectionErrors>,
        data_dir: &std::path::Path,
        settings: &Settings,
    ) -> Self {
        // This runs before the desktop shell's log is up: what the load found is logged by
        // `read_lists_at_start`.
        let lists_path = crate::paths::codex_lists_path(data_dir);
        let (saved, load_note) = match lists::load(&lists_path) {
            Ok(Some(saved)) if !saved.models.is_empty() => {
                let note = format!(
                    "Codex lists: {} models and {} voices saved from the read of {}",
                    saved.models.len(),
                    saved.voices.len(),
                    chrono::DateTime::from_timestamp_millis(saved.read_at_ms as i64)
                        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                        .unwrap_or_else(|| saved.read_at_ms.to_string())
                );
                (Some(saved), Some((log::Level::Info, note)))
            }
            Ok(_) => (None, None),
            Err(message) => (None, Some((log::Level::Warn, message))),
        };
        Self {
            inner: Arc::new(Inner {
                handle,
                chat: Arc::new(ChatLog::new(
                    events.clone(),
                    crate::paths::logs_dir(data_dir),
                )),
                events,
                errors,
                workspace: crate::paths::codex_workspace_dir(data_dir),
                config: Arc::new(RwLock::new(CodexConfig::from_settings(settings))),
                server: AsyncMutex::new(None),
                sessions: Mutex::new(HashMap::new()),
                tts: Mutex::new(None),
                conversations: Mutex::new(HashMap::new()),
                learned_depths: Mutex::new(HashMap::new()),
                account: Arc::new(Mutex::new(None)),
                voices: Arc::new(Mutex::new(saved.as_ref().and_then(|saved| saved.voices()))),
                models: Mutex::new(
                    saved
                        .as_ref()
                        .map(|saved| saved.models.clone())
                        .unwrap_or_default(),
                ),
                lists_path,
                lists_read_at_ms: Mutex::new(saved.as_ref().map(|saved| saved.read_at_ms)),
                lists_fresh: AtomicBool::new(false),
                lists_saved_at_start: saved.is_some(),
                lists_load_note: Mutex::new(load_note),
                last_error: Mutex::new(None),
                preparing: AtomicUsize::new(0),
                stop_epoch: AtomicU64::new(0),
                sign_in_started: Mutex::new(None),
                version_cache: Mutex::new(None),
            }),
        }
    }

    /// The version of `path`, from the cache while the file is the same one, otherwise
    /// from one `codex --version` run.
    fn executable_version(&self, path: &std::path::Path) -> Option<String> {
        let metadata = std::fs::metadata(path).ok()?;
        let (size, modified) = (metadata.len(), metadata.modified().ok());
        let mut cache = self.inner.version_cache.lock().expect("version cache lock");
        if let Some(cached) = cache.as_ref() {
            if cached.path == path && cached.size == size && cached.modified == modified {
                return cached.version.clone();
            }
        }
        let version = codex_version(path);
        *cache = Some(VersionCache {
            path: path.to_path_buf(),
            size,
            modified,
            version: version.clone(),
        });
        version
    }

    /// The app-server if it is running; never starts it.
    fn running_server(&self) -> Option<Arc<AppServer>> {
        self.inner
            .server
            .try_lock()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
            .filter(|server| server.is_alive())
    }

    fn attached_guilds(&self) -> usize {
        self.inner.sessions.lock().expect("sessions lock").len()
    }

    /// Starts the app-server if needed, checks the account and opens or reuses the guild's
    /// thread, so the first activation only places the call.
    pub async fn prepare_guild(&self, guild: u64) -> Result<(), PrepareError> {
        self.inner.preparing.fetch_add(1, Ordering::AcqRel);
        // A pending idle stop no longer applies.
        self.inner.stop_epoch.fetch_add(1, Ordering::AcqRel);
        let result = self.prepare_inner(guild).await;
        self.inner.preparing.fetch_sub(1, Ordering::AcqRel);
        match &result {
            Ok(()) => self.set_last_error(None),
            Err(error) => {
                self.set_last_error(Some(error.to_string()));
                if self.attached_guilds() == 0 {
                    self.schedule_stop(IDLE_STOP_DELAY, "the join was refused");
                }
            }
        }
        self.publish_status();
        result
    }

    async fn prepare_inner(&self, guild: u64) -> Result<(), PrepareError> {
        if find_codex(&self.config().executable).is_none() {
            return Err(PrepareError::NotFound);
        }
        let server = self.server().await.map_err(PrepareError::Start)?;
        let account = self
            .refresh_account(&server)
            .await
            .map_err(PrepareError::Start)?;
        if !account.signed_in {
            return Err(PrepareError::SignedOut);
        }
        // The first join of a run reads the lists again even when the saved ones were loaded;
        // later joins reuse what this run read.
        let fresh = self.inner.lists_fresh.load(Ordering::Acquire)
            && !self.inner.models.lock().expect("models lock").is_empty();
        self.refresh_voices(&server, !fresh).await;
        if !fresh {
            if let Err(error) = self.refresh_models(&server).await {
                log::warn!("Codex: model/list failed: {error}");
            }
        }
        let config = self.config();
        let settings = ThreadSettings {
            instructions: config.instructions.clone(),
            model: config.model.trim().to_owned(),
            effort: config.reasoning_effort.trim().to_owned(),
        };
        let conversation = self.conversation(guild);
        let reusable = conversation
            .lock()
            .expect("conversation lock")
            .thread
            .as_ref()
            .is_some_and(|thread| {
                thread.generation == server.generation && thread.requested == settings
            });
        if !reusable {
            let thread = open_thread(
                &server,
                &self.inner.events,
                &self.inner.workspace,
                &settings,
            )
            .await
            .map_err(PrepareError::Thread)?;
            let description = ThreadSettings {
                instructions: String::new(),
                model: thread.model.clone(),
                effort: thread.effort.clone(),
            }
            .describe();
            conversation.lock().expect("conversation lock").thread = Some(thread);
            self.inner.chat.push(
                guild,
                "system",
                None,
                &format!("New Codex thread: {description}."),
            );
        }
        self.inner
            .events
            .log(format!("Codex: ready for guild {guild}"));
        Ok(())
    }

    /// Stops the app-server after `delay` unless by then a guild is attached, a join is
    /// being prepared, a sign-in is waiting for its completion, or a newer stop or
    /// preparation replaced this one.
    fn schedule_stop(&self, delay: Duration, reason: &'static str) {
        let epoch = self.inner.stop_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let assistant = self.clone();
        self.inner.handle.spawn(async move {
            let mut wait = delay;
            loop {
                tokio::time::sleep(wait).await;
                if assistant.inner.stop_epoch.load(Ordering::Acquire) != epoch {
                    return;
                }
                if !assistant.sign_in_waiting() {
                    break;
                }
                wait = IDLE_STOP_DELAY;
            }
            assistant.stop_server_if_idle(reason).await;
        });
    }

    /// A sign-in was started on the running server and has not completed yet.
    fn sign_in_waiting(&self) -> bool {
        let started = *self.inner.sign_in_started.lock().expect("sign-in lock");
        let Some(started) = started else {
            return false;
        };
        let completed = self
            .running_server()
            .is_none_or(|server| server.sign_in_completed());
        !completed && started.elapsed() < SIGN_IN_WAIT_LIMIT
    }

    async fn stop_server_if_idle(&self, reason: &str) {
        let stopped = {
            let mut slot = self.inner.server.lock().await;
            if self.attached_guilds() > 0 || self.inner.preparing.load(Ordering::Acquire) > 0 {
                return;
            }
            slot.take()
        };
        *self.inner.sign_in_started.lock().expect("sign-in lock") = None;
        if let Some(server) = stopped {
            server.kill();
            // Thread ids die with the process; the conversations keep the history, and the
            // next join opens a new thread that continues them.
            self.inner
                .events
                .log(format!("Codex: stopping the app-server ({reason})"));
            self.publish_status();
        }
    }

    /// After an on-demand Sign in or Check: stop again if no guild needs the server.
    fn after_on_demand_use(&self) {
        if self.attached_guilds() == 0 {
            self.schedule_stop(IDLE_STOP_DELAY, "not needed outside a voice channel");
        }
    }

    pub fn chat_log(&self) -> &ChatLog {
        &self.inner.chat
    }

    /// The chat log, shared with the other engines (change `claude-and-opencode-engines`):
    /// the Chat tab shows one log whichever engine answers.
    pub fn chat(&self) -> Arc<ChatLog> {
        Arc::clone(&self.inner.chat)
    }

    fn config(&self) -> CodexConfig {
        self.inner.config.read().expect("codex config lock").clone()
    }

    fn publish_status(&self) {
        self.inner
            .events
            .publish(AppEvent::Assistant(Box::new(self.status())));
    }

    fn set_last_error(&self, error: Option<String>) {
        *self.inner.last_error.lock().expect("last error lock") = error.clone();
        self.inner.errors.report("codex", error);
    }

    /// The running app-server, started if needed.
    async fn server(&self) -> Result<Arc<AppServer>, String> {
        let mut slot = self.inner.server.lock().await;
        if let Some(server) = slot.as_ref().filter(|server| server.is_alive()) {
            return Ok(Arc::clone(server));
        }
        let executable =
            find_codex(&self.config().executable).ok_or_else(|| NOT_FOUND_STATUS.to_owned())?;
        let server = AppServer::spawn(
            self.inner.handle.clone(),
            self.inner.events.clone(),
            &executable,
        )
        .await?;
        self.inner.events.log(format!(
            "Codex: app-server started ({}, {})",
            executable.display(),
            server.version.as_deref().unwrap_or("version unknown")
        ));
        *slot = Some(Arc::clone(&server));
        Ok(server)
    }

    async fn refresh_account(&self, server: &AppServer) -> Result<AccountInfo, String> {
        let result = server
            .request("account/read", json!({ "refreshToken": false }))
            .await?;
        let account = parse_account(&result);
        server
            .account_dirty
            .store(false, std::sync::atomic::Ordering::Release);
        *self.inner.account.lock().expect("account lock") = Some(account.clone());
        Ok(account)
    }

    /// Reads the voices when none are held, or always with `force` (Check, Refresh lists,
    /// the read at start, the first join of a run). A failed read keeps what is held.
    async fn refresh_voices(&self, server: &AppServer, force: bool) {
        if !force && self.inner.voices.lock().expect("voices lock").is_some() {
            return;
        }
        match server
            .request("thread/realtime/listVoices", json!({}))
            .await
        {
            Ok(result) => {
                let voices = Voices::parse(&result);
                log::info!(
                    "Codex voices: {} (default {})",
                    voices.all.join(", "),
                    voices.default.as_deref().unwrap_or("none")
                );
                *self.inner.voices.lock().expect("voices lock") = Some(voices);
            }
            Err(error) => log::warn!("Codex: listVoices failed: {error}"),
        }
    }

    async fn refresh_models(&self, server: &AppServer) -> Result<Vec<ModelInfo>, String> {
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let mut params = json!({ "includeHidden": false });
            if let Some(cursor) = cursor.as_ref() {
                params["cursor"] = json!(cursor);
            }
            let result: Value = server.request("model/list", params).await?;
            models.extend(parse_models(&result));
            cursor = next_cursor(&result);
            if cursor.is_none() {
                break;
            }
        }
        // An empty answer replaces nothing: the lists held and the file stay, and the next
        // join reads again.
        if models.is_empty() {
            log::warn!("Codex: model/list returned no models; the lists held are kept");
            return Ok(models);
        }
        *self.inner.models.lock().expect("models lock") = models.clone();
        self.save_lists(server, &models);
        Ok(models)
    }

    /// After a successful `model/list`: the lists count as read in this run, and the file
    /// gets them with the voices held now. A failed write is logged; the lists stay in use.
    fn save_lists(&self, server: &AppServer, models: &[ModelInfo]) {
        let read_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or_default();
        *self
            .inner
            .lists_read_at_ms
            .lock()
            .expect("lists read time lock") = Some(read_at);
        self.inner.lists_fresh.store(true, Ordering::Release);
        let voices = self.inner.voices.lock().expect("voices lock").clone();
        let saved = lists::SavedLists::new(
            read_at,
            server.version.clone(),
            models.to_vec(),
            voices.as_ref(),
        );
        match lists::save(&self.inner.lists_path, &saved) {
            Ok(()) => log::info!(
                "Codex lists: {} models and {} voices saved to {}",
                saved.models.len(),
                saved.voices.len(),
                self.inner.lists_path.display()
            ),
            Err(error) => log::warn!(
                "Codex lists: could not write {}: {error}",
                self.inner.lists_path.display()
            ),
        }
    }

    /// Starts the app-server if needed and reads the account, voices and models. Outside a
    /// voice channel the server stops again a minute later.
    pub async fn check(&self) -> Result<AccountInfo, String> {
        self.after_on_demand_use();
        let result = async {
            let server = self.server().await?;
            let account = self.refresh_account(&server).await?;
            self.refresh_voices(&server, true).await;
            if let Err(error) = self.refresh_models(&server).await {
                log::warn!("Codex: model/list failed: {error}");
            }
            Ok(account)
        }
        .await;
        self.set_last_error(result.as_ref().err().cloned());
        match &result {
            Ok(account) if account.signed_in => self.inner.events.log(format!(
                "Codex: signed in ({}{})",
                account.kind.as_deref().unwrap_or("account"),
                account
                    .plan
                    .as_ref()
                    .map(|plan| format!(", {plan}"))
                    .unwrap_or_default()
            )),
            Ok(_) => self
                .inner
                .events
                .log("Codex: not signed in; use Sign in on the Status tab"),
            Err(error) => self.inner.events.log(format!("Codex: {error}")),
        }
        self.publish_status();
        result
    }

    /// Starts the ChatGPT sign-in and opens the returned page in the default browser. The
    /// app-server stays up until the sign-in reports its end, then stops a minute later when
    /// the bot is in no voice channel.
    pub async fn sign_in(&self) -> Result<String, String> {
        *self.inner.sign_in_started.lock().expect("sign-in lock") = Some(Instant::now());
        self.after_on_demand_use();
        let server = self.server().await?;
        let result = server
            .request("account/login/start", json!({ "type": "chatgpt" }))
            .await?;
        let url = result
            .get("authUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex did not return a sign-in link".to_owned())?
            .to_owned();
        let host = url
            .strip_prefix("https://")
            .and_then(|rest| rest.split(['/', '?', '#']).next())
            .unwrap_or_default();
        let trusted = ["openai.com", "chatgpt.com"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
        if !trusted {
            return Err(format!(
                "refusing to open an unexpected sign-in link: {host}"
            ));
        }
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &url])
            .spawn()
            .map_err(|error| format!("could not open the sign-in page: {error}"))?;
        self.inner.events.log(
            "Codex: opened the ChatGPT sign-in in the browser; finish it there, then use Check",
        );
        Ok(url)
    }

    /// Models the signed-in account can use, fetched now.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, String> {
        self.after_on_demand_use();
        let server = self.server().await?;
        self.refresh_models(&server).await
    }

    /// At app start (change `config-tab-and-codex-lists`): reads the account and the lists
    /// as Check does when `enabled` (`codex.refresh_lists_at_start`) or when no lists were
    /// saved, and only when a Codex executable is found; without one nothing starts and no
    /// error is reported. Returns whether the read was started; it runs in the background
    /// and the app-server stops a minute later unless a join needs it.
    pub fn read_lists_at_start(&self, enabled: bool) -> bool {
        let note = self
            .inner
            .lists_load_note
            .lock()
            .expect("lists note lock")
            .take();
        if let Some((level, message)) = note {
            log::log!(level, "{message}");
        }
        if find_codex(&self.config().executable).is_none() {
            return false;
        }
        let reason = if enabled {
            "the setting is on"
        } else if !self.inner.lists_saved_at_start {
            "no saved lists"
        } else {
            return false;
        };
        self.inner
            .events
            .log(format!("Codex: reading the lists at start ({reason})"));
        let assistant = self.clone();
        self.inner.handle.spawn(async move {
            if let Err(error) = assistant.check().await {
                log::warn!("Codex: the read at start failed: {error}");
            }
        });
        true
    }

    fn conversation(&self, guild: u64) -> Arc<Mutex<Conversation>> {
        Arc::clone(
            self.inner
                .conversations
                .lock()
                .expect("conversations lock")
                .entry(guild)
                .or_insert_with(|| Arc::new(Mutex::new(Conversation::new(guild)))),
        )
    }

    fn learned_depth(&self, guild: u64) -> Arc<Mutex<Option<playout::DepthEstimator>>> {
        Arc::clone(
            self.inner
                .learned_depths
                .lock()
                .expect("learned depths lock")
                .entry(guild)
                .or_default(),
        )
    }

    /// Stops every call and the process; waits briefly so the child is gone at exit.
    pub fn stop_blocking(&self) {
        let sessions = self
            .inner
            .sessions
            .lock()
            .expect("sessions lock")
            .drain()
            .map(|(_, session)| session)
            .collect::<Vec<_>>();
        for session in sessions {
            session.shutdown();
        }
        let inner = Arc::clone(&self.inner);
        let server = self.inner.handle.block_on(async move {
            tokio::time::timeout(Duration::from_secs(2), inner.server.lock())
                .await
                .ok()
                .and_then(|mut slot| slot.take())
        });
        if let Some(server) = server {
            server.kill();
        }
    }
}

impl Assistant for CodexAssistant {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn prepare(&self, guild: u64) -> PrepareFuture<'_> {
        Box::pin(async move {
            self.prepare_guild(guild)
                .await
                .map_err(|error| AssistantError::Unavailable(error.to_string()))
        })
    }

    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError> {
        let mut sessions = self.inner.sessions.lock().expect("sessions lock");
        if sessions.contains_key(&guild) {
            return Err(AssistantError::AlreadyAttached(guild));
        }
        // `prepare` ran before the join, so the server is up unless it died since.
        let Some(server) = self.running_server() else {
            return Err(AssistantError::Unavailable(
                "Codex stopped before the join finished; run /join again".to_owned(),
            ));
        };
        if self.config().mode.hears_with_whisper() {
            let tts = self
                .inner
                .tts
                .lock()
                .expect("tts lock")
                .clone()
                .ok_or_else(|| {
                    AssistantError::Unavailable("the Whisper mode's voice is not set up".to_owned())
                })?;
            let conversation = self.conversation(guild);
            let thread_id = conversation
                .lock()
                .expect("conversation lock")
                .thread
                .as_ref()
                .filter(|thread| thread.generation == server.generation)
                .map(|thread| thread.id.clone())
                .unwrap_or_default();
            let backend = CodexTurns::new(guild, server, thread_id, self.inner.events.clone());
            let config = Arc::clone(&self.inner.config);
            let session = TextSession::new(TextParts {
                guild,
                handle: self.inner.handle.clone(),
                backend,
                options: Arc::new(move || {
                    let config = config.read().expect("codex config lock");
                    TextOptions {
                        speaker_labels: config.speaker_labels,
                        one_speaker_at_a_time: config.one_speaker_at_a_time,
                        text_answers: !config.mode.speaks_locally(),
                    }
                }),
                conversation,
                ports,
                chat: Arc::clone(&self.inner.chat),
                events: self.inner.events.clone(),
                errors: Arc::clone(&self.inner.errors),
                tts,
            });
            sessions.insert(guild, Attached::Text(Arc::clone(&session)));
            return Ok(session);
        }
        let session = CodexSession::new(SessionParts {
            guild,
            handle: self.inner.handle.clone(),
            server,
            config: Arc::clone(&self.inner.config),
            account: Arc::clone(&self.inner.account),
            voices: Arc::clone(&self.inner.voices),
            conversation: self.conversation(guild),
            learned_depth: self.learned_depth(guild),
            workspace: self.inner.workspace.clone(),
            ports,
            chat: Arc::clone(&self.inner.chat),
            events: self.inner.events.clone(),
            errors: Arc::clone(&self.inner.errors),
        });
        sessions.insert(guild, Attached::Realtime(Arc::clone(&session)));
        Ok(session)
    }

    fn use_tts(&self, tts: crate::tts::manager::TtsManager) {
        *self.inner.tts.lock().expect("tts lock") = Some(tts);
    }

    fn detach(&self, guild: u64) {
        let (session, left) = {
            let mut sessions = self.inner.sessions.lock().expect("sessions lock");
            let session = sessions.remove(&guild);
            (session, sessions.len())
        };
        if let Some(session) = session {
            session.shutdown();
            if left == 0 {
                self.schedule_stop(LAST_LEAVE_STOP_DELAY, "the bot left its last voice channel");
            }
        }
    }

    fn capabilities(&self) -> Capabilities {
        let voices = self.inner.voices.lock().expect("voices lock").clone();
        Capabilities {
            models: self.inner.models.lock().expect("models lock").clone(),
            voices: voices
                .as_ref()
                .map(|voices| voices.all.clone())
                .unwrap_or_default(),
            default_voice: voices.and_then(|voices| voices.default),
            signed_in: self
                .inner
                .account
                .lock()
                .expect("account lock")
                .as_ref()
                .is_some_and(|account| account.signed_in),
            lists_read_at_ms: *self
                .inner
                .lists_read_at_ms
                .lock()
                .expect("lists read time lock"),
        }
    }

    fn status(&self) -> AssistantStatus {
        let config = self.config();
        let executable = find_codex(&config.executable);
        let server = self.running_server();
        let version = server
            .as_ref()
            .and_then(|server| server.version.clone())
            .or_else(|| {
                executable
                    .as_ref()
                    .and_then(|path| self.executable_version(path))
            });
        let account = self.inner.account.lock().expect("account lock").clone();
        let error = self
            .inner
            .last_error
            .lock()
            .expect("last error lock")
            .clone();
        let signed_in = account.as_ref().is_some_and(|account| account.signed_in);
        let summary = match (&executable, &server, &account, &error) {
            (None, _, _, _) => NOT_FOUND_STATUS.to_owned(),
            (_, _, _, Some(error)) => error.clone(),
            (_, None, _, _) => "codex found; it starts at /join".to_owned(),
            (_, Some(_), Some(account), _) if account.signed_in => format!(
                "signed in{}{}",
                account
                    .kind
                    .as_ref()
                    .map(|kind| format!(" ({kind})"))
                    .unwrap_or_default(),
                account
                    .email
                    .as_ref()
                    .map(|email| format!(" as {email}"))
                    .unwrap_or_default()
            ),
            (_, Some(_), Some(_), _) => "not signed in".to_owned(),
            (_, Some(_), None, _) => "app-server running; account not read yet".to_owned(),
        };
        let sessions = self.inner.sessions.lock().expect("sessions lock");
        let thread = sessions.values().find_map(|session| {
            self.inner
                .conversations
                .lock()
                .expect("conversations lock")
                .get(&session.status().guild_id.parse::<u64>().unwrap_or_default())
                .and_then(|conversation| {
                    conversation
                        .lock()
                        .expect("conversation lock")
                        .thread
                        .clone()
                })
        });
        AssistantStatus {
            engine: "codex".to_owned(),
            summary,
            signed_in,
            account_type: account.as_ref().and_then(|account| account.kind.clone()),
            email: account.as_ref().and_then(|account| account.email.clone()),
            plan: account.as_ref().and_then(|account| account.plan.clone()),
            sign_in_kind: None,
            executable: executable.map(|path| path.display().to_string()),
            version,
            running: server.is_some(),
            error,
            model_in_use: thread.as_ref().map(|thread| thread.model.clone()),
            reasoning_effort_in_use: thread.as_ref().map(|thread| thread.effort.clone()),
            calls: sessions.values().map(|session| session.status()).collect(),
        }
    }

    fn configure(&self, settings: &Settings) {
        self.inner
            .chat
            .set_saving(settings.assistant.save_chat_history);
        // The chat log is shared by every engine (change `claude-and-opencode-engines`).
        self.inner
            .chat
            .set_engine(settings.assistant.engine.display_name());
        let next = CodexConfig::from_settings(settings);
        let mut config = self.inner.config.write().expect("codex config lock");
        if *config != next {
            if config.model != next.model || config.reasoning_effort != next.reasoning_effort {
                self.inner
                    .events
                    .log("Codex: model or reasoning changed; the next call opens a new thread");
            }
            *config = next;
        }
    }

    fn stop(&self) {
        self.stop_blocking();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::CodecFailures;
    use crate::audio::{frame_channel, output_pair, Frame, FRAME_SAMPLES, INPUT_CAPACITY};

    /// The fake app-server in `testdata/fake-codex`, which runs on Node; `None` where Node
    /// is not installed, and the test then checks nothing.
    fn fake_codex(name: &str) -> Option<String> {
        let node = std::process::Command::new("node")
            .arg("--version")
            .output()
            .ok()?;
        node.status
            .success()
            .then(|| format!("{}/testdata/fake-codex/{name}", env!("CARGO_MANIFEST_DIR")))
    }

    fn assistant_with(
        runtime: &tokio::runtime::Runtime,
        dir: &std::path::Path,
        executable: &str,
    ) -> CodexAssistant {
        let events = EventBus::new(256);
        let mut settings = Settings::default();
        settings.codex.executable = executable.to_owned();
        CodexAssistant::new(
            runtime.handle().clone(),
            events.clone(),
            Arc::new(ConnectionErrors::new(events)),
            dir,
            &settings,
        )
    }

    #[test]
    fn the_call_codec_is_taken_from_the_diagnostics_section_at_configure() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let assistant = assistant_with(&runtime, dir.path(), "codex-not-here");
        assert_eq!(
            assistant.inner.config.read().unwrap().opus_codec,
            OpusCodec::Libopus
        );
        let mut settings = Settings::default();
        settings.diagnostics.opus_codec = "opus-rs".to_owned();
        assistant.configure(&settings);
        assert_eq!(
            assistant.inner.config.read().unwrap().opus_codec,
            OpusCodec::OpusRs,
            "the next call opens with it"
        );
    }

    /// A `codex.cmd` that answers `--version` and counts its runs in `runs.txt`.
    fn counting_codex(dir: &std::path::Path, version: &str) -> std::path::PathBuf {
        let path = dir.join("codex.cmd");
        std::fs::write(
            &path,
            format!("@echo off\r\necho run>>\"%~dp0runs.txt\"\r\necho codex-cli {version}\r\n"),
        )
        .unwrap();
        path
    }

    fn runs(dir: &std::path::Path) -> usize {
        std::fs::read_to_string(dir.join("runs.txt"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    #[test]
    fn a_status_read_runs_codex_version_only_for_a_new_file() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let fake = counting_codex(dir.path(), "9.9.9");
        let assistant = assistant_with(&runtime, dir.path(), &fake.display().to_string());
        for _ in 0..10 {
            assert_eq!(
                assistant.status().version.as_deref(),
                Some("codex-cli 9.9.9")
            );
        }
        assert_eq!(runs(dir.path()), 1, "ten status reads, one run");

        counting_codex(dir.path(), "10.0.0-new");
        assert_eq!(
            assistant.status().version.as_deref(),
            Some("codex-cli 10.0.0-new")
        );
        assert_eq!(runs(dir.path()), 2, "the replaced file is read once more");

        let missing = assistant_with(&runtime, dir.path(), "Z:/nowhere/codex.exe");
        assert_eq!(missing.status().version, None);
        assert_eq!(runs(dir.path()), 2, "nothing runs without an executable");
    }

    #[test]
    fn the_chat_history_switch_reaches_the_chat_log() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let assistant = assistant_with(&runtime, dir.path(), "Z:/nowhere/codex.exe");
        let mut settings = Settings::default();
        assistant.configure(&settings);
        assert!(!assistant.chat_log().is_saving());
        settings.assistant.save_chat_history = true;
        assistant.configure(&settings);
        assert!(assistant.chat_log().is_saving());
    }

    #[test]
    fn a_join_is_refused_without_codex_or_without_a_sign_in() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let missing = assistant_with(&runtime, dir.path(), "Z:/nowhere/codex.exe");
        assert_eq!(
            runtime.block_on(missing.prepare_guild(1)),
            Err(PrepareError::NotFound)
        );
        assert!(!missing.status().running);
        let reply = PrepareError::NotFound.to_string();
        assert!(
            reply.contains("Install Codex") && reply.contains("Status tab"),
            "{reply}"
        );
        assert!(reply.contains("/join again"), "{reply}");
        assert!(!reply.contains("npm"), "{reply}");
        let summary = missing.status().summary;
        assert!(summary.starts_with("codex not found"), "{summary}");
        assert!(summary.contains("Install Codex"), "{summary}");
        assert_eq!(
            PrepareError::SignedOut.to_string(),
            "Codex is not signed in: use Sign in on the VoiceBro Status tab, then run /join again",
            "the signed-out reply is unchanged"
        );

        let Some(signed_out) = fake_codex("fake_codex_signed_out.cmd") else {
            return;
        };
        let assistant = assistant_with(&runtime, dir.path(), &signed_out);
        let refused = runtime.block_on(assistant.prepare_guild(1));
        assert_eq!(refused, Err(PrepareError::SignedOut));
        assert!(refused.unwrap_err().to_string().contains("Sign in"));
        assistant.stop_blocking();
    }

    /// The Node fake behind a `codex.cmd` in `dir` that logs every request's method to
    /// `requests.txt` there; `None` without Node.
    fn logging_fake(dir: &std::path::Path) -> Option<String> {
        fake_codex("fake_codex.cmd")?;
        let script = format!(
            "{}/testdata/fake-codex/fake_codex.js",
            env!("CARGO_MANIFEST_DIR")
        );
        let path = dir.join("codex.cmd");
        std::fs::write(
            &path,
            format!(
                "@node \"{script}\" --log \"{}\" %*\r\n",
                dir.join("requests.txt").display()
            ),
        )
        .unwrap();
        Some(path.display().to_string())
    }

    /// How many requests of `method` the logging fake answered.
    fn requests(dir: &std::path::Path, method: &str) -> usize {
        std::fs::read_to_string(dir.join("requests.txt"))
            .map(|text| text.lines().filter(|line| *line == method).count())
            .unwrap_or(0)
    }

    fn saved_sample(dir: &std::path::Path) -> lists::SavedLists {
        let saved = lists::SavedLists::new(
            1_763_141_400_000,
            Some("codex-cli 0.167.0".to_owned()),
            vec![ModelInfo {
                id: "saved-model".to_owned(),
                display_name: "Saved model".to_owned(),
                description: String::new(),
                reasoning_efforts: vec!["medium".to_owned()],
                default_reasoning_effort: Some("medium".to_owned()),
                is_default: true,
            }],
            Some(&Voices {
                all: vec!["saved-voice".to_owned()],
                default: Some("saved-voice".to_owned()),
            }),
        );
        lists::save(&crate::paths::codex_lists_path(dir), &saved).unwrap();
        saved
    }

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(15), "{what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn saved_lists_are_offered_at_start_and_start_nothing() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let saved = saved_sample(dir.path());
        let missing = assistant_with(&runtime, dir.path(), "Z:/nowhere/codex.exe");
        let offered = missing.capabilities();
        assert_eq!(offered.models, saved.models);
        assert_eq!(offered.voices, ["saved-voice"]);
        assert_eq!(offered.default_voice.as_deref(), Some("saved-voice"));
        assert_eq!(offered.lists_read_at_ms, Some(saved.read_at_ms));
        assert!(
            !missing.read_lists_at_start(true),
            "no executable: no read, whatever the switch"
        );
        assert_eq!(missing.status().error, None, "and no error for it");
        assert!(!missing.status().running);

        let Some(fake) = logging_fake(dir.path()) else {
            return;
        };
        let assistant = assistant_with(&runtime, dir.path(), &fake);
        assert!(
            !assistant.read_lists_at_start(false),
            "saved lists and the switch off: Codex does not start"
        );
        std::thread::sleep(Duration::from_millis(500));
        assert!(!assistant.status().running);
        assert_eq!(requests(dir.path(), "initialize"), 0);

        // A file that is not a lists file is no lists: the read at start runs.
        std::fs::write(crate::paths::codex_lists_path(dir.path()), "{ broken").unwrap();
        let broken = assistant_with(&runtime, dir.path(), "Z:/nowhere/codex.exe");
        assert!(broken.capabilities().models.is_empty());
        assert_eq!(broken.capabilities().lists_read_at_ms, None);
    }

    #[test]
    fn the_read_at_start_runs_without_saved_lists_or_with_the_switch_on() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let Some(fake) = logging_fake(dir.path()) else {
            return;
        };
        let file = crate::paths::codex_lists_path(dir.path());
        let first = assistant_with(&runtime, dir.path(), &fake);
        assert_eq!(first.capabilities().lists_read_at_ms, None);
        assert!(first.read_lists_at_start(false), "nothing saved yet");
        wait_for("the read at start saves the lists", || {
            first.capabilities().lists_read_at_ms.is_some()
        });
        let saved = lists::load(&file).unwrap().expect("the file was written");
        let ids: Vec<&str> = saved.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["fake-model-a", "fake-model-b"]);
        assert_eq!(saved.voices, ["fake-cove", "fake-juniper"]);
        assert_eq!(saved.codex_version.as_deref(), Some("codex-cli 0.0.0-fake"));
        assert!(first.status().running, "it stops a minute later");
        assert_eq!(first.status().error, None);
        first.stop_blocking();

        let second = assistant_with(&runtime, dir.path(), &fake);
        assert_eq!(
            second.capabilities().models,
            saved.models,
            "loaded at start"
        );
        assert!(!second.read_lists_at_start(false));
        assert!(second.read_lists_at_start(true), "the switch is on");
        wait_for("the switch reads the lists again", || {
            requests(dir.path(), "model/list") == 2
        });
        wait_for("and saves them", || {
            second.capabilities().lists_read_at_ms > Some(saved.read_at_ms)
        });
        second.stop_blocking();
    }

    #[test]
    fn check_reads_the_lists_every_time_and_a_join_once_per_run() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let Some(fake) = logging_fake(dir.path()) else {
            return;
        };
        let saved = saved_sample(dir.path());
        let assistant = assistant_with(&runtime, dir.path(), &fake);

        runtime
            .block_on(assistant.prepare_guild(1))
            .expect("the fake prepares");
        assert_eq!(
            (
                requests(dir.path(), "model/list"),
                requests(dir.path(), "thread/realtime/listVoices")
            ),
            (1, 1),
            "the first join of the run reads the saved lists again"
        );
        let offered = assistant.capabilities();
        assert_eq!(offered.models[0].id, "fake-model-a");
        assert_eq!(offered.voices, ["fake-cove", "fake-juniper"]);
        assert!(offered.lists_read_at_ms > Some(saved.read_at_ms));
        let file = lists::load(&crate::paths::codex_lists_path(dir.path()))
            .unwrap()
            .unwrap();
        assert_eq!(file.models, offered.models, "and saves them");

        runtime
            .block_on(assistant.prepare_guild(2))
            .expect("a second join");
        assert_eq!(
            (
                requests(dir.path(), "model/list"),
                requests(dir.path(), "thread/realtime/listVoices")
            ),
            (1, 1),
            "a later join of the run reuses them"
        );

        for round in 2..=3 {
            runtime.block_on(assistant.check()).expect("check");
            assert_eq!(
                requests(dir.path(), "model/list"),
                round,
                "check reads the models"
            );
            assert_eq!(
                requests(dir.path(), "thread/realtime/listVoices"),
                round,
                "and the voices"
            );
        }
        assistant.stop_blocking();
    }

    #[test]
    fn an_empty_model_list_keeps_the_saved_lists() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        if logging_fake(dir.path()).is_none() {
            return;
        }
        let saved = saved_sample(dir.path());
        let script = format!(
            "{}/testdata/fake-codex/fake_codex.js",
            env!("CARGO_MANIFEST_DIR")
        );
        let empty = dir.path().join("codex-empty.cmd");
        std::fs::write(&empty, format!("@node \"{script}\" --models none %*\r\n")).unwrap();
        let assistant = assistant_with(&runtime, dir.path(), &empty.display().to_string());
        runtime.block_on(assistant.check()).expect("check");
        assert_eq!(assistant.capabilities().models, saved.models);
        assert_eq!(
            assistant.capabilities().lists_read_at_ms,
            Some(saved.read_at_ms)
        );
        assert_eq!(
            lists::load(&crate::paths::codex_lists_path(dir.path())),
            Ok(Some(saved)),
            "the file stays"
        );
        assistant.stop_blocking();
    }

    /// With `VOICEBRO_CODEX_SMOKE=1`: the real Codex starts at a join preparation and stops
    /// after the last leave.
    #[test]
    fn real_codex_starts_at_join_and_stops_after_the_last_leave_when_enabled() {
        if std::env::var("VOICEBRO_CODEX_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let assistant = assistant_with(&runtime, dir.path(), "");
        assert!(!assistant.status().running);
        let started = Instant::now();
        runtime
            .block_on(assistant.prepare_guild(1))
            .expect("codex runs and is signed in");
        eprintln!(
            "prepared in {} ms: {}",
            started.elapsed().as_millis(),
            assistant.status().summary
        );
        assert!(assistant.status().running);
        let (_sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        drop(
            assistant
                .attach(
                    1,
                    AudioPorts {
                        input,
                        output,
                        failures: CodecFailures::none(),
                        answers: crate::assistant::AnswerSink::none(),
                        heard: crate::assistant::no_heard(),
                    },
                )
                .unwrap(),
        );
        assistant.detach(1);
        let left = Instant::now();
        while assistant.running_server().is_some() {
            assert!(left.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("stopped {} ms after the leave", left.elapsed().as_millis());
    }

    #[test]
    fn the_app_server_runs_from_join_to_the_last_leave_and_the_history_survives() {
        let Some(fake) = fake_codex("fake_codex.cmd") else {
            return;
        };
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let assistant = assistant_with(&runtime, dir.path(), &fake);
        assert!(!assistant.status().running, "nothing runs before a join");

        runtime
            .block_on(assistant.prepare_guild(1))
            .expect("the fake prepares");
        assert!(assistant.status().running);
        let first = assistant
            .conversation(1)
            .lock()
            .unwrap()
            .thread
            .clone()
            .expect("the join opened a thread");
        assistant
            .conversation(1)
            .lock()
            .unwrap()
            .push("user", "remember forty two".to_owned());
        let (_sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, _source) = output_pair();
        let session = assistant
            .attach(
                1,
                AudioPorts {
                    input,
                    output,
                    failures: CodecFailures::none(),
                    answers: crate::assistant::AnswerSink::none(),
                    heard: crate::assistant::no_heard(),
                },
            )
            .expect("attach after a preparation");
        drop(session);

        assistant.detach(1);
        let left = Instant::now();
        while assistant.running_server().is_some() {
            assert!(
                left.elapsed() < Duration::from_secs(10),
                "the server stops after the last leave"
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        runtime
            .block_on(assistant.prepare_guild(1))
            .expect("the next join starts a new server");
        let conversation = assistant.conversation(1);
        let conversation = conversation.lock().unwrap();
        let second = conversation.thread.clone().unwrap();
        assert_ne!(first.generation, second.generation, "a new process");
        assert_ne!(first.id, second.id, "a new thread");
        assert_eq!(conversation.len(), 1, "the history is kept");
        drop(conversation);
        assistant.stop_blocking();
    }

    fn wav_48k_mono(path: &str) -> Vec<i16> {
        let bytes = std::fs::read(path).expect("wav readable");
        let data = bytes
            .windows(4)
            .position(|window| window == b"data")
            .map(|index| index + 8)
            .expect("wav has a data chunk");
        bytes[data..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect()
    }

    /// Feeds a 48 kHz mono WAV as 20 ms stereo frames with the jitter of songbird's tick
    /// (each frame due at 20 k ms plus -7..7 ms, intervals between 6 and 34 ms), then
    /// silence until the assistant has answered and been quiet for 1.5 s (or `limit`
    /// passes). Frames with speech are reported to `latency` as the gate does.
    fn speak_and_wait(
        sender: &crate::audio::FrameSender,
        output: &crate::audio::OutputHandle,
        latency: &crate::latency::SharedLatency,
        chat: &ChatLog,
        before: usize,
        samples: &[i16],
        limit: Duration,
    ) -> Option<String> {
        feed_speech(sender, latency, samples);
        let deadline = Instant::now() + limit;
        loop {
            sender.send(Frame::silence());
            std::thread::sleep(Duration::from_millis(20));
            let answer = chat
                .entries()
                .iter()
                .skip(before)
                .find(|entry| entry.kind == "assistant")
                .map(|entry| entry.text.clone());
            if answer.is_some()
                && output
                    .last_signal()
                    .is_some_and(|at| at.elapsed() > Duration::from_millis(1_500))
            {
                return answer;
            }
            if Instant::now() > deadline {
                return answer;
            }
        }
    }

    /// Feeds 48 kHz mono speech toward the call as songbird would, one frame per jittery
    /// 20 ms tick; returns when its last frame was fed.
    fn feed_speech(
        sender: &crate::audio::FrameSender,
        latency: &crate::latency::SharedLatency,
        samples: &[i16],
    ) -> Instant {
        let started = Instant::now();
        let mut seed = 5u32;
        for (index, chunk) in samples.chunks(FRAME_SAMPLES / 2).enumerate() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let jitter = ((seed >> 16) % 15) as i64 - 7;
            let due = started + Duration::from_millis((20 * index as i64 + 10 + jitter) as u64);
            if let Some(rest) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(rest);
            }
            let mut stereo = Vec::with_capacity(FRAME_SAMPLES);
            for sample in chunk {
                stereo.push(*sample);
                stereo.push(*sample);
            }
            stereo.resize(FRAME_SAMPLES, 0);
            let frame = Frame::from_slice(&stereo);
            if frame.peak() >= crate::gate::VOICE_PEAK_THRESHOLD {
                latency.lock().unwrap().speech(Instant::now());
            }
            sender.send(frame);
        }
        Instant::now()
    }

    /// Records the people's transcript pieces the session hands to the gate, with the time
    /// each arrived.
    #[derive(Default)]
    struct HeardLog(Mutex<Vec<(Instant, u64, String, bool)>>);

    impl crate::assistant::HeardSink for HeardLog {
        fn on_heard(&self, _: u64, turn: u64, text: &str, done: bool) {
            self.0
                .lock()
                .unwrap()
                .push((Instant::now(), turn, text.to_owned(), done));
        }
    }

    /// What the gate's rule (design D2 of change `voice-recognition-rework`) does with these
    /// pieces: when the voice is silenced (the first match, held or not) and when the
    /// session ends (a closed match or a turn's end, or a held match that got no newer
    /// piece for the hold's time).
    fn transcript_rule_decision(
        pieces: &[(Instant, u64, String, bool)],
        phrases: &[crate::wake::Phrase],
    ) -> (Option<Instant>, Option<Instant>) {
        let hold_time = crate::gate::TRANSCRIPT_HOLD;
        let mut hold: Option<(u64, Instant)> = None;
        let mut silenced = None;
        let mut previous: Option<(u64, String, Instant)> = None;
        for (at, turn, text, done) in pieces {
            if let Some((held_turn, since)) = hold {
                if *at >= since + hold_time {
                    return (silenced, Some(since + hold_time));
                }
                if held_turn != *turn {
                    return (silenced, Some(*at));
                }
            }
            let (joined, earlier) = match previous.as_ref().filter(|(prior, _, ended)| {
                prior != turn
                    && at.saturating_duration_since(*ended) <= crate::gate::PREVIOUS_TURN_WINDOW
            }) {
                Some((_, prior, _)) => (
                    format!("{prior} {text}"),
                    crate::settings::phrase_words(prior).len(),
                ),
                None => (text.clone(), 0),
            };
            if *done {
                previous = Some((*turn, text.clone(), *at));
            }
            match crate::wake::spotter::transcript_match_after(&joined, phrases, earlier) {
                Some(found) if !found.open || *done => {
                    return (silenced.or(Some(*at)), Some(*at));
                }
                Some(_) => {
                    silenced = silenced.or(Some(*at));
                    hold = Some((*turn, *at));
                }
                None => {
                    // The word went on: the voice comes back.
                    silenced = None;
                    hold = None;
                }
            }
        }
        (silenced, hold.map(|(_, since)| since + hold_time))
    }

    /// Reads the output every 20 ms like songbird's mixer, counting the frames with signal
    /// and keeping when each played, until `stop` is set.
    fn spawn_reader(
        mut source: crate::audio::PcmSource,
        stop: Arc<std::sync::atomic::AtomicBool>,
        voiced: Arc<std::sync::atomic::AtomicU64>,
        voiced_at: Arc<Mutex<Vec<Instant>>>,
    ) -> std::thread::JoinHandle<()> {
        use std::io::Read as _;
        std::thread::spawn(move || {
            let mut bytes = vec![0u8; FRAME_SAMPLES * 4];
            let mut next = Instant::now();
            while !stop.load(std::sync::atomic::Ordering::Acquire) {
                next += Duration::from_millis(20);
                let mut filled = 0;
                while filled < bytes.len() {
                    match source.read(&mut bytes[filled..]) {
                        Ok(0) | Err(_) => return,
                        Ok(read) => filled += read,
                    }
                }
                let loud = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|sample| f32::from_le_bytes(*sample).abs() > 256.0 / 32_768.0);
                if loud {
                    voiced.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    voiced_at.lock().unwrap().push(Instant::now());
                }
                if let Some(rest) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(rest);
                }
            }
        })
    }

    fn matches_any(text: &str, options: &str) -> bool {
        let lowered = text.to_lowercase();
        options
            .split('|')
            .any(|option| lowered.contains(&option.trim().to_lowercase()))
    }

    /// Probe 2.1 of change `speaker-turns`, when `VOICEBRO_CODEX_TURNS_A` and `_B` name two
    /// 48 kHz mono questions: per run a fresh call gets A, then N ms of silence, then (with
    /// the note) "now speaking: Sam" through `thread/realtime/appendText`, then B, then
    /// silence until the answer has ended. N runs over `VOICEBRO_CODEX_TURNS_GAPS` (default
    /// `100,200,300,500,700,1000`), the note over `VOICEBRO_CODEX_TURNS_NOTE` (`0`, `1` or
    /// `both`, the default), `VOICEBRO_CODEX_TURNS_RUNS` times each (default 3). Per run it
    /// prints whether answer audio came before B started, the people's transcript pieces,
    /// the chat items (the note among them) and the time from B's end to the answer.
    #[test]
    fn two_speakers_when_enabled() {
        let (Ok(wav_a), Ok(wav_b)) = (
            std::env::var("VOICEBRO_CODEX_TURNS_A"),
            std::env::var("VOICEBRO_CODEX_TURNS_B"),
        ) else {
            return;
        };
        let gaps: Vec<u64> = std::env::var("VOICEBRO_CODEX_TURNS_GAPS")
            .unwrap_or_else(|_| "100,200,300,500,700,1000".to_owned())
            .split(',')
            .filter_map(|gap| gap.trim().parse().ok())
            .collect();
        let notes: Vec<bool> = match std::env::var("VOICEBRO_CODEX_TURNS_NOTE").as_deref() {
            Ok("0") => vec![false],
            Ok("1") => vec![true],
            _ => vec![false, true],
        };
        let runs: usize = std::env::var("VOICEBRO_CODEX_TURNS_RUNS")
            .ok()
            .and_then(|runs| runs.parse().ok())
            .unwrap_or(3);
        let (a, b) = (wav_48k_mono(&wav_a), wav_48k_mono(&wav_b));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut summary = Vec::new();
        for gap in &gaps {
            for note in &notes {
                for run in 1..=runs {
                    let dir = tempfile::tempdir().unwrap();
                    let events = EventBus::new(4096);
                    let mut window = events.subscribe();
                    let errors = Arc::new(ConnectionErrors::new(events.clone()));
                    let mut settings = Settings::default();
                    settings.assistant.instructions = "You are a voice assistant in a Discord voice channel with several people. Answer briefly; when more than one person asked something, answer each of them by name.".to_owned();
                    // The note goes out only with labels at `changes`.
                    settings.assistant.speaker_labels = crate::settings::SpeakerLabels::Changes;
                    let assistant = CodexAssistant::new(
                        runtime.handle().clone(),
                        events,
                        errors,
                        dir.path(),
                        &settings,
                    );
                    runtime
                        .block_on(assistant.prepare_guild(1))
                        .expect("codex runs and is signed in");
                    let (sender, input) = frame_channel(INPUT_CAPACITY);
                    let (output, source) = output_pair();
                    let latency = crate::latency::SharedLatency::default();
                    output.use_latency(Arc::clone(&latency), 1);
                    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let voiced = Arc::new(std::sync::atomic::AtomicU64::new(0));
                    let voiced_at = Arc::new(Mutex::new(Vec::new()));
                    let reader = spawn_reader(
                        source,
                        Arc::clone(&stop),
                        Arc::clone(&voiced),
                        Arc::clone(&voiced_at),
                    );
                    let heard = Arc::new(HeardLog::default());
                    let (answers, mut answer_queue) = crate::assistant::AnswerSink::channel();
                    let session = assistant
                        .attach(
                            1,
                            AudioPorts {
                                input,
                                output: output.clone(),
                                failures: CodecFailures::none(),
                                answers,
                                heard: heard.clone(),
                            },
                        )
                        .expect("attach");
                    session.call_context(
                        Some("Alex".to_owned()),
                        &["Alex".to_owned(), "Sam".to_owned()],
                    );
                    let started = Instant::now();
                    session.session_wanted(true);
                    while !session.ready() {
                        assert!(
                            started.elapsed() < Duration::from_secs(40),
                            "the call did not become ready"
                        );
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    let a_end = feed_speech(&sender, &latency, &a);
                    while a_end.elapsed() < Duration::from_millis(*gap) {
                        sender.send(Frame::silence());
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    let note_at = note.then(|| {
                        session.speakers_changed(&["Sam".to_owned()]);
                        Instant::now()
                    });
                    let b_start = Instant::now();
                    let b_end = feed_speech(&sender, &latency, &b);
                    // The voice may answer with a filler and hand the questions to the agent;
                    // the run ends once every task the voice gave the agent has its final
                    // answer (the answers handed over for posting) and the voice has been
                    // quiet for 4 s after it, or after 70 s.
                    let deadline = Instant::now() + Duration::from_secs(70);
                    let mut finals: Vec<String> = Vec::new();
                    let mut final_at = None;
                    loop {
                        sender.send(Frame::silence());
                        std::thread::sleep(Duration::from_millis(20));
                        while let Ok(answer) = answer_queue.try_recv() {
                            finals.push(answer.text);
                            final_at = Some(Instant::now());
                        }
                        let entries = assistant.chat_log().entries();
                        let tasks = entries.iter().filter(|entry| entry.kind == "task").count();
                        let last = voiced_at.lock().unwrap().last().copied();
                        let quiet =
                            last.is_some_and(|last| last.elapsed() > Duration::from_secs(4));
                        let done = if tasks == 0 {
                            entries.iter().any(|entry| entry.kind == "assistant")
                        } else {
                            finals.len() >= tasks
                                && final_at.is_some_and(|at| last.is_some_and(|last| last > at))
                        };
                        if (done && quiet) || Instant::now() > deadline {
                            break;
                        }
                    }
                    let ms = |at: Instant| -> i64 {
                        if at >= a_end {
                            (at - a_end).as_millis() as i64
                        } else {
                            -((a_end - at).as_millis() as i64)
                        }
                    };
                    let frames = voiced_at.lock().unwrap().clone();
                    let before_b = frames.iter().any(|at| *at > a_end && *at < b_start);
                    let first_audio = frames.iter().find(|at| **at > a_end).copied();
                    let after_b = frames.iter().find(|at| **at > b_end).copied();
                    let mut items = Vec::new();
                    while let Ok(event) = window.try_recv() {
                        if let crate::events::AppEvent::Chat(entry) = event {
                            items.push(format!("{} {:?}", entry.kind, entry.text));
                        }
                    }
                    let pieces: Vec<String> = heard
                        .0
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(_, _, _, done)| *done)
                        .map(|(at, turn, text, _)| format!("{} ms turn {turn}: {text:?}", ms(*at)))
                        .collect();
                    // What the people heard: the voice's own lines, which retell the agent's
                    // final answers; the finals are printed apart.
                    let answer = assistant
                        .chat_log()
                        .entries()
                        .iter()
                        .filter(|entry| entry.kind == "assistant")
                        .map(|entry| entry.text.clone())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    // Covered means answered: by the agent's finals when the voice handed the
                    // questions over, else by the voice's lines (a filler that only names
                    // the cities does not count when there are finals).
                    let lower = if finals.is_empty() {
                        answer.to_lowercase()
                    } else {
                        finals.join(" ").to_lowercase()
                    };
                    let answer = format!("{answer} || finals: {}", finals.join(" | "));
                    let line = format!(
                        "turns: gap {gap} ms, note {}, run {run}: answered before B {before_b}; B started {} ms after A, note at {}; first answer audio at {}; first answer audio {} ms after B ended; covers (Bangkok, London) ({}, {}); pieces {pieces:?}; items {items:?}; answer {answer:?}",
                        if *note { "yes" } else { "no" },
                        ms(b_start),
                        note_at.map_or("-".to_owned(), |at| format!("{} ms", ms(at))),
                        first_audio.map_or("-".to_owned(), |at| format!("{} ms", ms(at))),
                        after_b.map_or("-".to_owned(), |at| (at - b_end).as_millis().to_string()),
                        lower.contains("bangkok"),
                        lower.contains("london"),
                    );
                    eprintln!("{line}");
                    summary.push(format!(
                        "gap {gap} note {} run {run}: before B {before_b}, audio after B {}, both {}",
                        if *note { "yes" } else { "no" },
                        after_b.map_or("-".to_owned(), |at| format!("{} ms", (at - b_end).as_millis())),
                        lower.contains("bangkok") && lower.contains("london"),
                    ));
                    session.session_wanted(false);
                    std::thread::sleep(Duration::from_millis(500));
                    stop.store(true, std::sync::atomic::Ordering::Release);
                    let _ = reader.join();
                    assistant.stop_blocking();
                }
            }
        }
        for line in summary {
            eprintln!("summary: {line}");
        }
    }

    /// Check 6.1 of change `speaker-turns`, when `VOICEBRO_CODEX_FLOOR_A` and `_B` name two
    /// 48 kHz mono questions: per run a gate in always mode feeds a real call, with Alex
    /// (source 1) saying A and Sam (source 2) starting B 2 s before A ends, as two Discord
    /// streams on a 20 ms tick. `VOICEBRO_CODEX_FLOOR_MODES` (`floor,mix` by default) runs
    /// it with one speaker at a time on and off, `VOICEBRO_CODEX_FLOOR_RUNS` times each
    /// (default 10). Prints per run whether the agent's answers (or the voice's lines) cover
    /// both cities and name both people, and the time from the last speech to the first
    /// answer audio.
    #[test]
    fn two_speakers_through_the_floor_when_enabled() {
        let (Ok(wav_a), Ok(wav_b)) = (
            std::env::var("VOICEBRO_CODEX_FLOOR_A"),
            std::env::var("VOICEBRO_CODEX_FLOOR_B"),
        ) else {
            return;
        };
        let modes: Vec<bool> = std::env::var("VOICEBRO_CODEX_FLOOR_MODES")
            .unwrap_or_else(|_| "floor,mix".to_owned())
            .split(',')
            .map(|mode| mode.trim() == "floor")
            .collect();
        let runs: usize = std::env::var("VOICEBRO_CODEX_FLOOR_RUNS")
            .ok()
            .and_then(|runs| runs.parse().ok())
            .unwrap_or(10);
        let stereo = |samples: &[i16]| -> Vec<Vec<i16>> {
            samples
                .chunks(FRAME_SAMPLES / 2)
                .map(|chunk| {
                    let mut frame: Vec<i16> = chunk.iter().flat_map(|s| [*s, *s]).collect();
                    frame.resize(FRAME_SAMPLES, 0);
                    frame
                })
                .collect()
        };
        let (a, b) = (stereo(&wav_48k_mono(&wav_a)), stereo(&wav_48k_mono(&wav_b)));
        let b_start = a.len().saturating_sub(100); // 2 s before A ends
        let last_speech = (b_start + b.len()).max(a.len());
        let quiet = vec![0i16; FRAME_SAMPLES];
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut summary = Vec::new();
        for floor in &modes {
            for run in 1..=runs {
                let dir = tempfile::tempdir().unwrap();
                let events = EventBus::new(4096);
                let errors = Arc::new(ConnectionErrors::new(events.clone()));
                let mut settings = Settings::default();
                settings.assistant.instructions = "You are a voice assistant in a Discord voice channel with several people. Answer briefly; when more than one person asked something, answer each of them by name.".to_owned();
                settings.session.mode = crate::settings::SessionMode::Always;
                settings.assistant.one_speaker_at_a_time = *floor;
                // `VOICEBRO_CODEX_FLOOR_LABELS=changes` sends the note at every floor change.
                if std::env::var("VOICEBRO_CODEX_FLOOR_LABELS").as_deref() == Ok("changes") {
                    settings.assistant.speaker_labels = crate::settings::SpeakerLabels::Changes;
                }
                let assistant = CodexAssistant::new(
                    runtime.handle().clone(),
                    events.clone(),
                    errors,
                    dir.path(),
                    &settings,
                );
                runtime
                    .block_on(assistant.prepare_guild(1))
                    .expect("codex runs and is signed in");
                let (sender, input) = frame_channel(INPUT_CAPACITY);
                let (output, source) = output_pair();
                let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let voiced = Arc::new(std::sync::atomic::AtomicU64::new(0));
                let voiced_at = Arc::new(Mutex::new(Vec::new()));
                let reader = spawn_reader(
                    source,
                    Arc::clone(&stop),
                    Arc::clone(&voiced),
                    Arc::clone(&voiced_at),
                );
                let (answers, mut answer_queue) = crate::assistant::AnswerSink::channel();
                let session = assistant
                    .attach(
                        1,
                        AudioPorts {
                            input,
                            output: output.clone(),
                            failures: CodecFailures::none(),
                            answers,
                            heard: crate::assistant::no_heard(),
                        },
                    )
                    .expect("attach");
                let gate = crate::gate::RelayGate::new(
                    1,
                    sender,
                    Arc::new(crate::audio::GainStage::new()),
                    output.clone(),
                    crate::gate::GateSettings::from_settings(&settings),
                    false,
                    events,
                    None,
                    None,
                );
                gate.set_speaker_name(1, "Alex");
                gate.set_speaker_name(2, "Sam");
                gate.set_participants(&["Alex".to_owned(), "Sam".to_owned()]);
                gate.set_session(session.clone());
                let started = Instant::now();
                while !session.ready() {
                    assert!(started.elapsed() < Duration::from_secs(40), "not ready");
                    std::thread::sleep(Duration::from_millis(50));
                }
                let begin = Instant::now();
                let mut speech_end = begin;
                let mut finals: Vec<String> = Vec::new();
                let mut final_at = None;
                let deadline = begin + Duration::from_secs(90);
                let mut sums = Vec::new();
                for tick in 0usize.. {
                    let due = begin + Duration::from_millis(20 * tick as u64);
                    if let Some(rest) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(rest);
                    }
                    let alex: &[i16] = a.get(tick).map_or(&quiet, Vec::as_slice);
                    let sam: &[i16] = tick
                        .checked_sub(b_start)
                        .and_then(|index| b.get(index))
                        .map_or(&quiet, Vec::as_slice);
                    let mut mixed = Frame::silence();
                    crate::audio::frame::mix_into(&[alex, sam], &mut sums, &mut mixed);
                    gate.process_tick(&mixed, &[(1, alex), (2, sam)]);
                    if tick == last_speech {
                        speech_end = Instant::now();
                    }
                    while let Ok(answer) = answer_queue.try_recv() {
                        finals.push(answer.text);
                        final_at = Some(Instant::now());
                    }
                    if tick > last_speech + 50 {
                        let entries = assistant.chat_log().entries();
                        let tasks = entries.iter().filter(|entry| entry.kind == "task").count();
                        let last = voiced_at.lock().unwrap().last().copied();
                        let quiet_now =
                            last.is_some_and(|last| last.elapsed() > Duration::from_secs(4));
                        let done = if tasks == 0 {
                            entries.iter().any(|entry| entry.kind == "assistant")
                        } else {
                            finals.len() >= tasks
                                && final_at.is_some_and(|at| last.is_some_and(|last| last > at))
                        };
                        if (done && quiet_now) || Instant::now() > deadline {
                            break;
                        }
                    }
                }
                let first_answer = voiced_at
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|at| **at > speech_end)
                    .copied();
                let lines = assistant
                    .chat_log()
                    .entries()
                    .iter()
                    .filter(|entry| matches!(entry.kind, "assistant" | "user" | "context"))
                    .map(|entry| format!("{}: {}", entry.kind, entry.text))
                    .collect::<Vec<_>>();
                let judged = if finals.is_empty() {
                    lines
                        .iter()
                        .filter(|line| line.starts_with("assistant"))
                        .cloned()
                        .collect::<String>()
                } else {
                    finals.join(" ")
                }
                .to_lowercase();
                let both = judged.contains("bangkok") && judged.contains("london");
                let named = judged.contains("alex") && judged.contains("sam");
                let latency = first_answer.map(|at| (at - speech_end).as_millis());
                eprintln!(
                    "floor check: {} run {run}: both answered {both}, both named {named}, first answer audio {} ms after the last speech; floor passes {}; lines {lines:?}; finals {finals:?}",
                    if *floor { "floor" } else { "mix" },
                    latency.map_or("-".to_owned(), |ms| ms.to_string()),
                    gate.status().counters.floor_passes,
                );
                summary.push(format!(
                    "{} run {run}: both {both}, named {named}, latency {}",
                    if *floor { "floor" } else { "mix" },
                    latency.map_or("-".to_owned(), |ms| format!("{ms} ms")),
                ));
                session.session_wanted(false);
                std::thread::sleep(Duration::from_millis(500));
                stop.store(true, std::sync::atomic::Ordering::Release);
                let _ = reader.join();
                assistant.stop_blocking();
            }
        }
        for line in summary {
            eprintln!("summary: {line}");
        }
    }

    /// A real call through the engine when `VOICEBRO_CODEX_CALL_WAV` names a 48 kHz mono
    /// question, fed with songbird-like jitter: the canary word in the instructions must
    /// start the answer, answer audio must play, and no frame toward the call may be dropped;
    /// with `VOICEBRO_CODEX_EXPECT1` the user transcript must contain one of its options.
    /// With `VOICEBRO_CODEX_CALL_WAV2` a second call follows a stop, and its answer must
    /// contain `VOICEBRO_CODEX_EXPECT2` (continuity across calls). With
    /// `VOICEBRO_CODEX_CALL_WAV3` (a question about current facts) the agent must search
    /// the web.
    #[test]
    fn live_call_when_enabled() {
        let Ok(wav) = std::env::var("VOICEBRO_CODEX_CALL_WAV") else {
            return;
        };
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let events = EventBus::new(4096);
        // What the window would receive: every chat event, serialized as the bridge does,
        // with the time it arrived.
        let mut window = events.subscribe();
        let errors = Arc::new(ConnectionErrors::new(events.clone()));
        let mut settings = Settings::default();
        settings.assistant.instructions = "You are a voice assistant in a Discord voice channel. Answer briefly, in one or two sentences, in the speaker's language. Start every reply with the English word Banana.".to_owned();
        // The answer handoff mode under test; the settings default otherwise.
        if let Ok(mode) = std::env::var("VOICEBRO_CODEX_HANDOFF_MODE") {
            settings.codex.handoff_mode = mode;
            settings
                .validate()
                .expect("VOICEBRO_CODEX_HANDOFF_MODE is a handoff mode");
        }
        // The call's Opus codec (change `opus-codec-experiment`); libopus when unset.
        let codec = OpusCodec::from_env().expect("VOICEBRO_OPUS_CODEC");
        settings.diagnostics.opus_codec = codec.name().to_owned();
        eprintln!("codec: {}", codec.label());
        let assistant = CodexAssistant::new(
            runtime.handle().clone(),
            events,
            errors,
            dir.path(),
            &settings,
        );
        runtime
            .block_on(assistant.prepare_guild(1))
            .expect("codex runs and is signed in");
        eprintln!("status: {:?}", assistant.status());
        let (sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, source) = output_pair();
        // The turn latency as the app measures it: speech fed by `speak_and_wait`, the
        // answer's playback reported by the output the reader pulls.
        let latency = crate::latency::SharedLatency::default();
        output.use_latency(Arc::clone(&latency), 1);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let voiced = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let voiced_at = Arc::new(Mutex::new(Vec::new()));
        let reader = spawn_reader(
            source,
            Arc::clone(&stop),
            Arc::clone(&voiced),
            Arc::clone(&voiced_at),
        );
        let heard = Arc::new(HeardLog::default());
        // The answers the session hands over for posting (change `post-assistant-answers`).
        let (answers, mut answer_queue) = crate::assistant::AnswerSink::channel();
        let mut posted: Vec<crate::assistant::AgentAnswer> = Vec::new();
        let mut take_answers = |posted: &mut Vec<crate::assistant::AgentAnswer>| {
            while let Ok(answer) = answer_queue.try_recv() {
                eprintln!(
                    "answer for posting: task {:?}; {} characters:\n{}",
                    answer.task,
                    answer.text.chars().count(),
                    answer.text
                );
                posted.push(answer);
            }
        };
        let session = assistant
            .attach(
                1,
                AudioPorts {
                    input,
                    output: output.clone(),
                    failures: CodecFailures::none(),
                    answers,
                    heard: heard.clone(),
                },
            )
            .expect("attach");
        session.call_context(
            Some("Alex".to_owned()),
            &["Alex".to_owned(), "Bob".to_owned()],
        );
        // Every packet of the assistant's voice, for the replay stand.
        let record = std::env::var("VOICEBRO_CODEX_RECORD").ok();
        if let Some(path) = &record {
            jitter::recording::start(path).expect("recording file");
        }
        let started = Instant::now();
        session.session_wanted(true);
        while !session.ready() {
            assert!(
                started.elapsed() < Duration::from_secs(40),
                "the call did not become ready: {:?}",
                assistant.status()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("ready after {:?}", started.elapsed());
        let chat = assistant.chat_log();
        // Task 5.6 of change `voice-recognition-rework`: the deactivation phrase said 1.5 s
        // into the answer. Prints, in ms after the phrase's last frame was fed, each piece
        // of the people's transcript, when the gate's rule would end the session, and the
        // first frame of the voice's reply; then ends the call.
        if let Ok(phrase_wav) = std::env::var("VOICEBRO_CODEX_CALL_DEACTIVATE_WAV") {
            let phrases = crate::wake::Phrase::list(
                &std::env::var("VOICEBRO_CODEX_DEACTIVATE_PHRASES")
                    .unwrap_or_else(|_| "goodbye, отбой братан".to_owned())
                    .split(',')
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            );
            let question_end = feed_speech(&sender, &latency, &wav_48k_mono(&wav));
            let deadline = Instant::now() + Duration::from_secs(40);
            let answer_start = loop {
                sender.send(Frame::silence());
                std::thread::sleep(Duration::from_millis(20));
                let first = voiced_at
                    .lock()
                    .unwrap()
                    .iter()
                    .copied()
                    .find(|at| *at > question_end);
                if let Some(first) = first {
                    break first;
                }
                assert!(Instant::now() < deadline, "the voice never answered");
            };
            while answer_start.elapsed() < Duration::from_millis(1_500) {
                sender.send(Frame::silence());
                std::thread::sleep(Duration::from_millis(20));
            }
            let phrase_start = Instant::now();
            let phrase_end = feed_speech(&sender, &latency, &wav_48k_mono(&phrase_wav));
            for _ in 0..300 {
                sender.send(Frame::silence());
                std::thread::sleep(Duration::from_millis(20));
            }
            let ms = |at: Instant| -> i64 {
                if at >= phrase_end {
                    (at - phrase_end).as_millis() as i64
                } else {
                    -((phrase_end - at).as_millis() as i64)
                }
            };
            let pieces: Vec<(Instant, u64, String, bool)> = heard
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(at, ..)| *at >= phrase_start)
                .cloned()
                .collect();
            for (at, turn, text, done) in &pieces {
                eprintln!(
                    "deactivation: piece at {} ms, turn {turn}{}: {text:?}",
                    ms(*at),
                    if *done { " (done)" } else { "" }
                );
            }
            let (silenced, decision) = transcript_rule_decision(&pieces, &phrases);
            // The reply: the first voiced frame after the phrase that follows 200 ms without
            // one (the answer cut by the barge-in, then the reply).
            let frames = voiced_at.lock().unwrap().clone();
            let reply = frames
                .iter()
                .enumerate()
                .filter(|(_, at)| **at > phrase_end)
                .find(|(index, at)| {
                    *index == 0
                        || at.saturating_duration_since(frames[index - 1])
                            >= Duration::from_millis(200)
                })
                .map(|(_, at)| *at);
            let answer_cut = frames
                .iter()
                .rev()
                .find(|at| **at < reply.unwrap_or(phrase_end + Duration::from_secs(60)))
                .copied();
            let shown = |at: Option<Instant>, none: &str| {
                at.map(|at| ms(at).to_string())
                    .unwrap_or_else(|| none.to_owned())
            };
            eprintln!(
                "deactivation: answer started {} ms, last answer frame {} ms, the rule silences the voice at {} ms and ends the session at {} ms, the reply's first frame at {} ms; the rule {}",
                ms(answer_start),
                shown(answer_cut, "-"),
                shown(silenced, "never"),
                shown(decision, "never"),
                shown(reply, "none"),
                match (silenced, reply) {
                    (Some(silenced), Some(reply)) if silenced < reply => "comes first",
                    (Some(_), None) => "comes first (no reply)",
                    (Some(_), Some(_)) => "comes after the reply started",
                    (None, _) => "never matched",
                }
            );
            session.session_wanted(false);
            std::thread::sleep(Duration::from_millis(500));
            stop.store(true, std::sync::atomic::Ordering::Release);
            let _ = reader.join();
            assistant.stop_blocking();
            assert!(decision.is_some(), "the transcript held the phrase");
            return;
        }
        let answer = speak_and_wait(
            &sender,
            &output,
            &latency,
            chat,
            chat.entries().len(),
            &wav_48k_mono(&wav),
            Duration::from_secs(45),
        );
        let heard = chat
            .entries()
            .iter()
            .find(|entry| entry.kind == "user")
            .map(|entry| entry.text.clone());
        let audio = assistant.status().calls[0].audio.unwrap_or_default();
        eprintln!(
            "first question heard as {heard:?}; first answer: {answer:?}; voiced frames played {}",
            voiced.load(std::sync::atomic::Ordering::Relaxed)
        );
        eprintln!("{}", audio.log_line(1));
        let answer = answer.expect("the assistant answered");
        assert!(
            answer
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
                .starts_with("banana"),
            "canary not followed: {answer}"
        );
        assert!(
            voiced.load(std::sync::atomic::Ordering::Relaxed) > 25,
            "no answer audio played"
        );
        assert_eq!(audio.input_drops, 0, "a frame toward the call was dropped");
        if let Ok(expected) = std::env::var("VOICEBRO_CODEX_EXPECT1") {
            let heard = heard.expect("the question was transcribed");
            assert!(
                matches_any(&heard, &expected),
                "the question was misheard: {heard}"
            );
        }

        if let Ok(wav2) = std::env::var("VOICEBRO_CODEX_CALL_WAV2") {
            session.session_wanted(false);
            std::thread::sleep(Duration::from_secs(2));
            assert!(!session.ready());
            // The playout depth the first call learnt is kept for the guild. To see the next
            // call start from what is kept, put a depth there that the prior cannot give.
            let kept = assistant.learned_depth(1);
            let learnt = kept
                .lock()
                .unwrap()
                .as_ref()
                .map(playout::DepthEstimator::depth_ms);
            eprintln!("the first call learnt a voice buffer of {learnt:?} ms");
            assert!(learnt.is_some(), "the first call left what it learnt");
            let mut planted = playout::DepthEstimator::new();
            let origin = Instant::now();
            for i in 0..500usize {
                let stall = (i / 100) as u64 * 300;
                planted.on_packet(
                    origin + Duration::from_millis(i as u64 * 20 + stall),
                    (i * 960) as u32,
                );
            }
            let planted_depth = planted.depth_ms();
            assert_ne!(planted_depth, 200, "distinct from the prior");
            *kept.lock().unwrap() = Some(planted.learned());
            let before = chat.entries().len();
            let started = Instant::now();
            session.session_wanted(true);
            while !session.ready() {
                assert!(
                    started.elapsed() < Duration::from_secs(40),
                    "second call not ready"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            eprintln!("second call ready after {:?}", started.elapsed());
            let starting = assistant.status().calls[0]
                .audio
                .unwrap_or_default()
                .depth_ms;
            eprintln!("the second call opened with {starting} ms; {planted_depth} ms was kept");
            assert_eq!(
                starting, planted_depth as u64,
                "the second call starts from what was kept"
            );
            let answer = speak_and_wait(
                &sender,
                &output,
                &latency,
                chat,
                before,
                &wav_48k_mono(&wav2),
                Duration::from_secs(45),
            );
            eprintln!("second answer: {answer:?}");
            let answer = answer.expect("the second answer arrived");
            if let Ok(expected) = std::env::var("VOICEBRO_CODEX_EXPECT2") {
                assert!(
                    matches_any(&answer, &expected),
                    "the second answer does not continue the conversation: {answer}"
                );
            }
        }
        if let Ok(wav3) = std::env::var("VOICEBRO_CODEX_CALL_WAV3") {
            let before = chat.entries().len();
            let answer = speak_and_wait(
                &sender,
                &output,
                &latency,
                chat,
                before,
                &wav_48k_mono(&wav3),
                Duration::from_secs(60),
            );
            // The agent's answer comes back through the voice model: wait for the task to
            // finish, then for the voice's retelling and the end of its audio.
            let deadline = Instant::now() + Duration::from_secs(120);
            let finished = |entries: &[crate::assistant::chat_log::ChatEntry]| {
                entries
                    .iter()
                    .skip(before)
                    .position(|entry| entry.text.starts_with("Codex finished the task"))
                    .map(|index| before + index)
            };
            while Instant::now() < deadline && finished(&chat.entries()).is_none() {
                std::thread::sleep(Duration::from_millis(200));
            }
            let done = finished(&chat.entries()).expect("the agent finished the task");
            while Instant::now() < deadline
                && !(chat
                    .entries()
                    .iter()
                    .skip(done)
                    .any(|entry| entry.kind == "assistant")
                    && output
                        .last_signal()
                        .is_some_and(|at| at.elapsed() > Duration::from_millis(2_500)))
            {
                std::thread::sleep(Duration::from_millis(200));
            }
            let entries = chat.entries();
            let agent = entries[before..done]
                .iter()
                .rev()
                .find(|entry| entry.kind == "agent")
                .map(|entry| entry.text.clone());
            let retelling: Vec<String> = entries[done..]
                .iter()
                .filter(|entry| entry.kind == "assistant")
                .map(|entry| entry.text.clone())
                .collect();
            let searches: Vec<String> = entries[before..]
                .iter()
                .filter(|entry| entry.kind == "tool" && entry.text.starts_with("Web search"))
                .map(|entry| entry.text.clone())
                .collect();
            eprintln!("third answer: {answer:?}; searches: {searches:?}");
            eprintln!("agent answer: {agent:?}");
            eprintln!("voice retelling: {retelling:?}");
            assert!(!searches.is_empty(), "the agent did not search the web");
            assert!(!retelling.is_empty(), "the voice retold the agent's answer");
            // One answer per completed task, the last one holding the agent's final text as
            // the chat log shows it.
            take_answers(&mut posted);
            let completed = entries
                .iter()
                .filter(|entry| entry.text.starts_with("Codex finished the task"))
                .count();
            eprintln!(
                "answers for posting: {}; tasks completed: {completed}",
                posted.len()
            );
            assert_eq!(posted.len(), completed, "one answer per completed task");
            let last = posted.last().expect("the delegated task's answer");
            assert_eq!(Some(&last.text), agent.as_ref(), "the agent's final text");
            assert!(last.task.is_some(), "the delegated task is named");
        }
        take_answers(&mut posted);
        session.session_wanted(false);
        std::thread::sleep(Duration::from_millis(500));
        stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = reader.join();
        // The whole call's audio counters (the line above was taken after the first answer).
        let last_audio = assistant.status().calls[0].audio.unwrap_or_default();
        eprintln!("{}", last_audio.log_line(1));
        // Toward Discord (change `transmit-only-when-speaking`): sending turned on for the
        // answers before their voice played, and nothing was sent while the call was silent.
        let toward_discord = output.counters();
        eprintln!(
            "toward Discord: sending turned on {} times, nothing sent for {} of {} frames, {} cut starts",
            toward_discord.sends_started,
            toward_discord.frames_not_sent,
            toward_discord.played + toward_discord.silence_frames,
            toward_discord.cut_starts
        );
        assert!(
            toward_discord.sends_started >= 1,
            "sending turned on for the answer"
        );
        assert_eq!(
            toward_discord.cut_starts, 0,
            "no answer started before sending was on"
        );
        assert!(
            toward_discord.frames_not_sent > 0,
            "nothing was sent while the call was silent"
        );
        if let Some(path) = &record {
            let lines = jitter::recording::finish();
            eprintln!(
                "recorded {lines} packets to {path}; the call received {}",
                last_audio.packets_received
            );
        }
        for entry in chat.entries() {
            eprintln!(
                "chat {} {} {:?}: {}",
                entry.at, entry.kind, entry.who, entry.text
            );
        }
        // Every chat entry reached the event bus as a `chat` event that keeps its kind.
        let mut sent = Vec::new();
        while let Ok(event) = window.try_recv() {
            if let AppEvent::Chat(_) = &event {
                let json = serde_json::to_value(&event).unwrap();
                assert_eq!(json["event"], "chat", "{json}");
                sent.push((
                    json["kind"].as_str().unwrap_or("").to_owned(),
                    json["id"].clone(),
                ));
            }
        }
        let recorded: Vec<(String, Value)> = chat
            .entries()
            .iter()
            .map(|entry| (entry.kind.to_owned(), json!(entry.id)))
            .collect();
        eprintln!(
            "chat events sent: {}, kinds {:?}",
            sent.len(),
            sent.iter()
                .map(|(kind, _)| kind.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            sent, recorded,
            "each recorded entry was sent once, with its kind"
        );
        let turns = latency.lock().unwrap().recent();
        for turn in &turns {
            let event = crate::latency::LatencyEvent::Report(turn.clone());
            eprintln!("{}", crate::latency::log_line(1, &event));
        }
        assert!(
            turns.iter().any(|turn| turn.kind == "response"),
            "the first answer was measured as a response: {turns:?}"
        );
        assistant.stop_blocking();
    }

    /// The Whisper mode's text sessions (change `whisper-mode`, task 7.2) against the fake.
    mod text {
        use super::*;
        use crate::assistant::{AgentAnswer, UserText};
        use crate::tts::manager::{Loader, TtsManager};
        use crate::tts::{Synth, TtsError};

        /// 0.2 s of a tone per piece, recording what it read.
        struct Reader(Arc<Mutex<Vec<String>>>);

        impl Synth for Reader {
            fn sample_rate(&self) -> u32 {
                24_000
            }

            fn synthesize(
                &mut self,
                text: &str,
                _: Option<&str>,
                _: f32,
                _: &dyn Fn() -> bool,
            ) -> Result<Option<Vec<f32>>, TtsError> {
                self.0.lock().unwrap().push(text.to_owned());
                Ok(Some(
                    (0..4_800)
                        .map(|index| (index as f32 * 0.1).sin() * 0.5)
                        .collect(),
                ))
            }
        }

        struct ReaderLoader(Arc<Mutex<Vec<String>>>);

        impl Loader for ReaderLoader {
            fn load(&self, _: &str, _: &std::path::Path) -> Result<Box<dyn Synth>, TtsError> {
                Ok(Box::new(Reader(Arc::clone(&self.0))))
            }
        }

        struct Rig {
            runtime: tokio::runtime::Runtime,
            dir: tempfile::TempDir,
            assistant: CodexAssistant,
            session: Arc<dyn AssistantSession>,
            answers: tokio::sync::mpsc::Receiver<AgentAnswer>,
            read: Arc<Mutex<Vec<String>>>,
            _source: crate::audio::PcmSource,
        }

        fn whisper_settings(executable: &str) -> Settings {
            let mut settings = Settings::default();
            settings.codex.executable = executable.to_owned();
            settings.assistant.mode = AssistantMode::Whisper;
            settings.tts.voice = "windows:test".to_owned();
            settings
        }

        fn rig() -> Option<Rig> {
            rig_with(false)
        }

        /// A Whisper-mode rig with one speaker at a time on or off (change `speaker-turns`).
        fn rig_with(one_at_a_time: bool) -> Option<Rig> {
            rig_on(None, one_at_a_time)
        }

        /// As [`rig_with`], on `executable` (the real Codex) instead of the logging fake.
        fn rig_on(executable: Option<String>, one_at_a_time: bool) -> Option<Rig> {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let fake = match executable {
                Some(executable) => executable,
                None => logging_fake(dir.path())?,
            };
            let assistant = assistant_with(&runtime, dir.path(), &fake);
            let mut settings = whisper_settings(&fake);
            settings.assistant.one_speaker_at_a_time = one_at_a_time;
            assistant.configure(&settings);
            let read = Arc::new(Mutex::new(Vec::new()));
            let tts = TtsManager::with_parts(
                Arc::new(ReaderLoader(Arc::clone(&read))),
                Arc::new(
                    |_: &crate::assets::Asset,
                     _: &std::path::Path,
                     _: &dyn Fn(crate::assets::Progress)| Ok(()),
                ),
                dir.path().to_path_buf(),
                EventBus::new(64),
            );
            tts.configure(&settings);
            tts.prepare_join().unwrap();
            read.lock().unwrap().clear();
            assistant.use_tts(tts);
            runtime
                .block_on(assistant.prepare_guild(1))
                .expect("the fake prepares");
            let (sink, answers) = crate::assistant::AnswerSink::channel();
            let (_sender, input) = frame_channel(INPUT_CAPACITY);
            let (output, source) = output_pair();
            let session = assistant
                .attach(
                    1,
                    AudioPorts {
                        input,
                        output,
                        failures: CodecFailures::none(),
                        answers: sink,
                        heard: crate::assistant::no_heard(),
                    },
                )
                .unwrap();
            session.call_context(
                Some("Alex".to_owned()),
                &["Alex".to_owned(), "Sam".to_owned()],
            );
            session.session_wanted(true);
            Some(Rig {
                runtime,
                dir,
                assistant,
                session,
                answers,
                read,
                _source: source,
            })
        }

        fn question(text: &str) -> UserText {
            UserText {
                speaker: Some("Alex".to_owned()),
                text: text.to_owned(),
                language: Some("en".to_owned()),
                speech_end: Instant::now(),
                text_at: Instant::now(),
                audio_seconds: 2.0,
                model: "whisper-base-q8_0".to_owned(),
                device: "cpu".to_owned(),
                latency: Arc::default(),
            }
        }

        fn wait(condition: impl Fn() -> bool) {
            let started = Instant::now();
            while !condition() {
                assert!(started.elapsed() < Duration::from_secs(10), "timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn methods(dir: &std::path::Path) -> Vec<String> {
            std::fs::read_to_string(dir.join("requests.txt"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        #[test]
        fn a_question_is_answered_spoken_and_posted() {
            let Some(mut rig) = rig() else {
                return;
            };
            assert!(rig.session.ready(), "ready once the thread exists");
            rig.session
                .user_text(question("what time is it in London?"));
            assert!(rig.session.working());
            let answer = rig
                .runtime
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(10), rig.answers.recv()).await
                })
                .expect("an answer in time")
                .unwrap();
            assert_eq!(answer.task.as_deref(), Some("what time is it in London?"));
            assert!(answer
                .text
                .starts_with("You asked: Alex: what time is it in London?"));
            wait(|| rig.read.lock().unwrap().len() >= 3);
            let read = rig.read.lock().unwrap().clone();
            assert_eq!(read[0], "Let me check that.", "the interim line is spoken");
            assert!(
                read.concat().contains("In London it is 9 degrees."),
                "{read:?}"
            );
            // The first turn carried the people in the channel.
            let params = rig
                .runtime
                .block_on(
                    rig.assistant
                        .running_server()
                        .unwrap()
                        .request("fake/lastTurnStart", serde_json::json!({})),
                )
                .unwrap();
            let channel = params["additionalContext"]["voicebro.channel"]["value"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(channel.contains("Alex, Sam"), "{channel}");
            assert_eq!(
                params["input"][0]["text"],
                "Alex: what time is it in London?"
            );
            rig.assistant.detach(1);
            rig.assistant.stop_blocking();
        }

        #[test]
        fn a_second_question_interrupts_the_first_and_only_it_is_posted() {
            let Some(mut rig) = rig() else {
                return;
            };
            let server = rig.assistant.running_server().unwrap();
            rig.runtime
                .block_on(server.request("fake/turnDelay", serde_json::json!({ "ms": 40 })))
                .unwrap();
            rig.session.user_text(question("first question"));
            wait(|| !rig.read.lock().unwrap().is_empty());
            rig.session.user_text(question("second question"));
            let answer = rig
                .runtime
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(10), rig.answers.recv()).await
                })
                .expect("an answer in time")
                .unwrap();
            assert_eq!(answer.task.as_deref(), Some("second question"));
            let calls = methods(rig.dir.path());
            let interrupt = calls.iter().position(|method| method == "turn/interrupt");
            let second_start = calls
                .iter()
                .enumerate()
                .filter(|(_, method)| *method == "turn/start")
                .nth(1)
                .map(|(index, _)| index);
            assert!(
                interrupt.is_some() && interrupt < second_start,
                "the interrupt goes out before the new start: {calls:?}"
            );
            std::thread::sleep(Duration::from_millis(300));
            assert!(
                rig.answers.try_recv().is_err(),
                "the interrupted turn is not posted"
            );
            rig.assistant.detach(1);
            rig.assistant.stop_blocking();
        }

        /// Change `speaker-turns`, scenario "Second question while Codex works": with one
        /// speaker at a time on, the second question joins the running turn with
        /// `turn/steer`, nothing is interrupted, and the posted answer names both questions.
        #[test]
        fn with_one_at_a_time_a_second_question_joins_the_running_turn() {
            let Some(mut rig) = rig_with(true) else {
                return;
            };
            let server = rig.assistant.running_server().unwrap();
            rig.runtime
                .block_on(server.request("fake/turnDelay", serde_json::json!({ "ms": 40 })))
                .unwrap();
            rig.session.user_text(question("first question"));
            wait(|| !rig.read.lock().unwrap().is_empty());
            rig.session.user_text(question("second question"));
            let answer = rig
                .runtime
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(10), rig.answers.recv()).await
                })
                .expect("an answer in time")
                .unwrap();
            assert_eq!(
                answer.task.as_deref(),
                Some("first question / second question")
            );
            let calls = methods(rig.dir.path());
            assert!(
                calls.iter().any(|method| method == "turn/steer"),
                "{calls:?}"
            );
            assert!(
                !calls.iter().any(|method| method == "turn/interrupt"),
                "{calls:?}"
            );
            assert_eq!(
                calls
                    .iter()
                    .filter(|method| *method == "turn/start")
                    .count(),
                1,
                "{calls:?}"
            );
            let steers = rig
                .runtime
                .block_on(server.request("fake/steers", serde_json::json!({})))
                .unwrap();
            assert_eq!(steers, serde_json::json!(["Alex: second question"]));
            rig.assistant.detach(1);
            rig.assistant.stop_blocking();
        }

        /// Scenario "The running turn ends at the same moment": a refused steer makes the
        /// question wait and start the next turn once the running one completes; both
        /// answers are posted, in order, and nothing is interrupted.
        #[test]
        fn with_one_at_a_time_a_refused_steer_becomes_the_next_turn() {
            let Some(mut rig) = rig_with(true) else {
                return;
            };
            let server = rig.assistant.running_server().unwrap();
            rig.runtime.block_on(async {
                server
                    .request("fake/turnDelay", serde_json::json!({ "ms": 40 }))
                    .await
                    .unwrap();
                server
                    .request("fake/refuseNextSteer", serde_json::json!({}))
                    .await
                    .unwrap();
            });
            rig.session.user_text(question("first question"));
            wait(|| !rig.read.lock().unwrap().is_empty());
            rig.session.user_text(question("second question"));
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let answer = rig
                    .runtime
                    .block_on(async {
                        tokio::time::timeout(Duration::from_secs(10), rig.answers.recv()).await
                    })
                    .expect("an answer in time")
                    .unwrap();
                tasks.push(answer.task.unwrap_or_default());
            }
            assert_eq!(tasks, ["first question", "second question"]);
            let calls = methods(rig.dir.path());
            assert!(
                !calls.iter().any(|method| method == "turn/interrupt"),
                "{calls:?}"
            );
            assert_eq!(
                calls
                    .iter()
                    .filter(|method| *method == "turn/start")
                    .count(),
                2,
                "{calls:?}"
            );
            // A deactivation still stops everything at once.
            rig.session.user_text(question("third question"));
            rig.session.session_wanted(false);
            assert!(!rig.session.working(), "the voice stopped at once");
            rig.assistant.detach(1);
            rig.assistant.stop_blocking();
        }

        /// Task 6.2 of change `speaker-turns`, when `VOICEBRO_CODEX_TWO_QUESTIONS=1`: against
        /// the real Codex (a reader stands in for the local voice), Alex's question and Sam's
        /// 1 s later, with one speaker at a time on; `VOICEBRO_CODEX_TWO_RUNS` runs (default
        /// 5). Prints per run the posted answers with their tasks, whether each was read to
        /// its end (no answer cut), whether both cities are answered, and the time to the
        /// first spoken piece.
        #[test]
        fn two_questions_one_at_a_time_when_enabled() {
            if std::env::var("VOICEBRO_CODEX_TWO_QUESTIONS")
                .ok()
                .as_deref()
                != Some("1")
            {
                return;
            }
            let runs: usize = std::env::var("VOICEBRO_CODEX_TWO_RUNS")
                .ok()
                .and_then(|runs| runs.parse().ok())
                .unwrap_or(5);
            let executable = super::super::app_server::find_codex("")
                .expect("codex is installed")
                .display()
                .to_string();
            let alnum = |text: &str| -> String {
                text.chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect()
            };
            for run in 1..=runs {
                let mut rig = rig_on(Some(executable.clone()), true).expect("a rig");
                let started = Instant::now();
                let mut alex = question("What time is it in Bangkok?");
                alex.speaker = Some("Alex".to_owned());
                rig.session.user_text(alex);
                std::thread::sleep(Duration::from_secs(1));
                let mut sam = question("And what is the weather in London?");
                sam.speaker = Some("Sam".to_owned());
                rig.session.user_text(sam);
                let mut first_piece = None;
                let mut answers: Vec<AgentAnswer> = Vec::new();
                let deadline = Instant::now() + Duration::from_secs(150);
                // Nobody pulls the voice here, so the end is when both questions have their
                // answer and the reader has read nothing new for 2 s.
                let (mut read_count, mut read_since) = (0, Instant::now());
                loop {
                    let count = rig.read.lock().unwrap().len();
                    if first_piece.is_none() && count > 0 {
                        first_piece = Some(started.elapsed());
                    }
                    if count != read_count {
                        (read_count, read_since) = (count, Instant::now());
                    }
                    while let Ok(answer) = rig.answers.try_recv() {
                        answers.push(answer);
                    }
                    let tasks: usize = answers
                        .iter()
                        .map(|answer| answer.task.as_deref().unwrap_or("").split(" / ").count())
                        .sum();
                    if (tasks >= 2 && read_since.elapsed() > Duration::from_secs(2))
                        || Instant::now() > deadline
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                let read = alnum(&rig.read.lock().unwrap().concat());
                let mut lines = Vec::new();
                for answer in &answers {
                    // A markdown link is read by its text only.
                    let spoken = answer
                        .text
                        .split("](")
                        .enumerate()
                        .map(|(index, part)| {
                            if index == 0 {
                                part
                            } else {
                                part.split_once(')').map_or("", |(_, rest)| rest)
                            }
                        })
                        .collect::<String>();
                    let text = alnum(&spoken);
                    let tail: String = text
                        .chars()
                        .rev()
                        .take(30)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    lines.push(format!(
                        "task {:?}, read to its end {}: {:?}",
                        answer.task,
                        read.contains(&tail),
                        answer.text
                    ));
                }
                let all = answers
                    .iter()
                    .map(|answer| answer.text.to_lowercase())
                    .collect::<String>();
                eprintln!(
                    "two questions run {run}: first spoken piece {} ms, done {} ms; both answered ({}, {}); {} answer(s): {}",
                    first_piece.map_or("-".to_owned(), |at| at.as_millis().to_string()),
                    started.elapsed().as_millis(),
                    all.contains("bangkok"),
                    all.contains("london"),
                    answers.len(),
                    lines.join(" || ")
                );
                rig.assistant.detach(1);
                rig.assistant.stop_blocking();
            }
        }

        #[test]
        fn a_deactivation_during_a_turn_interrupts_it_and_posts_nothing() {
            let Some(mut rig) = rig() else {
                return;
            };
            let server = rig.assistant.running_server().unwrap();
            rig.runtime
                .block_on(server.request("fake/turnDelay", serde_json::json!({ "ms": 40 })))
                .unwrap();
            rig.session.user_text(question("a long question"));
            wait(|| !rig.read.lock().unwrap().is_empty());
            rig.session.session_wanted(false);
            assert!(!rig.session.working(), "the voice stopped at once");
            wait(|| {
                methods(rig.dir.path())
                    .iter()
                    .any(|method| method == "turn/interrupt")
            });
            std::thread::sleep(Duration::from_millis(500));
            assert!(rig.answers.try_recv().is_err());
            rig.assistant.detach(1);
            rig.assistant.stop_blocking();
        }

        /// Task 7.4, gated: `VOICEBRO_WHISPER_DIR` (assets with the Whisper files and the
        /// voice), `VOICEBRO_WHISPER_QUESTION_WAV` (a question, 16 or 48 kHz), optional
        /// `VOICEBRO_WHISPER_MODEL`, `VOICEBRO_WHISPER_DEVICE`, `VOICEBRO_TTS_VOICE` and
        /// `VOICEBRO_WHISPER_ANSWER_WAV`. The WAV goes through Whisper, a real Codex text
        /// turn and the voice; the answer is written as a WAV, and the four timings print.
        #[test]
        fn whisper_text_turn_when_enabled() {
            let (Ok(assets), Ok(wav)) = (
                std::env::var("VOICEBRO_WHISPER_DIR"),
                std::env::var("VOICEBRO_WHISPER_QUESTION_WAV"),
            ) else {
                return;
            };
            let assets = std::path::PathBuf::from(assets);
            let model_id = std::env::var("VOICEBRO_WHISPER_MODEL")
                .unwrap_or_else(|_| crate::stt::catalog::DEFAULT_MODEL.to_owned());
            let device = match std::env::var("VOICEBRO_WHISPER_DEVICE").as_deref() {
                Ok("gpu") => crate::settings::WhisperDevice::Gpu,
                _ => crate::settings::WhisperDevice::Cpu,
            };
            let voice = std::env::var("VOICEBRO_TTS_VOICE")
                .unwrap_or_else(|_| crate::tts::catalog::DEFAULT_VOICE.to_owned());
            // Whisper.
            let entry = crate::stt::catalog::find(&model_id).expect("a catalog model");
            let runtime_lib = crate::stt::whisper::runtime(&assets).expect("the runtime");
            let choice = crate::stt::whisper::choose_device(&runtime_lib, device, &assets);
            let mut model = crate::stt::whisper::WhisperModel::load(
                runtime_lib,
                &assets.join(entry.asset.directory).join(entry.asset.marker),
                &assets
                    .join(crate::stt::catalog::VAD_MODEL.directory)
                    .join(crate::stt::catalog::VAD_MODEL.marker),
                choice.used == crate::settings::WhisperDevice::Gpu,
            )
            .expect("the model loads");
            let audio = crate::stt::read_wav_16k(std::path::Path::new(&wav)).expect("the WAV");
            let speech_end = Instant::now();
            let raw = crate::stt::SpeechToText::transcribe(
                &mut model,
                &audio,
                &crate::stt::DecodeOptions {
                    language: "auto".to_owned(),
                    threads: 0,
                    prompt: None,
                },
            )
            .expect("transcribed");
            let text_at = Instant::now();
            let crate::stt::transcriber::Cleaned::Text(text) = crate::stt::transcriber::clean(
                &raw,
                crate::stt::whisper::NO_SPEECH_THRESHOLD,
                audio.len() as f64 / 16_000.0,
            ) else {
                panic!("Whisper heard nothing in {wav}");
            };
            eprintln!("Whisper ({model_id}, {}): «{text}»", choice.backend);
            // Codex and the voice.
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let mut settings = Settings::default();
            // VOICEBRO_ASSISTANT_MODE=text runs the turn in the text answers mode (change
            // `text-answers-mode`): the answer is posted and nothing is spoken.
            let text_answers = std::env::var("VOICEBRO_ASSISTANT_MODE").as_deref() == Ok("text");
            settings.assistant.mode = if text_answers {
                AssistantMode::Text
            } else {
                AssistantMode::Whisper
            };
            settings.tts.voice = voice.clone();
            let events = EventBus::new(256);
            let assistant = CodexAssistant::new(
                runtime.handle().clone(),
                events.clone(),
                Arc::new(ConnectionErrors::new(events.clone())),
                dir.path(),
                &settings,
            );
            let tts = TtsManager::new(assets.clone(), events);
            tts.configure(&settings);
            tts.prepare_join().expect("the voice loads");
            assistant.use_tts(tts.clone());
            runtime
                .block_on(assistant.prepare_guild(1))
                .expect("codex runs and is signed in");
            let (sink, mut answers) = crate::assistant::AnswerSink::channel();
            let (_sender, input) = frame_channel(INPUT_CAPACITY);
            let (output, mut source) = output_pair();
            let session = assistant
                .attach(
                    1,
                    AudioPorts {
                        input,
                        output,
                        failures: CodecFailures::none(),
                        answers: sink,
                        heard: crate::assistant::no_heard(),
                    },
                )
                .unwrap();
            // VOICEBRO_WHISPER_PEOPLE="Alex, Sam": the channel's people, the first as the
            // caller, for the additionalContext canary (task 7.3).
            if let Ok(people) = std::env::var("VOICEBRO_WHISPER_PEOPLE") {
                let names: Vec<String> = people
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
                session.call_context(names.first().cloned(), &names);
            }
            session.session_wanted(true);
            let latency: crate::latency::SharedLatency = Arc::default();
            session.user_text(UserText {
                speaker: Some("Tester".to_owned()),
                text,
                language: raw.language.clone(),
                speech_end,
                text_at,
                audio_seconds: audio.len() as f64 / 16_000.0,
                model: model_id.clone(),
                device: choice.used.name().to_owned(),
                latency: Arc::clone(&latency),
            });
            // Read the voice as songbird would, every 20 ms, until the answer was posted and
            // the voice is quiet.

            let mut voice_audio: Vec<f32> = Vec::new();
            let mut buffer = vec![0u8; FRAME_SAMPLES * 4];
            let started = Instant::now();
            let mut posted = None;
            let mut voiced = false;
            let mut quiet_since: Option<Instant> = None;
            while started.elapsed() < Duration::from_secs(120) {
                std::thread::sleep(Duration::from_millis(20));
                let read = std::io::Read::read(&mut source, &mut buffer).unwrap_or(0);
                let mut loud = false;
                for chunk in buffer[..read].as_chunks::<8>().0 {
                    let left = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    loud |= left.abs() > 0.01;
                    voice_audio.push(left);
                }
                if let Ok(answer) = answers.try_recv() {
                    posted = Some(answer);
                }
                if loud {
                    voiced = true;
                    quiet_since = None;
                } else if voiced {
                    quiet_since.get_or_insert_with(Instant::now);
                }
                // Done once the answer was posted and its voice played and went quiet; in the
                // text answers mode 2 s after the post, to hear that nothing follows.
                if posted.is_some()
                    && quiet_since.is_some_and(|since| since.elapsed() > Duration::from_secs(2))
                {
                    break;
                }
                if text_answers && posted.is_some() {
                    let since = *quiet_since.get_or_insert_with(Instant::now);
                    if since.elapsed() > Duration::from_secs(2) {
                        break;
                    }
                }
            }
            if text_answers {
                assert!(!voiced, "the text answers mode played a voice");
                assert_eq!(tts.status().spoken, 0, "nothing was synthesized");
            }
            eprintln!("voice: {:?}", tts.status());
            for entry in assistant.chat_log().entries() {
                eprintln!("chat {}: {}", entry.kind, entry.text);
            }
            let posted = posted.expect("Codex answered");
            eprintln!("Codex answered: {}", posted.text);
            for report in latency.lock().unwrap().recent() {
                eprintln!(
                    "{}",
                    crate::latency::log_line(1, &crate::latency::LatencyEvent::Report(report))
                );
            }
            if let Ok(out) = std::env::var("VOICEBRO_WHISPER_ANSWER_WAV") {
                std::fs::write(&out, crate::tts::wav_bytes(&voice_audio, 48_000)).unwrap();
                eprintln!("wrote {out}");
            }
            assert!(
                !latency.lock().unwrap().recent().is_empty(),
                "the turn was measured"
            );
            assistant.detach(1);
            assistant.stop_blocking();
        }
    }
}
