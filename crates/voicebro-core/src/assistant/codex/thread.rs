//! One ephemeral, read-only Codex thread per guild, opened with the configured model and
//! reasoning effort, falling back to the account defaults when Codex rejects them.

use super::app_server::AppServer;
use crate::events::EventBus;
use crate::status::log_used;
use serde_json::{json, Value};
use std::path::Path;

/// What a new thread is created with. Empty model or effort means the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadSettings {
    pub instructions: String,
    pub model: String,
    pub effort: String,
}

impl ThreadSettings {
    pub fn describe(&self) -> String {
        let model = if self.model.is_empty() {
            "default model"
        } else {
            self.model.as_str()
        };
        let effort = if self.effort.is_empty() {
            "default"
        } else {
            self.effort.as_str()
        };
        format!("{model}, reasoning {effort}")
    }
}

/// The thread a conversation uses, and the settings it was created with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadInfo {
    pub generation: u64,
    pub id: String,
    /// The model actually in use (empty: the account default).
    pub model: String,
    pub effort: String,
    /// The settings the thread was asked for, to detect a change.
    pub requested: ThreadSettings,
}

/// `untrusted` as in the first version's working code (its rewrite spec said `never`): the
/// agent keeps its server-side tools, web search included, and every approval request that
/// reaches the app is declined.
pub fn thread_start_params(workspace: &Path, settings: &ThreadSettings) -> Value {
    let mut params = json!({
        "cwd": workspace,
        "sandbox": "read-only",
        "approvalPolicy": "untrusted",
        "ephemeral": true,
        "developerInstructions": settings.instructions,
    });
    if !settings.model.is_empty() {
        params["model"] = json!(settings.model);
    }
    if !settings.effort.is_empty() {
        params["config"] = json!({ "model_reasoning_effort": settings.effort });
    }
    params
}

fn thread_id(result: Value) -> Result<String, String> {
    result
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Codex did not return a thread id".to_owned())
}

/// Creates an ephemeral read-only thread with the chosen model and reasoning effort. If
/// Codex rejects those, it falls back to the account defaults so voice keeps working, and
/// the log names both values.
pub async fn open_thread(
    server: &AppServer,
    events: &EventBus,
    workspace: &Path,
    settings: &ThreadSettings,
) -> Result<ThreadInfo, String> {
    std::fs::create_dir_all(workspace)
        .map_err(|error| format!("could not create the Codex workspace: {error}"))?;
    let custom = !settings.model.is_empty() || !settings.effort.is_empty();
    let (id, used) = match server
        .request("thread/start", thread_start_params(workspace, settings))
        .await
    {
        Ok(result) => (thread_id(result)?, settings.clone()),
        Err(error) if custom => {
            let defaults = ThreadSettings {
                instructions: settings.instructions.clone(),
                ..ThreadSettings::default()
            };
            events.log(log_used(
                "Codex model",
                &settings.describe(),
                &defaults.describe(),
                &format!("rejected by Codex: {error}"),
            ));
            (
                thread_id(
                    server
                        .request("thread/start", thread_start_params(workspace, &defaults))
                        .await?,
                )?,
                defaults,
            )
        }
        Err(error) => return Err(error),
    };
    events.log(format!("Codex: thread ready ({})", used.describe()));
    Ok(ThreadInfo {
        generation: server.generation,
        id,
        model: used.model,
        effort: used.effort,
        requested: settings.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_params_are_read_only_and_carry_the_model_only_when_set() {
        let settings = ThreadSettings {
            instructions: "p".to_owned(),
            model: "gpt-6.1-sol".to_owned(),
            effort: "high".to_owned(),
        };
        let params = thread_start_params(Path::new("C:/w"), &settings);
        assert_eq!(params["model"], "gpt-6.1-sol");
        assert_eq!(params["config"]["model_reasoning_effort"], "high");
        assert_eq!(params["sandbox"], "read-only");
        assert_eq!(params["approvalPolicy"], "untrusted");
        assert_eq!(params["ephemeral"], true);
        let defaults = thread_start_params(Path::new("C:/w"), &ThreadSettings::default());
        assert!(defaults.get("model").is_none());
        assert!(defaults.get("config").is_none());
        assert_eq!(settings.describe(), "gpt-6.1-sol, reasoning high");
        assert_eq!(
            ThreadSettings::default().describe(),
            "default model, reasoning default"
        );
    }
}
