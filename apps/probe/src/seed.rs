//! Seeds the app's secret store from `probe.local.json`, so the app can be tested on this
//! machine without typing the token into the window.

use crate::Outcome;
use anyhow::{anyhow, Context};
use voicebro_core::secrets::{platform_store, SecretKey};

pub fn secrets(_args: &[String]) -> anyhow::Result<Outcome> {
    let text = std::fs::read_to_string("probe.local.json").context("reading probe.local.json")?;
    let local: serde_json::Value = serde_json::from_str(&text)?;
    let token = local
        .get("discord_token")
        .and_then(serde_json::Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| anyhow!("probe.local.json has no discord_token"))?;
    let store = platform_store();
    store.set(SecretKey::DiscordToken, token.trim())?;
    let back = store.get(SecretKey::DiscordToken)?;
    let ok = back.as_deref() == Some(token.trim());
    Ok(Outcome {
        pass: ok,
        details: vec![format!(
            "{} stored in the app's secret store: {}",
            SecretKey::DiscordToken.target(),
            if ok { "readable" } else { "mismatch" }
        )],
    })
}
