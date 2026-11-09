//! Account, models and voices as the app-server reports them.

use crate::assistant::ModelInfo;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AccountInfo {
    pub signed_in: bool,
    pub kind: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
}

pub fn parse_account(result: &Value) -> AccountInfo {
    let account = result.get("account").filter(|account| !account.is_null());
    let text = |key: &str| {
        account
            .and_then(|account| account.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    AccountInfo {
        signed_in: account.is_some(),
        kind: text("type"),
        email: text("email"),
        plan: text("planType"),
    }
}

/// One page of `model/list`, hidden models left out.
pub fn parse_models(result: &Value) -> Vec<ModelInfo> {
    result
        .get("data")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter(|model| {
                    !model
                        .get("hidden")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .filter_map(|model| {
                    let id = model.get("id").and_then(Value::as_str)?.to_owned();
                    let text = |key: &str| {
                        model
                            .get(key)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    let display_name = text("displayName");
                    Some(ModelInfo {
                        display_name: if display_name.is_empty() {
                            id.clone()
                        } else {
                            display_name
                        },
                        description: text("description"),
                        reasoning_efforts: model
                            .get("supportedReasoningEfforts")
                            .and_then(Value::as_array)
                            .map(|efforts| {
                                efforts
                                    .iter()
                                    .filter_map(|effort| {
                                        effort
                                            .get("reasoningEffort")
                                            .or(Some(effort))
                                            .and_then(Value::as_str)
                                            .map(str::to_owned)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        default_reasoning_effort: model
                            .get("defaultReasoningEffort")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        is_default: model
                            .get("isDefault")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn next_cursor(result: &Value) -> Option<String> {
    result
        .get("nextCursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
}

/// The voices a realtime v3 call accepts, and the default when the configured one is not
/// among them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Voices {
    pub all: Vec<String>,
    pub default: Option<String>,
}

impl Voices {
    pub fn parse(result: &Value) -> Self {
        let list = |key: &str| {
            result
                .pointer(&format!("/voices/{key}"))
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let mut all = list("v3");
        for voice in list("v1") {
            if !all.contains(&voice) {
                all.push(voice);
            }
        }
        let default = ["defaultV3", "defaultV1"]
            .iter()
            .find_map(|key| {
                result
                    .pointer(&format!("/voices/{key}"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| all.first().cloned());
        Self { all, default }
    }

    /// The voice to use: the configured one when the account has it, else the default.
    /// Returns `(voice, fell_back)`; `None` when nothing is known.
    pub fn choose(&self, configured: &str) -> (Option<String>, bool) {
        let configured = configured.trim();
        if self.all.is_empty() {
            return (
                (!configured.is_empty()).then(|| configured.to_owned()),
                false,
            );
        }
        if self.all.iter().any(|voice| voice == configured) {
            return (Some(configured.to_owned()), false);
        }
        (self.default.clone(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accounts_models_and_voices_are_parsed() {
        let signed_out = parse_account(&json!({ "account": null, "requiresOpenaiAuth": true }));
        assert!(!signed_out.signed_in);
        let signed_in = parse_account(&json!({
            "account": { "type": "chatgpt", "email": "me@example.com", "planType": "plus" }
        }));
        assert!(signed_in.signed_in);
        assert_eq!(signed_in.plan.as_deref(), Some("plus"));

        let models = parse_models(&json!({ "data": [
            {
                "id": "gpt-6.1-sol", "displayName": "GPT-6.1 Sol", "description": "Fast",
                "supportedReasoningEfforts": [
                    { "reasoningEffort": "low", "description": "" },
                    { "reasoningEffort": "high", "description": "" }
                ],
                "defaultReasoningEffort": "low", "isDefault": true, "hidden": false
            },
            { "id": "hidden-model", "displayName": "", "supportedReasoningEfforts": [],
              "defaultReasoningEffort": "low", "isDefault": false, "hidden": true }
        ], "nextCursor": "" }));
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].reasoning_efforts, vec!["low", "high"]);
        assert!(models[0].is_default);
        assert_eq!(next_cursor(&json!({ "nextCursor": "" })), None);
        assert_eq!(
            next_cursor(&json!({ "nextCursor": "x" })).as_deref(),
            Some("x")
        );

        let voices = Voices::parse(&json!({ "voices": {
            "v1": ["juniper", "maple"], "v3": ["juniper", "cove"], "defaultV1": "juniper"
        }}));
        assert_eq!(voices.all, vec!["juniper", "cove", "maple"]);
        assert_eq!(voices.default.as_deref(), Some("juniper"));
        assert_eq!(voices.choose("cove"), (Some("cove".to_owned()), false));
        assert_eq!(voices.choose("cedar"), (Some("juniper".to_owned()), true));
        assert_eq!(
            Voices::default().choose("cedar"),
            (Some("cedar".to_owned()), false)
        );
    }
}
