//! The status snapshot the UI pulls, and the one place that logs replaced values.
//!
//! The snapshot holds measured and known values only. A quantity that is not measured is
//! absent, never a label.

use crate::settings::Settings;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct SecretStatus {
    pub is_set: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub build_stamp: String,
    pub data_dir: String,
    pub muted: bool,
    /// Keyed by secret name; values say only whether the secret is set.
    pub secrets: BTreeMap<String, SecretStatus>,
    /// Keyed by source (`discord`, `codex`, `call`, `secret_store`); present while failing.
    pub connection_errors: BTreeMap<String, String>,
    pub discord: crate::discord::DiscordStatus,
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
