//! The Codex assistant: ChatGPT voice through `codex app-server`, the realtime call over
//! WebRTC inside the process. See `docs/ARCHITECTURE.md` 4.5.

pub mod account;
pub mod app_server;
pub mod conversation;
pub mod jitter;
pub mod pacing;
pub mod rtc;
pub mod session;
pub mod thread;

use self::account::{next_cursor, parse_account, parse_models, AccountInfo, Voices};
use self::app_server::{codex_version, find_codex, AppServer};
use self::conversation::Conversation;
use self::session::{CodexSession, SessionParts};
use self::thread::{open_thread, ThreadSettings};
use super::chat_log::ChatLog;
use super::{
    Assistant, AssistantError, AssistantSession, AssistantStatus, Capabilities, ModelInfo,
    PrepareFuture,
};
use crate::audio::AudioPorts;
use crate::events::{AppEvent, EventBus};
use crate::settings::{Settings, SpeakerLabels};
use crate::status::ConnectionErrors;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

/// Why a join was refused. The message is the reply the caller sees in Discord.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrepareError {
    #[error("Codex is not installed: install Codex CLI (npm install -g @openai/codex) or set its path in the VoiceBro settings")]
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
}

impl CodexConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            executable: settings.codex.executable.clone(),
            model: settings.codex.model.clone(),
            reasoning_effort: settings.codex.reasoning_effort.clone(),
            voice: settings.codex.voice.clone(),
            instructions: settings.codex.instructions.clone(),
            speaker_labels: settings.assistant.speaker_labels,
            handoff_mode: settings.codex.handoff_mode.clone(),
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
    sessions: Mutex<HashMap<u64, Arc<CodexSession>>>,
    /// Survives leaving and rejoining voice; lives as long as the app runs.
    conversations: Mutex<HashMap<u64, Arc<Mutex<Conversation>>>>,
    account: Arc<Mutex<Option<AccountInfo>>>,
    voices: Arc<Mutex<Option<Voices>>>,
    models: Mutex<Vec<ModelInfo>>,
    last_error: Mutex<Option<String>>,
    /// Joins being prepared right now; the app-server is not stopped under them.
    preparing: AtomicUsize,
    /// Bumped by every scheduled stop and every preparation; a stop timer that finds a
    /// newer value does nothing.
    stop_epoch: AtomicU64,
    /// When the last ChatGPT sign-in was started, until its server reports completion.
    sign_in_started: Mutex<Option<Instant>>,
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
                conversations: Mutex::new(HashMap::new()),
                account: Arc::new(Mutex::new(None)),
                voices: Arc::new(Mutex::new(None)),
                models: Mutex::new(Vec::new()),
                last_error: Mutex::new(None),
                preparing: AtomicUsize::new(0),
                stop_epoch: AtomicU64::new(0),
                sign_in_started: Mutex::new(None),
            }),
        }
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
        self.refresh_voices(&server).await;
        if self.inner.models.lock().expect("models lock").is_empty() {
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
        let executable = find_codex(&self.config().executable).ok_or_else(|| {
            "codex not found: install Codex CLI (npm install -g @openai/codex) or set its path in Settings"
                .to_owned()
        })?;
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

    async fn refresh_voices(&self, server: &AppServer) {
        if self.inner.voices.lock().expect("voices lock").is_some() {
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
        *self.inner.models.lock().expect("models lock") = models.clone();
        Ok(models)
    }

    /// Starts the app-server if needed and reads the account, voices and models. Outside a
    /// voice channel the server stops again a minute later.
    pub async fn check(&self) -> Result<AccountInfo, String> {
        self.after_on_demand_use();
        let result = async {
            let server = self.server().await?;
            let account = self.refresh_account(&server).await?;
            self.refresh_voices(&server).await;
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
        let session = CodexSession::new(SessionParts {
            guild,
            handle: self.inner.handle.clone(),
            server,
            config: Arc::clone(&self.inner.config),
            account: Arc::clone(&self.inner.account),
            voices: Arc::clone(&self.inner.voices),
            conversation: self.conversation(guild),
            workspace: self.inner.workspace.clone(),
            ports,
            chat: Arc::clone(&self.inner.chat),
            events: self.inner.events.clone(),
            errors: Arc::clone(&self.inner.errors),
        });
        sessions.insert(guild, Arc::clone(&session));
        Ok(session)
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
        }
    }

    fn status(&self) -> AssistantStatus {
        let config = self.config();
        let executable = find_codex(&config.executable);
        let server = self.running_server();
        let version = server
            .as_ref()
            .and_then(|server| server.version.clone())
            .or_else(|| executable.as_ref().and_then(|path| codex_version(path)));
        let account = self.inner.account.lock().expect("account lock").clone();
        let error = self
            .inner
            .last_error
            .lock()
            .expect("last error lock")
            .clone();
        let signed_in = account.as_ref().is_some_and(|account| account.signed_in);
        let summary = match (&executable, &server, &account, &error) {
            (None, _, _, _) => "codex not found".to_owned(),
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
    fn a_join_is_refused_without_codex_or_without_a_sign_in() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let missing = assistant_with(&runtime, dir.path(), "Z:/nowhere/codex.exe");
        assert_eq!(
            runtime.block_on(missing.prepare_guild(1)),
            Err(PrepareError::NotFound)
        );
        assert!(!missing.status().running);
        assert!(PrepareError::NotFound.to_string().contains("install Codex"));

        let Some(signed_out) = fake_codex("fake_codex_signed_out.cmd") else {
            return;
        };
        let assistant = assistant_with(&runtime, dir.path(), &signed_out);
        let refused = runtime.block_on(assistant.prepare_guild(1));
        assert_eq!(refused, Err(PrepareError::SignedOut));
        assert!(refused.unwrap_err().to_string().contains("Sign in"));
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
        drop(assistant.attach(1, AudioPorts { input, output }).unwrap());
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
            .attach(1, AudioPorts { input, output })
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

    /// Reads the output every 20 ms like songbird's mixer, counting the frames with signal,
    /// until `stop` is set.
    fn spawn_reader(
        mut source: crate::audio::PcmSource,
        stop: Arc<std::sync::atomic::AtomicBool>,
        voiced: Arc<std::sync::atomic::AtomicU64>,
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
        settings.codex.instructions = "You are a voice assistant in a Discord voice channel. Answer briefly, in one or two sentences, in the speaker's language. Start every reply with the English word Banana.".to_owned();
        // The answer handoff mode under test; the settings default otherwise.
        if let Ok(mode) = std::env::var("VOICEBRO_CODEX_HANDOFF_MODE") {
            settings.codex.handoff_mode = mode;
            settings
                .validate()
                .expect("VOICEBRO_CODEX_HANDOFF_MODE is a handoff mode");
        }
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
        let reader = spawn_reader(source, Arc::clone(&stop), Arc::clone(&voiced));
        let session = assistant
            .attach(
                1,
                AudioPorts {
                    input,
                    output: output.clone(),
                },
            )
            .expect("attach");
        session.call_context(
            Some("Alex".to_owned()),
            &["Alex".to_owned(), "Bob".to_owned()],
        );
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
        }
        session.session_wanted(false);
        std::thread::sleep(Duration::from_millis(500));
        stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = reader.join();
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
}
