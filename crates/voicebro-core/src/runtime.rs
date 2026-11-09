//! The composition root. It owns the settings, the secret store, the event bus, the Discord
//! adapter, the gates, the wake engine and the assistant, and it is the only place that
//! knows every subsystem.

use crate::assistant::{Assistant, StubAssistant};
use crate::audio::{frame_channel, AudioPorts, Frame, OutputHandle, INPUT_CAPACITY};
use crate::discord::{AssistantState, DiscordError, DiscordRuntime, JoinPreparation, VoiceHost};
use crate::events::{AppEvent, EventBus};
use crate::gate::{ErrorReporter, GateSettings, RelayGate, RelayRegistry};
use crate::secrets::{SecretError, SecretKey, SecretStore};
use crate::settings::store::{SettingsStore, StoreError};
use crate::settings::{SessionMode, Settings, ValidationError};
use crate::status::{ConnectionErrors, HotkeyStatus, SecretStatus, Status};
use crate::wake::WakeManager;
use serde_json::Value;
use serenity::all::GuildId;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Validation(#[from] ValidationError),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Secret(#[from] SecretError),
    #[error("the patch is not a settings document: {0}")]
    Patch(#[from] serde_json::Error),
    #[error("unknown secret «{0}»")]
    UnknownSecret(String),
    #[error("{0}")]
    Discord(#[from] DiscordError),
}

impl ErrorReporter for ConnectionErrors {
    fn report(&self, source: &str, message: Option<String>) {
        ConnectionErrors::report(self, source, message);
    }
}

/// What Discord needs from the app: a gate per voice channel, built on join from the
/// current settings and attached to the assistant.
struct GateHost {
    registry: RelayRegistry,
    wake: WakeManager,
    assistant: Arc<dyn Assistant>,
    events: EventBus,
    errors: Arc<ConnectionErrors>,
    gate_settings: Mutex<GateSettings>,
    discord: Mutex<Option<Arc<DiscordRuntime>>>,
}

impl VoiceHost for GateHost {
    fn prepare_join(&self, guild: u64) -> JoinPreparation<'_> {
        Box::pin(async move {
            match self.assistant.prepare(guild).await {
                Ok(()) => {
                    self.errors.report("assistant", None);
                    Ok(())
                }
                Err(error) => {
                    let reason = error.to_string();
                    self.events
                        .log(format!("join to guild {guild} refused: {reason}"));
                    Err(reason)
                }
            }
        })
    }

    fn on_tick(&self, guild: u64, mixed: &Frame, speakers: &[(u32, &[i16])]) {
        if let Some(gate) = self.registry.get(guild) {
            gate.process_tick(mixed, speakers);
        }
    }

    fn set_speaker_name(&self, guild: u64, ssrc: u32, name: &str) {
        if let Some(gate) = self.registry.get(guild) {
            gate.set_speaker_name(ssrc, name);
        }
    }

    fn set_participants(&self, guild: u64, names: &[String]) {
        if let Some(gate) = self.registry.get(guild) {
            gate.set_participants(names);
        }
    }

    fn on_join(&self, guild: u64, output: OutputHandle) {
        let (sender, receiver) = frame_channel(INPUT_CAPACITY);
        output.use_gain(Arc::clone(&self.registry.gains().output));
        let gate = RelayGate::new(
            guild,
            sender,
            Arc::clone(&self.registry.gains().input),
            output.clone(),
            self.gate_settings
                .lock()
                .expect("gate settings lock")
                .clone(),
            self.registry.is_muted(),
            self.events.clone(),
            Some(Arc::new(self.wake.clone())),
            Some(Arc::clone(&self.errors) as Arc<dyn ErrorReporter>),
        );
        self.registry.register(Arc::clone(&gate));
        match self.assistant.attach(
            guild,
            AudioPorts {
                input: receiver,
                output,
            },
        ) {
            Ok(session) => gate.set_session(session),
            Err(error) => {
                self.events.log(format!(
                    "assistant could not attach to guild {guild}: {error}"
                ));
                self.errors.report("assistant", Some(error.to_string()));
            }
        }
    }

    fn on_leave(&self, guild: u64) {
        self.assistant.detach(guild);
        if let Some(gate) = self.registry.unregister(guild) {
            gate.output().flush();
        }
    }

    fn assistant_state(&self, guild: u64) -> AssistantState {
        let Some(gate) = self.registry.get(guild) else {
            return AssistantState::Off;
        };
        let status = gate.status();
        if status.muted {
            AssistantState::Muted
        } else if status.mode == SessionMode::WakeWord && !status.active {
            AssistantState::WaitingForWakeWord
        } else {
            AssistantState::Listening
        }
    }

    fn toggle_mute(&self) -> bool {
        let muted = self.registry.toggle_muted();
        self.after_mute_change(muted);
        muted
    }

    fn is_muted(&self) -> bool {
        self.registry.is_muted()
    }
}

impl GateHost {
    fn after_mute_change(&self, muted: bool) {
        log::info!("mute {}", if muted { "on" } else { "off" });
        self.events.publish(AppEvent::MuteChanged { muted });
        if let Some(discord) = self.discord.lock().expect("discord lock").as_ref() {
            discord.mark_panels_dirty();
        }
    }
}

pub struct Runtime {
    build_stamp: String,
    data_dir: PathBuf,
    store: SettingsStore,
    secrets: Arc<dyn SecretStore>,
    events: EventBus,
    handle: Handle,
    settings: RwLock<Settings>,
    host: Arc<GateHost>,
    discord: Arc<DiscordRuntime>,
    wake: WakeManager,
    assistant: Arc<dyn Assistant>,
    errors: Arc<ConnectionErrors>,
    secrets_set: Mutex<HashSet<SecretKey>>,
    hotkey: Mutex<HotkeyStatus>,
}

impl Runtime {
    /// Builds the runtime with the stub assistant. `with_assistant` swaps in a real one.
    pub fn new(
        build_stamp: impl Into<String>,
        data_dir: PathBuf,
        store: SettingsStore,
        secrets: Arc<dyn SecretStore>,
        events: EventBus,
        handle: Handle,
    ) -> Self {
        Self::with_assistant(
            build_stamp,
            data_dir,
            store,
            secrets,
            events.clone(),
            handle,
            Arc::new(ConnectionErrors::new(events)),
            Arc::new(StubAssistant::default()),
        )
    }

    /// `errors` is shared with the assistant, which reports its own failures there.
    #[allow(clippy::too_many_arguments)]
    pub fn with_assistant(
        build_stamp: impl Into<String>,
        data_dir: PathBuf,
        store: SettingsStore,
        secrets: Arc<dyn SecretStore>,
        events: EventBus,
        handle: Handle,
        errors: Arc<ConnectionErrors>,
        assistant: Arc<dyn Assistant>,
    ) -> Self {
        let registry = RelayRegistry::default();
        let wake = WakeManager::with_default_engines(
            crate::paths::assets_dir(&data_dir),
            events.clone(),
            Arc::new(registry.clone()),
        );
        let host = Arc::new(GateHost {
            registry,
            wake: wake.clone(),
            assistant: Arc::clone(&assistant),
            events: events.clone(),
            errors: Arc::clone(&errors),
            gate_settings: Mutex::new(GateSettings::from_settings(&Settings::default())),
            discord: Mutex::new(None),
        });
        let discord = Arc::new(DiscordRuntime::new(
            events.clone(),
            host.clone(),
            handle.clone(),
        ));
        *host.discord.lock().expect("discord lock") = Some(Arc::clone(&discord));
        Self {
            build_stamp: build_stamp.into(),
            data_dir,
            store,
            secrets,
            events,
            handle,
            settings: RwLock::new(Settings::default()),
            host,
            discord,
            wake,
            assistant,
            errors,
            secrets_set: Mutex::new(HashSet::new()),
            hotkey: Mutex::new(HotkeyStatus::default()),
        }
    }

    /// Loads settings (writing the defaults on first start), learns which secrets are set,
    /// configures the wake engine, and connects to Discord whenever a token is stored.
    /// Nothing of the assistant runs until the first `/join`.
    pub fn start(&self) -> Result<(), RuntimeError> {
        self.store.cleanup_partial();
        let settings = match self.store.load()? {
            Some(settings) => {
                if let Err(error) = settings.validate() {
                    self.events.log(format!(
                        "settings file has a problem ({error}); fix it in Settings"
                    ));
                }
                settings
            }
            None => {
                let defaults = Settings::default();
                self.store.save(&defaults)?;
                self.events.log(format!(
                    "no settings file yet; wrote the defaults to {}",
                    self.store.path().display()
                ));
                defaults
            }
        };
        *self.settings.write().expect("settings lock") = settings;
        *self.host.gate_settings.lock().expect("gate settings lock") =
            GateSettings::from_settings(&self.settings());
        self.host.registry.gains().apply(&self.settings().audio);
        crate::wake::assets::cleanup_partial(&crate::paths::assets_dir(&self.data_dir));
        self.wake.configure(&self.settings());
        // Nothing of the assistant runs yet: its process starts at the first /join.
        self.assistant.configure(&self.settings());

        for key in SecretKey::ALL {
            match self.secrets.get(*key) {
                Ok(Some(_)) => {
                    self.secrets_set.lock().expect("secrets lock").insert(*key);
                }
                Ok(None) => {}
                Err(error) => {
                    self.set_connection_error("secret_store", Some(error.to_string()));
                    log::error!("secret store: {error}");
                }
            }
        }
        log::info!("runtime started; data dir {}", self.data_dir.display());

        if self.is_secret_set(SecretKey::DiscordToken) {
            if let Err(error) = self.connect() {
                self.set_connection_error("discord", Some(error.to_string()));
            }
        }
        Ok(())
    }

    /// Stops the wake engine and Discord; waits for Discord so the process can exit cleanly.
    pub fn shutdown(&self) {
        self.wake.stop();
        self.assistant.stop();
        let discord = Arc::clone(&self.discord);
        if discord.is_running() {
            let _ = self.handle.block_on(async move {
                tokio::time::timeout(Duration::from_secs(10), discord.stop()).await
            });
        }
        log::info!("runtime stopped");
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().expect("settings lock").clone()
    }

    pub fn wake(&self) -> &WakeManager {
        &self.wake
    }

    pub fn assistant(&self) -> &Arc<dyn Assistant> {
        &self.assistant
    }

    /// Validates and stores a patched document, then applies the changed sections.
    pub fn save_settings(&self, patch: &Value) -> Result<Settings, RuntimeError> {
        let current = self.settings();
        let next = current.with_patch(patch)?;
        next.validate()?;
        self.store.save(&next)?;
        let changed = current.changed_sections(&next);
        *self.settings.write().expect("settings lock") = next.clone();
        self.apply(&changed);
        self.events.publish(AppEvent::SettingsChanged);
        Ok(next)
    }

    /// Reconfigures only the subsystems whose section changed.
    fn apply(&self, changed: &[&'static str]) {
        let settings = self.settings();
        let mut wake_dirty = false;
        let mut gates_dirty = false;
        for section in changed {
            log::info!("settings: {section} changed");
            match *section {
                "discord" => {
                    log::info!("settings: the guild id applies at the next connection");
                }
                "session" => {
                    wake_dirty = true;
                    gates_dirty = true;
                }
                // Shared atomics: the next frame sent and the next frame played use the
                // new gains, and nothing is rebuilt.
                "audio" => self.host.registry.gains().apply(&settings.audio),
                "wake" => wake_dirty = true,
                "assistant" => {
                    gates_dirty = true;
                    self.assistant.configure(&settings);
                }
                "codex" => self.assistant.configure(&settings),
                _ => {}
            }
        }
        if gates_dirty {
            let gate_settings = GateSettings::from_settings(&settings);
            *self.host.gate_settings.lock().expect("gate settings lock") = gate_settings.clone();
            self.host.registry.apply_settings(&gate_settings);
        }
        if wake_dirty {
            self.wake.configure(&settings);
        }
    }

    pub fn set_secret(&self, name: &str, value: &str) -> Result<(), RuntimeError> {
        let key = SecretKey::from_name(name)
            .ok_or_else(|| RuntimeError::UnknownSecret(name.to_owned()))?;
        self.secrets.set(key, value.trim())?;
        self.secrets_set.lock().expect("secrets lock").insert(key);
        self.set_connection_error("secret_store", None);
        log::info!("secret {} set", key.name());
        self.events.publish(AppEvent::SecretChanged {
            name: key.name().to_owned(),
            is_set: true,
        });
        if key == SecretKey::DiscordToken {
            self.reconnect();
        }
        Ok(())
    }

    pub fn clear_secret(&self, name: &str) -> Result<(), RuntimeError> {
        let key = SecretKey::from_name(name)
            .ok_or_else(|| RuntimeError::UnknownSecret(name.to_owned()))?;
        self.secrets.delete(key)?;
        self.secrets_set.lock().expect("secrets lock").remove(&key);
        log::info!("secret {} cleared", key.name());
        self.events.publish(AppEvent::SecretChanged {
            name: key.name().to_owned(),
            is_set: false,
        });
        if key == SecretKey::DiscordToken {
            self.disconnect();
        }
        Ok(())
    }

    /// The value itself, for the one consumer that needs it. Never logged or serialized.
    pub fn secret(&self, key: SecretKey) -> Result<Option<String>, RuntimeError> {
        Ok(self.secrets.get(key)?)
    }

    pub fn is_secret_set(&self, key: SecretKey) -> bool {
        self.secrets_set
            .lock()
            .expect("secrets lock")
            .contains(&key)
    }

    /// Starts the Discord client with the stored token.
    pub fn connect(&self) -> Result<(), RuntimeError> {
        let token = self
            .secret(SecretKey::DiscordToken)?
            .ok_or(DiscordError::NoToken)?;
        let guild = self.settings().discord.guild_id_value().map(GuildId::new);
        self.discord.start(token, guild)?;
        self.set_connection_error("discord", None);
        Ok(())
    }

    /// Stops the Discord client in the background.
    pub fn disconnect(&self) {
        let discord = Arc::clone(&self.discord);
        self.handle.spawn(async move {
            discord.stop().await;
        });
    }

    /// Stops, then starts again with the current token. Used when the token changes.
    fn reconnect(&self) {
        let discord = Arc::clone(&self.discord);
        let token = self.secret(SecretKey::DiscordToken).ok().flatten();
        let guild = self.settings().discord.guild_id_value().map(GuildId::new);
        let events = self.events.clone();
        self.handle.spawn(async move {
            discord.stop().await;
            if let Some(token) = token {
                if let Err(error) = discord.start(token, guild) {
                    events.log(format!("Discord: could not reconnect: {error}"));
                }
            }
        });
    }

    pub fn is_muted(&self) -> bool {
        self.host.registry.is_muted()
    }

    pub fn set_muted(&self, muted: bool) -> bool {
        if self.host.registry.set_muted(muted) {
            self.host.after_mute_change(muted);
        }
        muted
    }

    /// The single entry for all three mute sources (app button, hotkey, Discord panel).
    pub fn toggle_mute(&self) -> bool {
        self.set_muted(!self.is_muted())
    }

    /// The desktop shell reports the global hotkey registration here.
    pub fn set_hotkey_status(&self, status: HotkeyStatus) {
        match &status.error {
            Some(error) => self.events.log(format!(
                "hotkey «{}» not registered: {error}; the app button and the Discord panel still work",
                status.shortcut
            )),
            None if status.registered => {
                log::info!("hotkey «{}» registered", status.shortcut);
            }
            None => log::info!("no mute hotkey configured"),
        }
        *self.hotkey.lock().expect("hotkey lock") = status.clone();
        self.events.publish(AppEvent::Hotkey(Box::new(status)));
    }

    pub fn set_connection_error(&self, source: &str, message: Option<String>) {
        self.errors.report(source, message);
    }

    pub fn status(&self) -> Status {
        let set = self.secrets_set.lock().expect("secrets lock").clone();
        Status {
            build_stamp: self.build_stamp.clone(),
            data_dir: self.data_dir.display().to_string(),
            muted: self.is_muted(),
            hotkey: self.hotkey.lock().expect("hotkey lock").clone(),
            secrets: SecretKey::ALL
                .iter()
                .map(|key| {
                    (
                        key.name().to_owned(),
                        SecretStatus {
                            is_set: set.contains(key),
                        },
                    )
                })
                .collect(),
            connection_errors: self.errors.snapshot(),
            discord: self.discord.status(),
            wake: self.wake.status(),
            gates: self.host.registry.statuses(),
            audio: self.host.registry.gains().levels(),
            assistant: self.assistant.status(),
            settings: self.settings(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::output_pair;
    use crate::secrets::MemoryStore;
    use serde_json::json;

    fn runtime(dir: &tempfile::TempDir, tokio_rt: &tokio::runtime::Runtime) -> Runtime {
        Runtime::new(
            "0.1.0+test 2026-11-09",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            EventBus::new(16),
            tokio_rt.handle().clone(),
        )
    }

    #[test]
    fn first_start_writes_defaults_and_status_has_no_secret_values() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        assert!(dir.path().join("settings.json").exists());
        // A nonsense token: the connect attempt fails in the background; the secret path
        // itself must still behave.
        runtime
            .set_secret("discord_token", "  super-secret-token ")
            .unwrap();
        let status = runtime.status();
        assert!(status.secrets["discord_token"].is_set);
        let json = serde_json::to_string(&status).unwrap();
        assert!(
            !json.contains("super-secret-token"),
            "status must never carry the value"
        );
        assert_eq!(
            runtime.secret(SecretKey::DiscordToken).unwrap().as_deref(),
            Some("super-secret-token"),
            "the consumer gets the trimmed value"
        );
        runtime.clear_secret("discord_token").unwrap();
        assert!(!runtime.status().secrets["discord_token"].is_set);
        runtime.shutdown();
    }

    #[test]
    fn saving_a_patch_validates_persists_and_reports_the_changed_sections() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        let mut events = runtime.events().subscribe();
        let saved = runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 120 } }))
            .unwrap();
        assert_eq!(saved.session.idle_timeout_seconds, 120);
        assert_eq!(runtime.settings().session.idle_timeout_seconds, 120);
        let on_disk = SettingsStore::new(dir.path().join("settings.json"))
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(on_disk.session.idle_timeout_seconds, 120);
        // The session section changed, so the wake manager reports its state first.
        let mut saw_settings_changed = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::SettingsChanged => saw_settings_changed = true,
                AppEvent::Wake(status) => assert!(
                    matches!(
                        status.state,
                        crate::wake::WakeState::Off | crate::wake::WakeState::NoPhrases
                    ),
                    "{:?}",
                    status.state
                ),
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(saw_settings_changed);

        let error = runtime
            .save_settings(&json!({ "session": { "idle_timeout_seconds": 1 } }))
            .unwrap_err();
        assert!(error.to_string().contains("session.idle_timeout_seconds"));
        assert_eq!(
            runtime.settings().session.idle_timeout_seconds,
            120,
            "rejected save changes nothing"
        );
    }

    #[test]
    fn mute_is_one_state_with_an_event_per_change() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        let mut events = runtime.events().subscribe();
        assert!(runtime.toggle_mute());
        assert!(!runtime.toggle_mute());
        runtime.set_muted(false);
        assert!(matches!(
            events.try_recv(),
            Ok(AppEvent::MuteChanged { muted: true })
        ));
        assert!(matches!(
            events.try_recv(),
            Ok(AppEvent::MuteChanged { muted: false })
        ));
        assert!(
            events.try_recv().is_err(),
            "no event when the state did not change"
        );
    }

    #[test]
    fn joining_builds_a_gate_attached_to_the_assistant_and_leaving_removes_it() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always" } }))
            .unwrap();
        let (output, _source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output.clone());
        let status = runtime.status();
        assert!(status.gates["42"].active, "always mode: active at once");
        assert!(status.gates["42"].session_wanted);
        assert_eq!(status.assistant.calls[0].guild_id, "42");
        assert_eq!(host.assistant_state(42), AssistantState::Listening);
        assert_eq!(host.assistant_state(43), AssistantState::Off);

        runtime.set_muted(true);
        assert_eq!(host.assistant_state(42), AssistantState::Muted);
        assert!(output.is_muted());
        runtime.set_muted(false);

        runtime
            .save_settings(&json!({ "session": { "mode": "wake_word" } }))
            .unwrap();
        assert_eq!(host.assistant_state(42), AssistantState::WaitingForWakeWord);
        assert!(
            output.is_muted(),
            "waiting for the wake word keeps the output silent"
        );

        host.on_leave(42);
        assert!(runtime.status().gates.is_empty());
        assert!(runtime.status().assistant.calls.is_empty());
        assert_eq!(host.assistant_state(42), AssistantState::Off);
        runtime.shutdown();
    }

    #[test]
    fn a_saved_audio_section_reaches_the_next_frames_without_rebuilding() {
        use crate::audio::FRAME_SAMPLES;
        use std::io::Read;
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        runtime.start().unwrap();
        runtime
            .save_settings(&json!({ "session": { "mode": "always", "earcons": false } }))
            .unwrap();
        let (output, mut source) = output_pair();
        let host: Arc<dyn VoiceHost> = runtime.host.clone();
        host.on_join(42, output.clone());
        let gate = runtime.host.registry.get(42).unwrap();

        runtime
            .save_settings(&json!({ "audio": { "input_gain": 4.0, "output_gain": 2.5 } }))
            .unwrap();
        assert!(
            Arc::ptr_eq(&gate, &runtime.host.registry.get(42).unwrap()),
            "the gate is not rebuilt"
        );

        let loud = Frame::from_slice(&[10_000; FRAME_SAMPLES]);
        host.on_tick(42, &loud, &[(7, loud.as_slice())]);
        let audio = runtime.status().audio;
        assert_eq!((audio.input.gain, audio.output.gain), (4.0, 2.5));
        assert_eq!(
            audio.input.clips, FRAME_SAMPLES as u64,
            "the next frame sent used the new input gain"
        );

        for _ in 0..3 {
            output.push(Frame::from_slice(&[1_000; FRAME_SAMPLES]));
        }
        let mut bytes = vec![0u8; FRAME_SAMPLES * 4];
        source.read_exact(&mut bytes).unwrap();
        let first = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert!(
            (first - 2_500.0 / 32_768.0).abs() < 1e-6,
            "the next frame played used the new output gain: {first}"
        );
        assert_eq!(runtime.status().audio.output.clips, 0);
        runtime.shutdown();
    }

    #[test]
    fn nothing_of_codex_runs_after_start() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let events = EventBus::new(64);
        let errors = Arc::new(ConnectionErrors::new(events.clone()));
        let mut settings = Settings::default();
        // A path that exists: the engine would find it if it tried to start.
        let fake = dir.path().join("codex.exe");
        std::fs::write(&fake, b"").unwrap();
        settings.codex.executable = fake.display().to_string();
        let codex = crate::assistant::codex::CodexAssistant::new(
            tokio_rt.handle().clone(),
            events.clone(),
            Arc::clone(&errors),
            dir.path(),
            &settings,
        );
        let runtime = Runtime::with_assistant(
            "0.1.0+test 2026-11-10",
            dir.path().to_path_buf(),
            SettingsStore::new(dir.path().join("settings.json")),
            Arc::new(MemoryStore::default()),
            events,
            tokio_rt.handle().clone(),
            errors,
            Arc::new(codex),
        );
        runtime.start().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let assistant = runtime.status().assistant;
        assert!(!assistant.running, "{}", assistant.summary);
        assert!(assistant.summary.contains("/join"), "{}", assistant.summary);
        runtime.shutdown();
    }

    #[test]
    fn hotkey_status_is_kept_and_published() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        let mut events = runtime.events().subscribe();
        runtime.set_hotkey_status(HotkeyStatus {
            shortcut: "Ctrl+Alt+M".to_owned(),
            registered: false,
            error: Some("taken".to_owned()),
        });
        assert_eq!(runtime.status().hotkey.error.as_deref(), Some("taken"));
        let mut saw = false;
        while let Ok(event) = events.try_recv() {
            if let AppEvent::Hotkey(status) = event {
                saw = !status.registered;
            }
        }
        assert!(saw);
    }

    #[test]
    fn connect_without_a_token_is_refused() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        assert!(matches!(
            runtime.connect(),
            Err(RuntimeError::Discord(DiscordError::NoToken))
        ));
    }

    #[test]
    fn unknown_secret_names_are_rejected() {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime(&dir, &tokio_rt);
        assert!(matches!(
            runtime.set_secret("password", "x"),
            Err(RuntimeError::UnknownSecret(_))
        ));
    }
}
