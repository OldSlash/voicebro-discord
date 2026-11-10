//! The event bus: the core publishes, the desktop shell forwards to the webview.
//!
//! Events are hints. The UI pulls a full [`crate::Status`] snapshot whenever it needs the
//! truth, so a lagged subscriber that drops old events loses nothing permanent.

use serde::Serialize;
use tokio::sync::broadcast;

/// Something the UI may want to show. Later changes add variants (Discord, gate, wake,
/// assistant, chat, asset downloads) next to these.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    fn events_reach_subscribers_and_serialize_with_a_kind_tag() {
        let bus = EventBus::new(8);
        let mut receiver = bus.subscribe();
        bus.publish(AppEvent::MuteChanged { muted: true });
        let event = receiver.try_recv().expect("event delivered");
        let json = serde_json::to_value(&event).expect("serializes");
        assert_eq!(json["kind"], "mute_changed");
        assert_eq!(json["muted"], true);
    }

    #[test]
    fn publishing_without_subscribers_does_not_fail() {
        let bus = EventBus::new(8);
        bus.publish(AppEvent::SettingsChanged);
    }
}
