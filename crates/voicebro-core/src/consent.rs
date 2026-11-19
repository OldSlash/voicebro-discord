//! The consent records (change `voice-consent`, design D2 and D3): who ran `/optin` and
//! pressed "I agree", with the text they agreed to.
//!
//! A record is the person's own act in Discord: the account id is the one Discord put in the
//! button press. The records live in `consents.bin` in the app's folder, encrypted with
//! DPAPI for the current Windows user ([`crate::protect`]); they never go into
//! `settings.json`. A record of another consent text version or another assistant engine is
//! deleted, since the consent names who receives the speech.

use crate::events::{AppEvent, EventBus};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// The consent text people agree to now. Raising it deletes every record of an older one.
pub const CONSENT_TEXT_VERSION: u32 = 1;

const FILE_VERSION: u32 = 1;
/// The DPAPI entropy of `consents.bin`.
const ENTROPY: &[u8] = b"VoiceBro consents v1";

/// Every consent text ever published, by version, so that what a record agreed to can
/// always be shown. `{provider}` and `{url}` are filled in when it is shown.
const CONSENT_TEXTS: &[(u32, &str)] = &[(
    1,
    "This bot runs an AI assistant. If you agree:\n\
     - After someone in the channel says the activation phrase, what you say is sent to \
     {provider} so that the assistant can answer: your voice in the voice mode, or, in the \
     Whisper mode, the text of your speech written down on the operator's PC.\n\
     - Your Discord id, your name and the time you agreed are kept on the operator's PC \
     until you withdraw.\n\
     - /optout withdraws at once. What you said before was already sent and cannot be \
     called back.\n\
     Privacy policy: {url}",
)];

/// The consent text of `version` for an engine's provider and the policy URL; `None` for a
/// version that was never published.
pub fn consent_text(version: u32, provider: &str, url: &str) -> Option<String> {
    CONSENT_TEXTS
        .iter()
        .find(|(number, _)| *number == version)
        .map(|(_, text)| text.replace("{provider}", provider).replace("{url}", url))
}

/// The recipient a record is made for (change `claude-and-opencode-engines`, design D10):
/// `codex`, `claude`, or `opencode:<provider>` with the provider of OpenCode's model in use
/// (`opencode:default` while OpenCode picks its own). A record of another key is deleted, so
/// a change of the engine or of OpenCode's provider ends the records.
pub fn recipient_key(settings: &crate::settings::Settings) -> String {
    use crate::settings::AssistantEngine;
    match settings.assistant.engine {
        AssistantEngine::Codex => "codex".to_owned(),
        AssistantEngine::Claude => "claude".to_owned(),
        AssistantEngine::Opencode => format!(
            "opencode:{}",
            settings.opencode.provider().unwrap_or("default")
        ),
    }
}

/// Who receives the speech for a recipient key, as the consent text names it. The Codex
/// wording is the one of build 1.2.0, so its records stay valid.
pub fn provider_name(key: &str) -> String {
    match key {
        "codex" => "OpenAI, through Codex".to_owned(),
        "claude" => "Anthropic, through Claude Code".to_owned(),
        _ => match key.strip_prefix("opencode:") {
            Some(provider) => format!(
                "{}, through OpenCode, and, for the web searches the assistant makes, \
                 OpenCode's web search service",
                opencode_company(provider)
            ),
            None => "the assistant's provider".to_owned(),
        },
    }
}

/// The company behind an OpenCode provider id, for the consent text; an id not in the
/// table is named as it is.
fn opencode_company(provider: &str) -> String {
    let company = match provider {
        // No model chosen: OpenCode's default, which may be one of its own free models.
        "default" => {
            "the AI provider set up in OpenCode (or, for its free models, the OpenCode \
             project's own service)"
        }
        "openai" => "OpenAI",
        "anthropic" => "Anthropic",
        "google" => "Google",
        "google-vertex" => "Google Cloud (Vertex AI)",
        "amazon-bedrock" => "Amazon Web Services (Bedrock)",
        "azure" => "Microsoft (Azure)",
        "github-copilot" => "GitHub (Copilot)",
        "openrouter" => "OpenRouter",
        "opencode" => "the OpenCode project (OpenCode Zen)",
        "mistral" => "Mistral AI",
        "xai" => "xAI",
        "groq" => "Groq",
        "deepseek" => "DeepSeek",
        other => return format!("the provider «{other}»"),
    };
    company.to_owned()
}

/// The records file in the app's folder.
pub fn consents_path(data_dir: &Path) -> PathBuf {
    data_dir.join("consents.bin")
}

/// One person who agreed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub discord_user_id: u64,
    /// The name at the moment of agreeing; a label only, the id decides.
    pub discord_name: String,
    /// RFC 3339, UTC.
    pub agreed_at: String,
    pub text_version: u32,
    /// The assistant engine the consent names (`codex`).
    pub assistant: String,
}

#[derive(Serialize, Deserialize)]
struct ConsentFile {
    version: u32,
    records: Vec<ConsentRecord>,
}

/// A record as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsentView {
    /// The Discord user id, as a string (JavaScript numbers lose its last digits).
    pub id: String,
    pub name: String,
    pub agreed_at: String,
}

/// Everything the window needs for the list in the Access card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ConsentStatus {
    pub records: Vec<ConsentView>,
    /// Why the records file could not be read at start, while that stands.
    pub unreadable: Option<String>,
}

/// What "I agree" led to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgreeOutcome {
    Recorded,
    /// A record for the current text and engine exists already; its time.
    AlreadyAgreed {
        agreed_at: String,
    },
    /// The button belongs to an older consent text.
    OldText,
}

/// What `/optout` or Remove led to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawOutcome {
    Removed { name: String },
    NothingToRemove,
}

/// Where the records are kept.
pub trait ConsentStore: Send + Sync {
    /// An empty list when there is no file; an error when a file is there but unreadable.
    fn load(&self) -> Result<Vec<ConsentRecord>, String>;
    /// Replaces every record; an empty list deletes the file.
    fn save(&self, records: &[ConsentRecord]) -> Result<(), String>;
    /// Where it is, for the log.
    fn describe(&self) -> String;
}

/// For tests and platforms without DPAPI: the records last as long as the process.
#[derive(Default)]
pub struct MemoryConsentStore {
    records: Mutex<Vec<ConsentRecord>>,
}

impl ConsentStore for MemoryConsentStore {
    fn load(&self) -> Result<Vec<ConsentRecord>, String> {
        Ok(self.records.lock().expect("memory consent lock").clone())
    }
    fn save(&self, records: &[ConsentRecord]) -> Result<(), String> {
        *self.records.lock().expect("memory consent lock") = records.to_vec();
        Ok(())
    }
    fn describe(&self) -> String {
        "memory".to_owned()
    }
}

/// `consents.bin`: `{"version": 1, "records": [...]}` encrypted with DPAPI for the current
/// Windows user, written through `consents.bin.tmp` and a rename.
pub struct FileConsentStore {
    path: PathBuf,
}

impl FileConsentStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ConsentStore for FileConsentStore {
    fn load(&self) -> Result<Vec<ConsentRecord>, String> {
        let Some(plain) = crate::protect::read_sealed(&self.path, ENTROPY)? else {
            return Ok(Vec::new());
        };
        let file: ConsentFile = serde_json::from_slice(&plain)
            .map_err(|error| format!("not a list of consents: {error}"))?;
        if file.version != FILE_VERSION {
            return Err(format!("unknown version {}", file.version));
        }
        Ok(file.records)
    }

    fn save(&self, records: &[ConsentRecord]) -> Result<(), String> {
        if records.is_empty() {
            return crate::protect::delete_sealed(&self.path);
        }
        let plain = serde_json::to_vec(&ConsentFile {
            version: FILE_VERSION,
            records: records.to_vec(),
        })
        .map_err(|error| error.to_string())?;
        crate::protect::write_sealed(&self.path, &plain, ENTROPY)
    }

    fn describe(&self) -> String {
        self.path.display().to_string()
    }
}

/// The store this build uses: the DPAPI file on Windows, memory elsewhere.
pub fn platform_store(data_dir: &Path) -> Arc<dyn ConsentStore> {
    if cfg!(windows) {
        Arc::new(FileConsentStore::new(consents_path(data_dir)))
    } else {
        Arc::new(MemoryConsentStore::default())
    }
}

type Listener = Box<dyn Fn() + Send + Sync>;

struct Inner {
    records: Vec<ConsentRecord>,
    unreadable: Option<String>,
    /// The assistant engine records are kept for.
    assistant: String,
}

/// The consent records the runtime, the Discord adapter and the window share.
pub struct ConsentState {
    inner: Mutex<Inner>,
    /// The ids of the records, replaced whole on every change; read by the access filter.
    ids: RwLock<Arc<HashSet<u64>>>,
    store: Arc<dyn ConsentStore>,
    events: EventBus,
    /// Called after every change, outside the lock (the access filter's refresh).
    listener: OnceLock<Listener>,
}

impl ConsentState {
    /// Reads the records for `assistant`; an unreadable file is no records, logged once with
    /// its reason. Records of another text version or engine are deleted (D2).
    pub fn new(store: Arc<dyn ConsentStore>, events: EventBus, assistant: &str) -> Self {
        let mut unreadable = None;
        let records = match store.load() {
            Ok(records) => records,
            Err(reason) => {
                log::warn!(
                    "consent: {} could not be read ({reason}); nobody is recorded",
                    store.describe()
                );
                unreadable = Some(format!("the consent records could not be read ({reason})"));
                Vec::new()
            }
        };
        let state = Self {
            inner: Mutex::new(Inner {
                records,
                unreadable,
                assistant: assistant.to_owned(),
            }),
            ids: RwLock::new(Arc::new(HashSet::new())),
            store,
            events,
            listener: OnceLock::new(),
        };
        state.drop_stale();
        state.refresh_ids();
        state
    }

    /// Sets the function called after every change; only the first call counts.
    pub fn set_listener(&self, listener: Listener) {
        let _ = self.listener.set(listener);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("consent lock")
    }

    /// Deletes the records of another text version or engine, with one log line.
    fn drop_stale(&self) -> bool {
        let mut inner = self.lock();
        let before = inner.records.len();
        let assistant = inner.assistant.clone();
        let text_changed = inner
            .records
            .iter()
            .any(|record| record.text_version != CONSENT_TEXT_VERSION);
        inner.records.retain(|record| {
            record.text_version == CONSENT_TEXT_VERSION && record.assistant == assistant
        });
        let removed = before - inner.records.len();
        if removed == 0 {
            return false;
        }
        log::info!(
            "consent: {removed} record(s) removed because the {} changed; those people run /optin again",
            if text_changed {
                "consent text"
            } else {
                "assistant engine"
            }
        );
        if let Err(error) = self.store.save(&inner.records) {
            log::warn!("consent: the records could not be saved: {error}");
        }
        true
    }

    fn refresh_ids(&self) {
        let ids: HashSet<u64> = self
            .lock()
            .records
            .iter()
            .map(|record| record.discord_user_id)
            .collect();
        *self.ids.write().expect("consent ids lock") = Arc::new(ids);
    }

    /// After a change: the ids, the window's event, the listener.
    fn changed(&self) {
        self.refresh_ids();
        self.events
            .publish(AppEvent::Consent(Box::new(self.status())));
        if let Some(listener) = self.listener.get() {
            listener();
        }
    }

    /// Writes the records; a failure is logged and the records stay in memory.
    fn save(&self, records: &[ConsentRecord]) {
        if let Err(error) = self.store.save(records) {
            log::warn!("consent: the records could not be saved: {error}");
        }
    }

    /// The ids of everyone recorded.
    pub fn ids(&self) -> Arc<HashSet<u64>> {
        Arc::clone(&self.ids.read().expect("consent ids lock"))
    }

    /// The record of `user`, if any.
    pub fn record(&self, user: u64) -> Option<ConsentRecord> {
        self.lock()
            .records
            .iter()
            .find(|record| record.discord_user_id == user)
            .cloned()
    }

    pub fn status(&self) -> ConsentStatus {
        let inner = self.lock();
        ConsentStatus {
            records: inner
                .records
                .iter()
                .map(|record| ConsentView {
                    id: record.discord_user_id.to_string(),
                    name: record.discord_name.clone(),
                    agreed_at: record.agreed_at.clone(),
                })
                .collect(),
            unreadable: inner.unreadable.clone(),
        }
    }

    /// The assistant engine changed: records of another one are deleted.
    pub fn set_assistant(&self, assistant: &str) {
        {
            let mut inner = self.lock();
            if inner.assistant == assistant {
                return;
            }
            inner.assistant = assistant.to_owned();
        }
        if self.drop_stale() {
            self.changed();
        }
    }

    /// "I agree" pressed under the consent text of `text_version`.
    pub fn agree(&self, user: u64, name: &str, text_version: u32, now: &str) -> AgreeOutcome {
        if text_version != CONSENT_TEXT_VERSION {
            return AgreeOutcome::OldText;
        }
        {
            let mut inner = self.lock();
            if let Some(record) = inner
                .records
                .iter()
                .find(|record| record.discord_user_id == user)
            {
                return AgreeOutcome::AlreadyAgreed {
                    agreed_at: record.agreed_at.clone(),
                };
            }
            let record = ConsentRecord {
                discord_user_id: user,
                discord_name: name.to_owned(),
                agreed_at: now.to_owned(),
                text_version,
                assistant: inner.assistant.clone(),
            };
            inner.records.push(record);
            inner.unreadable = None;
            self.save(&inner.records);
        }
        log::info!("consent: {name} agreed (text v{text_version})");
        self.changed();
        AgreeOutcome::Recorded
    }

    /// `/optout` from `user`, or Remove in the window (`by_window`).
    pub fn withdraw(&self, user: u64, by_window: bool) -> WithdrawOutcome {
        let name = {
            let mut inner = self.lock();
            let Some(index) = inner
                .records
                .iter()
                .position(|record| record.discord_user_id == user)
            else {
                return WithdrawOutcome::NothingToRemove;
            };
            let record = inner.records.remove(index);
            inner.unreadable = None;
            self.save(&inner.records);
            record.discord_name
        };
        if by_window {
            log::info!("consent: {name} removed in the window");
        } else {
            log::info!("consent: {name} withdrew");
        }
        self.changed();
        WithdrawOutcome::Removed { name }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(user: u64, version: u32, assistant: &str) -> ConsentRecord {
        ConsentRecord {
            discord_user_id: user,
            discord_name: format!("user{user}"),
            agreed_at: "2026-11-18T10:00:00Z".to_owned(),
            text_version: version,
            assistant: assistant.to_owned(),
        }
    }

    #[test]
    fn agree_withdraw_and_again() {
        let store: Arc<dyn ConsentStore> = Arc::new(MemoryConsentStore::default());
        let state = ConsentState::new(Arc::clone(&store), EventBus::default(), "codex");
        let mut events = state.events.subscribe();
        assert_eq!(
            state.agree(7, "Alex", CONSENT_TEXT_VERSION, "2026-11-18T10:00:00Z"),
            AgreeOutcome::Recorded
        );
        assert!(state.ids().contains(&7));
        assert!(matches!(events.try_recv(), Ok(AppEvent::Consent(_))));
        assert_eq!(
            state.agree(7, "Alex", CONSENT_TEXT_VERSION, "later"),
            AgreeOutcome::AlreadyAgreed {
                agreed_at: "2026-11-18T10:00:00Z".to_owned()
            }
        );
        assert_eq!(
            state.agree(8, "Sam", CONSENT_TEXT_VERSION - 1, "now"),
            AgreeOutcome::OldText
        );
        assert!(!state.ids().contains(&8));
        assert_eq!(store.load().unwrap().len(), 1);
        assert_eq!(
            state.withdraw(7, false),
            WithdrawOutcome::Removed {
                name: "Alex".to_owned()
            }
        );
        assert!(state.ids().is_empty());
        assert_eq!(state.withdraw(7, false), WithdrawOutcome::NothingToRemove);
        assert!(store.load().unwrap().is_empty());
    }

    #[test]
    fn the_listener_runs_after_each_change() {
        let state = ConsentState::new(
            Arc::new(MemoryConsentStore::default()),
            EventBus::default(),
            "codex",
        );
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        state.set_listener(Box::new(move || {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        state.agree(1, "A", CONSENT_TEXT_VERSION, "t");
        state.agree(2, "B", CONSENT_TEXT_VERSION, "t");
        state.withdraw(1, true);
        state.withdraw(1, true);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn stale_records_are_dropped_at_start_and_on_an_engine_change() {
        let store: Arc<dyn ConsentStore> = Arc::new(MemoryConsentStore::default());
        store
            .save(&[
                record(1, CONSENT_TEXT_VERSION, "codex"),
                record(2, CONSENT_TEXT_VERSION + 1, "codex"),
                record(3, CONSENT_TEXT_VERSION, "other"),
            ])
            .unwrap();
        let state = ConsentState::new(Arc::clone(&store), EventBus::default(), "codex");
        assert_eq!(state.ids().iter().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(store.load().unwrap().len(), 1, "the file is rewritten");
        state.set_assistant("claude");
        assert!(state.ids().is_empty());
        assert!(store.load().unwrap().is_empty());
    }

    /// Change `claude-and-opencode-engines`, D10: the recipient follows the engine and
    /// OpenCode's provider, and the Codex text is the one of 1.2.0 word for word.
    #[test]
    fn the_recipient_follows_the_engine_and_the_codex_text_is_unchanged() {
        use crate::settings::{AssistantEngine, Settings};
        let mut settings = Settings::default();
        assert_eq!(recipient_key(&settings), "codex");
        let codex = consent_text(1, &provider_name("codex"), "https://example.org/p").unwrap();
        assert!(codex.starts_with(
            "This bot runs an AI assistant. If you agree:\n- After someone in the channel says \
             the activation phrase, what you say is sent to OpenAI, through Codex so that"
        ), "{codex}");

        settings.assistant.engine = AssistantEngine::Claude;
        assert_eq!(recipient_key(&settings), "claude");
        let claude = consent_text(1, &provider_name("claude"), "u").unwrap();
        assert!(
            claude.contains("Anthropic, through Claude Code"),
            "{claude}"
        );
        assert!(!claude.contains("OpenAI"), "{claude}");

        settings.assistant.engine = AssistantEngine::Opencode;
        assert_eq!(recipient_key(&settings), "opencode:default");
        settings.opencode.model = "anthropic/claude-sonnet-4-5".to_owned();
        let key = recipient_key(&settings);
        assert_eq!(key, "opencode:anthropic");
        let opencode = consent_text(1, &provider_name(&key), "u").unwrap();
        assert!(
            opencode.contains("Anthropic, through OpenCode"),
            "{opencode}"
        );
        assert!(opencode.contains("web search service"), "{opencode}");
        assert!(provider_name("opencode:acme").contains("«acme»"));
    }

    /// Requirement "Records end with a new consent text or engine": OpenCode's provider is
    /// part of the engine.
    #[test]
    fn a_change_of_opencode_provider_ends_the_records() {
        let store: Arc<dyn ConsentStore> = Arc::new(MemoryConsentStore::default());
        store
            .save(&[record(1, CONSENT_TEXT_VERSION, "opencode:openai")])
            .unwrap();
        let state = ConsentState::new(Arc::clone(&store), EventBus::default(), "opencode:openai");
        assert_eq!(state.ids().len(), 1);
        state.set_assistant("opencode:anthropic");
        assert!(state.ids().is_empty());
    }

    #[test]
    fn the_version_one_text_says_what_is_sent_kept_and_how_to_withdraw() {
        let text = consent_text(1, &provider_name("codex"), "https://example.org/p").unwrap();
        for words in [
            "OpenAI",
            "your voice",
            "Whisper",
            "text of your speech",
            "kept on the operator's PC",
            "/optout",
            "https://example.org/p",
        ] {
            assert!(text.contains(words), "{words}: {text}");
        }
        for version in 1..=CONSENT_TEXT_VERSION {
            assert!(
                consent_text(version, "x", "y").is_some(),
                "version {version}"
            );
        }
        assert!(consent_text(CONSENT_TEXT_VERSION + 1, "x", "y").is_none());
    }

    #[cfg(windows)]
    mod file {
        use super::*;

        #[test]
        fn records_survive_a_reload_and_a_changed_byte_reads_as_none() {
            let dir = tempfile::tempdir().unwrap();
            let path = consents_path(dir.path());
            let store: Arc<dyn ConsentStore> = Arc::new(FileConsentStore::new(path.clone()));
            let state = ConsentState::new(Arc::clone(&store), EventBus::default(), "codex");
            state.agree(11, "Alex", CONSENT_TEXT_VERSION, "t1");
            state.agree(12, "Sam", CONSENT_TEXT_VERSION, "t2");
            state.withdraw(11, false);
            let again = ConsentState::new(
                Arc::new(FileConsentStore::new(path.clone())),
                EventBus::default(),
                "codex",
            );
            assert_eq!(again.status().records.len(), 1);
            assert_eq!(again.status().records[0].name, "Sam");
            assert!(!crate::protect::tmp_path(&path).exists());

            let mut bytes = std::fs::read(&path).unwrap();
            let middle = bytes.len() / 2;
            bytes[middle] ^= 0x55;
            std::fs::write(&path, bytes).unwrap();
            let broken = ConsentState::new(
                Arc::new(FileConsentStore::new(path.clone())),
                EventBus::default(),
                "codex",
            );
            assert!(broken.ids().is_empty());
            let reason = broken.status().unreadable.expect("a reason");
            assert!(reason.contains("could not be read"), "{reason}");
        }

        #[test]
        fn an_unknown_version_reads_as_unreadable() {
            let dir = tempfile::tempdir().unwrap();
            let path = consents_path(dir.path());
            let plain = br#"{"version":2,"records":[]}"#;
            crate::protect::write_sealed(&path, plain, ENTROPY).unwrap();
            let error = FileConsentStore::new(path).load().unwrap_err();
            assert!(error.contains("version 2"), "{error}");
        }

        #[test]
        fn the_last_withdrawal_deletes_the_file() {
            let dir = tempfile::tempdir().unwrap();
            let path = consents_path(dir.path());
            let state = ConsentState::new(
                Arc::new(FileConsentStore::new(path.clone())),
                EventBus::default(),
                "codex",
            );
            state.agree(1, "A", CONSENT_TEXT_VERSION, "t");
            assert!(path.exists());
            state.withdraw(1, true);
            assert!(!path.exists());
        }
    }
}
