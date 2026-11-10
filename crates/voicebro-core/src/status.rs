//! The status snapshot the UI pulls, and the one place that logs replaced values.
//!
//! The snapshot holds measured and known values only. A quantity that is not measured is
//! absent, never a label.

use crate::events::{AppEvent, EventBus};
use crate::settings::Settings;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The connection errors by source (`discord`, `codex`, `assistant`, `secret_store`,
/// `runtime`), shared by the runtime, the gates and the assistant. Setting one publishes
/// `AppEvent::ConnectionError`; clearing it publishes the same event with no message.
pub struct ConnectionErrors {
    errors: Mutex<BTreeMap<String, String>>,
    events: EventBus,
}

impl ConnectionErrors {
    pub fn new(events: EventBus) -> Self {
        Self {
            errors: Mutex::new(BTreeMap::new()),
            events,
        }
    }

    pub fn report(&self, source: &str, message: Option<String>) {
        {
            let mut errors = self.errors.lock().expect("errors lock");
            match &message {
                Some(text) => {
                    errors.insert(source.to_owned(), text.clone());
                }
                None => {
                    errors.remove(source);
                }
            }
        }
        self.events.publish(AppEvent::ConnectionError {
            source: source.to_owned(),
            message,
        });
    }

    pub fn snapshot(&self) -> BTreeMap<String, String> {
        self.errors.lock().expect("errors lock").clone()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretStatus {
    pub is_set: bool,
}

/// The global mute hotkey as the desktop shell reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct HotkeyStatus {
    pub shortcut: String,
    pub registered: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub build_stamp: String,
    pub data_dir: String,
    pub muted: bool,
    pub hotkey: HotkeyStatus,
    /// Keyed by secret name; values say only whether the secret is set.
    pub secrets: BTreeMap<String, SecretStatus>,
    /// Keyed by source (`discord`, `codex`, `call`, `secret_store`); present while failing.
    pub connection_errors: BTreeMap<String, String>,
    pub discord: crate::discord::DiscordStatus,
    pub wake: crate::wake::WakeStatus,
    /// One gate per guild the bot is in a voice channel of, keyed by guild id.
    pub gates: BTreeMap<String, crate::gate::GateStatus>,
    pub assistant: crate::assistant::AssistantStatus,
    pub settings: Settings,
}

/// Logs that `requested` was replaced by `used`, and returns the line. Called wherever a
/// setting is rejected and a fallback takes its place, so the log never reports a value
/// that is not in effect.
pub fn log_used(what: &str, requested: &str, used: &str, reason: &str) -> String {
    let line = format!("{what}: requested «{requested}», using «{used}»: {reason}");
    log::warn!("{line}");
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_used_names_all_three_parts() {
        let line = log_used(
            "voice",
            "cedar",
            "juniper",
            "not available with a ChatGPT sign-in",
        );
        assert!(
            line.contains("cedar") && line.contains("juniper") && line.contains("not available")
        );
    }
}
