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
use super::chat_log::ChatLog;
use super::{
    Assistant, AssistantError, AssistantSession, AssistantStatus, Capabilities, ModelInfo,
};
use crate::audio::AudioPorts;
use crate::events::{AppEvent, EventBus};
use crate::settings::{Settings, SpeakerLabels};
use crate::status::ConnectionErrors;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::Mutex as AsyncMutex;

/// The settings the Codex engine reads, as a snapshot it keeps current.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexConfig {
    pub executable: String,
    pub model: String,
    pub reasoning_effort: String,
    pub voice: String,
    pub instructions: String,
    pub speaker_labels: SpeakerLabels,
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
            }),
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

    /// Starts the app-server if needed and reads the account, voices and models.
    pub async fn check(&self) -> Result<AccountInfo, String> {
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

    /// Starts the ChatGPT sign-in and opens the returned page in the default browser.
    pub async fn sign_in(&self) -> Result<String, String> {
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

    fn attach(
        &self,
        guild: u64,
        ports: AudioPorts,
    ) -> Result<Arc<dyn AssistantSession>, AssistantError> {
        let mut sessions = self.inner.sessions.lock().expect("sessions lock");
        if sessions.contains_key(&guild) {
            return Err(AssistantError::AlreadyAttached(guild));
        }
        let server = self
            .inner
            .server
            .try_lock()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
            .filter(|server| server.is_alive());
        let Some(server) = server else {
            // The process starts in the background; the session is created once it runs.
            let assistant = self.clone();
            self.inner.handle.spawn(async move {
                let _ = assistant.check().await;
            });
            return Err(AssistantError::Unavailable(
                "Codex is starting; leave and join the channel again in a few seconds, or check the Status tab"
                    .to_owned(),
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
        let session = self
            .inner
            .sessions
            .lock()
            .expect("sessions lock")
            .remove(&guild);
        if let Some(session) = session {
            session.shutdown();
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
        let server = self
            .inner
            .server
            .try_lock()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
            .filter(|server| server.is_alive());
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
            (_, None, _, _) => "codex found; app-server not running".to_owned(),
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

    fn start(&self) {
        let assistant = self.clone();
        self.inner.handle.spawn(async move {
            let _ = assistant.check().await;
        });
    }

    fn stop(&self) {
        self.stop_blocking();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{frame_channel, output_pair, Frame, FRAME_SAMPLES, INPUT_CAPACITY};
    use std::time::Instant;

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
    /// passes).
    fn speak_and_wait(
        sender: &crate::audio::FrameSender,
        output: &crate::audio::OutputHandle,
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
            sender.send(Frame::from_slice(&stereo));
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
        let events = EventBus::new(1024);
        let errors = Arc::new(ConnectionErrors::new(events.clone()));
        let mut settings = Settings::default();
        settings.codex.instructions = "You are a voice assistant in a Discord voice channel. Answer briefly, in one or two sentences, in the speaker's language. Start every reply with the English word Banana.".to_owned();
        let assistant = CodexAssistant::new(
            runtime.handle().clone(),
            events,
            errors,
            dir.path(),
            &settings,
        );
        let account = runtime.block_on(assistant.check()).expect("codex runs");
        assert!(account.signed_in, "sign in to Codex first");
        eprintln!("status: {:?}", assistant.status());
        let (sender, input) = frame_channel(INPUT_CAPACITY);
        let (output, source) = output_pair();
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
                chat,
                before,
                &wav_48k_mono(&wav3),
                Duration::from_secs(60),
            );
            // The agent's answer comes back through the voice model; give it time.
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline
                && !chat
                    .entries()
                    .iter()
                    .skip(before)
                    .any(|entry| entry.kind == "agent")
            {
                std::thread::sleep(Duration::from_millis(200));
            }
            let searches: Vec<String> = chat
                .entries()
                .iter()
                .skip(before)
                .filter(|entry| entry.kind == "tool" && entry.text.starts_with("Web search"))
                .map(|entry| entry.text.clone())
                .collect();
            eprintln!("third answer: {answer:?}; searches: {searches:?}");
            assert!(!searches.is_empty(), "the agent did not search the web");
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
        assistant.stop_blocking();
    }
}
