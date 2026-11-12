//! The event bus: the core publishes, the desktop shell forwards to the webview.
//!
//! Events are hints. The UI pulls a full [`crate::Status`] snapshot whenever it needs the
//! truth, so a lagged subscriber that drops old events loses nothing permanent.

use serde::Serialize;
use tokio::sync::broadcast;

/// Something the UI may want to show. Serialized with the variant name in `event`, a key no
/// payload uses (a chat entry has its own `kind`, which an `event`-less tag would collide
/// with and overwrite in the window).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AppEvent {
    /// A human-readable line for the UI log pane.
    Log { line: String },
    /// The app-wide mute changed, from any of its sources.
    MuteChanged { muted: bool },
    /// Settings were saved and applied; the UI should re-read them.
    SettingsChanged,
    /// A secret was set or cleared. Only the name and whether it is set, never the value.
    SecretChanged { name: String, is_set: bool },
    /// An external connection failed or recovered. `message: None` means recovered.
    ConnectionError {
        source: String,
        message: Option<String>,
    },
    /// The Discord connection or a voice channel changed.
    Discord(Box<crate::discord::DiscordStatus>),
    /// The wake-word engine changed state (downloading, loading, ready, error).
    Wake(Box<crate::wake::WakeStatus>),
    /// A guild's session gate changed (session started or ended, mute, settings).
    Gate(Box<crate::gate::GateStatus>),
    /// The global hotkey was registered, re-registered or failed.
    Hotkey(Box<crate::status::HotkeyStatus>),
    /// One new line of the assistant chat log.
    Chat(Box<crate::assistant::chat_log::ChatEntry>),
    /// The assistant's account, process or call state changed.
    Assistant(Box<crate::assistant::AssistantStatus>),
    /// Progress of a first-use asset download, in steps of at least 20 %.
    Assets {
        label: String,
        percent: u8,
        error: Option<String>,
    },
    /// The bot left a voice channel because the Opus codec panicked (change
    /// `opus-codec-experiment`): the window's notice.
    CodecFailure {
        guild_name: String,
        channel_name: String,
        /// `discord-out`, `receive`, `call-send` or `call-receive`.
        path: String,
        /// What the codec was doing, for the user.
        activity: String,
        /// Name and version, as `opus-rs 0.1.42`.
        codec: String,
        /// The panic's message.
        message: String,
    },
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AppEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publishes to every current subscriber. With no subscribers the event is dropped,
    /// which is fine: the status snapshot is the source of truth.
    pub fn publish(&self, event: AppEvent) {
        let _ = self.sender.send(event);
    }

    pub fn log(&self, line: impl Into<String>) {
        let line = line.into();
        log::info!("{line}");
        self.publish(AppEvent::Log { line });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_reach_subscribers_and_serialize_with_an_event_tag() {
        let bus = EventBus::new(8);
        let mut receiver = bus.subscribe();
        bus.publish(AppEvent::MuteChanged { muted: true });
        let event = receiver.try_recv().expect("event delivered");
        let json = serde_json::to_value(&event).expect("serializes");
        assert_eq!(json["event"], "mute_changed");
        assert_eq!(json["muted"], true);
    }

    /// Every variant carries `event` exactly once, holding its name; no payload uses the
    /// key. A chat entry keeps its own `kind`.
    #[test]
    fn no_payload_collides_with_the_event_tag() {
        use crate::assistant::chat_log::ChatEntry;
        let chat = ChatEntry {
            id: 1,
            guild_id: "42".to_owned(),
            at: "12:00:00".to_owned(),
            kind: "agent",
            who: None,
            text: "Searching the web".to_owned(),
        };
        let gate = crate::gate::GateStatus {
            guild_id: "42".to_owned(),
            mode: crate::settings::SessionMode::WakeWord,
            muted: false,
            active: false,
            listening_for_wake_word: true,
            session_wanted: false,
            assistant_ready: false,
            last_wake_phrase: None,
            replay_held: 0,
            counters: Default::default(),
        };
        let all = [
            (
                "log",
                AppEvent::Log {
                    line: "x".to_owned(),
                },
            ),
            ("mute_changed", AppEvent::MuteChanged { muted: true }),
            ("settings_changed", AppEvent::SettingsChanged),
            (
                "secret_changed",
                AppEvent::SecretChanged {
                    name: "discord_token".to_owned(),
                    is_set: true,
                },
            ),
            (
                "connection_error",
                AppEvent::ConnectionError {
                    source: "codex".to_owned(),
                    message: Some("x".to_owned()),
                },
            ),
            ("discord", AppEvent::Discord(Box::default())),
            ("wake", AppEvent::Wake(Box::default())),
            ("gate", AppEvent::Gate(Box::new(gate))),
            ("hotkey", AppEvent::Hotkey(Box::default())),
            ("chat", AppEvent::Chat(Box::new(chat))),
            ("assistant", AppEvent::Assistant(Box::default())),
            (
                "assets",
                AppEvent::Assets {
                    label: "Vosk model".to_owned(),
                    percent: 40,
                    error: None,
                },
            ),
        ];
        for (name, event) in all {
            let text = serde_json::to_string(&event).unwrap();
            assert_eq!(text.matches("\"event\":").count(), 1, "{name}: {text}");
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(json["event"], name, "{text}");
            if name == "chat" {
                assert_eq!(json["kind"], "agent", "the entry keeps its kind");
            }
        }
    }

    #[test]
    fn publishing_without_subscribers_does_not_fail() {
        let bus = EventBus::new(8);
        bus.publish(AppEvent::SettingsChanged);
    }
}
