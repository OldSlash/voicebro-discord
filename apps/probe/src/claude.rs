//! `probe claude-smoke` (change `claude-and-opencode-engines`, task 1.2): Claude Code as the
//! app runs it. Prints the version and the kind of sign-in (never a credential), the models
//! of the `initialize` answer, then runs two stream-json turns with every lockdown flag of
//! design D4 and an appended canary instruction, in a fresh working folder under `target\`.
//!
//! PASS when Claude Code is signed in, both turns are answered and begin with the canary
//! word (the appended instruction is followed), the second answer remembers the first turn,
//! the process offers only the web tools, and no session file was written for the folder.

use crate::Outcome;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use voicebro_core::assistant::claude::process::{ClaudeListener, ClaudeProcess};
use voicebro_core::assistant::claude::{claude_args, find_claude, parse_auth_status, parse_models};
use voicebro_core::EventBus;

/// Collects the process's messages.
#[derive(Default)]
struct Collector {
    messages: Mutex<Vec<Value>>,
}

impl ClaudeListener for Collector {
    fn on_message(&self, message: &Value) {
        self.messages.lock().unwrap().push(message.clone());
    }

    fn on_exit(&self) {}
}

impl Collector {
    fn results(&self) -> Vec<Value> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter(|message| message["type"] == "result")
            .cloned()
            .collect()
    }

    /// Each tool result Claude Code handed back: whether it succeeded, and its first words.
    fn tool_results(&self) -> Vec<(bool, String)> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter(|message| message["type"] == "user")
            .filter_map(|message| message["message"]["content"].as_array())
            .flatten()
            .filter(|part| part["type"] == "tool_result")
            .map(|part| {
                let text = part["content"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| part["content"].to_string());
                (
                    !part["is_error"].as_bool().unwrap_or(false),
                    text.chars().take(120).collect::<String>(),
                )
            })
            .collect()
    }

    fn tools(&self) -> Option<Value> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .find(|message| message["type"] == "system" && message["subtype"] == "init")
            .map(|message| message["tools"].clone())
    }
}

/// The folder Claude Code names after a working folder under `~\.claude\projects`.
fn project_folder(workspace: &std::path::Path) -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let mangled: String = workspace
        .display()
        .to_string()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(profile.join(".claude/projects").join(mangled))
}

pub fn smoke(args: &[String]) -> anyhow::Result<Outcome> {
    let model = args.first().cloned().unwrap_or_else(|| "haiku".to_owned());
    let executable = find_claude("").ok_or_else(|| anyhow::anyhow!("Claude Code not found"))?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut details = vec![format!("claude: {}", executable.display())];
    let version = std::process::Command::new(&executable)
        .arg("--version")
        .output()?;
    details.push(format!(
        "version: {}",
        String::from_utf8_lossy(&version.stdout).trim()
    ));

    let status = std::process::Command::new(&executable)
        .args(["auth", "status"])
        .output()?;
    let status: Value = serde_json::from_slice(&status.stdout).unwrap_or(Value::Null);
    let sign_in = parse_auth_status(&status);
    let fields: Vec<String> = status
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    details.push(format!(
        "auth status: {} (authMethod {:?}, apiProvider {:?}, plan {:?}, email {}; fields: {})",
        sign_in.summary(),
        sign_in.method,
        sign_in.provider,
        sign_in.plan,
        if sign_in.email.is_some() {
            "set"
        } else {
            "none"
        },
        fields.join(", ")
    ));
    if !sign_in.signed_in() {
        return Ok(Outcome::fail(details));
    }

    let root = std::env::current_dir()?.join("target/claude-smoke");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let workspace = root.join(format!("workspace-{stamp}"));
    std::fs::create_dir_all(&workspace)?;
    let mcp = root.join("mcp.json");
    std::fs::write(&mcp, json!({ "mcpServers": {} }).to_string())?;
    let instructions = root.join("instructions.txt");
    std::fs::write(
        &instructions,
        "Always begin every reply with the single word BANANA, then a colon.",
    )?;
    let process_args = claude_args(&model, Some(&instructions), &mcp);
    details.push(format!("arguments: {}", process_args.join(" ")));

    let outcome = runtime.block_on(async {
        let process = ClaudeProcess::spawn(
            &tokio::runtime::Handle::current(),
            EventBus::new(16),
            &executable,
            &process_args,
            &workspace,
        )
        .map_err(anyhow::Error::msg)?;
        let collector = Arc::new(Collector::default());
        process.listen(Some(Arc::downgrade(&collector) as Weak<dyn ClaudeListener>));
        let init = process
            .control(json!({ "subtype": "initialize" }))
            .await
            .map_err(anyhow::Error::msg)?;
        let models: Vec<String> = parse_models(&init)
            .into_iter()
            .map(|model| model.id)
            .collect();
        let account_fields: Vec<String> = init["account"]
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default();
        let mut lines = vec![
            format!("initialize: models {}", models.join(", ")),
            format!("initialize: account fields {}", account_fields.join(", ")),
        ];
        let started = Instant::now();
        process
            .send_user("Remember the word cobalt. Reply in five words.")
            .map_err(anyhow::Error::msg)?;
        while collector.results().is_empty() && started.elapsed() < Duration::from_secs(90) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        lines.push(format!("turn 1: {} ms", started.elapsed().as_millis()));
        process
            .send_user("Which word did I ask you to remember? One word.")
            .map_err(anyhow::Error::msg)?;
        while collector.results().len() < 2 && started.elapsed() < Duration::from_secs(180) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // A web search, which the don't-ask mode denies unless it is allowed in advance.
        process
            .send_user(
                "What is the weather in London today? Search the web, then answer in one sentence.",
            )
            .map_err(anyhow::Error::msg)?;
        while collector.results().len() < 3 && started.elapsed() < Duration::from_secs(300) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let results = collector.results();
        let tools = collector.tools();
        let tool_results = collector.tool_results();
        process.kill();
        anyhow::Ok((lines, results, tools, tool_results))
    })?;
    let (lines, results, tools, tool_results) = outcome;
    details.extend(lines);
    for (ok, text) in &tool_results {
        details.push(format!(
            "tool result: {} {text}",
            if *ok { "ok" } else { "ERROR" }
        ));
    }
    let denials: Vec<String> = results
        .iter()
        .filter_map(|result| result.get("permission_denials").and_then(Value::as_array))
        .flatten()
        .map(|denial| denial["tool_name"].as_str().unwrap_or("?").to_owned())
        .collect();
    details.push(format!("permission denials: {denials:?}"));
    let searched = tool_results.iter().any(|(ok, _)| *ok) && denials.is_empty();
    let answers: Vec<String> = results
        .iter()
        .map(|result| result["result"].as_str().unwrap_or("").to_owned())
        .collect();
    for (index, answer) in answers.iter().enumerate() {
        details.push(format!("answer {}: {answer}", index + 1));
    }
    details.push(format!("tools: {}", tools.clone().unwrap_or(Value::Null)));
    let canary = answers.len() == 3 && answers.iter().all(|answer| answer.starts_with("BANANA"));
    let remembered = answers
        .get(1)
        .is_some_and(|answer| answer.to_lowercase().contains("cobalt"));
    let web_only = tools
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools
                .iter()
                .all(|tool| tool == "WebSearch" || tool == "WebFetch")
        });
    let folder = project_folder(&workspace);
    let session_files = folder
        .as_ref()
        .and_then(|folder| std::fs::read_dir(folder).ok())
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
                .count()
        })
        .unwrap_or(0);
    details.push(format!(
        "canary followed: {canary}; remembered: {remembered}; web tools only: {web_only}; searched: {searched}; session files: {session_files}"
    ));
    // Claude Code makes an empty folder with an empty `memory` folder for the working folder;
    // nothing else was written. Leave nothing behind.
    if let Some(folder) = folder {
        let _ = std::fs::remove_dir(folder.join("memory"));
        let _ = std::fs::remove_dir(&folder);
    }
    let _ = std::fs::remove_dir_all(&workspace);
    Ok(
        if canary && remembered && web_only && searched && session_files == 0 {
            Outcome::pass(details)
        } else {
            Outcome::fail(details)
        },
    )
}
