//! `probe opencode-smoke` (change `claude-and-opencode-engines`, task 1.4): OpenCode 2.x as
//! the app runs it. Starts `opencode serve` with the app's configuration file and environment
//! (design D7) in a fresh working folder under `target\`, prints the version, the providers
//! set up and the models (never a credential), then on one session: two turns with a canary
//! instruction in the agent's `system`, a request to read a file in the working folder (the
//! agent may only fetch and search), a web search (its answer printed with every invisible
//! character escaped), and an interrupt of a long answer. Prints every kind of
//! event the server sent, so the V2 API read from OpenCode's source is checked live.
//!
//! PASS when a call without the password is refused, a model is usable, both turns end and
//! begin with the canary word, the second remembers the first, the file's secret never
//! appears, a web search is made, the interrupt ends the execution, and the session is
//! deleted.

use crate::Outcome;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use voicebro_core::assistant::opencode::server::{OpencodeListener, OpencodeServer, EVERY_EVENT};
use voicebro_core::assistant::opencode::turns::OpencodeTurns;
use voicebro_core::assistant::opencode::{
    config_document, connected_providers, find_opencode, model_name, model_names, search_provider,
    server_env, AGENT, PROVIDERS_ROUTE,
};

/// The secret in the working folder that the agent must not read.
const SECRET: &str = "PLUM-7731";

/// Every kind of event the server sent, with the first event of each kind whole.
#[derive(Default)]
struct Kinds {
    kinds: Mutex<BTreeSet<String>>,
    first: Mutex<Vec<String>>,
}

impl OpencodeListener for Kinds {
    fn on_event(&self, kind: &str, event: &Value) {
        if self.kinds.lock().unwrap().insert(kind.to_owned()) {
            let mut text = event.to_string();
            text.truncate(400);
            self.first.lock().unwrap().push(text);
        }
    }

    fn on_exit(&self) {}
}

/// The session's texts, tool events and execution ends.
#[derive(Default)]
struct Collector {
    texts: Mutex<Vec<String>>,
    deltas: Mutex<String>,
    ends: Mutex<Vec<String>>,
    tools: Mutex<Vec<String>>,
}

impl OpencodeListener for Collector {
    fn on_event(&self, kind: &str, data: &Value) {
        match kind {
            "session.text.delta" => self
                .deltas
                .lock()
                .unwrap()
                .push_str(data.get("delta").and_then(Value::as_str).unwrap_or("")),
            "session.text.ended" => {
                let text = std::mem::take(&mut *self.deltas.lock().unwrap());
                let text = data
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or(text);
                self.texts.lock().unwrap().push(text);
            }
            "session.execution.succeeded"
            | "session.execution.failed"
            | "session.execution.interrupted" => {
                let error = data
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str);
                self.ends.lock().unwrap().push(match error {
                    Some(error) => format!("{kind} ({error})"),
                    None => kind.to_owned(),
                });
            }
            other if other.starts_with("session.tool.") => {
                let mut text = format!("{other} {data}");
                if let Some(end) = text.char_indices().nth(700).map(|(at, _)| at) {
                    text.truncate(end);
                }
                self.tools.lock().unwrap().push(text);
            }
            _ => {}
        }
    }

    fn on_exit(&self) {}
}

impl Collector {
    fn ends(&self) -> usize {
        self.ends.lock().unwrap().len()
    }

    /// The texts since `from`, joined.
    fn answer_since(&self, from: usize) -> String {
        self.texts.lock().unwrap()[from..]
            .join(" ")
            .trim()
            .to_owned()
    }
}

/// The status line of a call to `port` without the password.
fn without_password(port: u16) -> String {
    let attempt = || -> std::io::Result<String> {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream
            .write_all(b"GET /api/info HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);
        Ok(String::from_utf8_lossy(&reply)
            .lines()
            .next()
            .unwrap_or("")
            .to_owned())
    };
    attempt().unwrap_or_else(|error| format!("no answer: {error}"))
}

async fn wait_for(condition: impl Fn() -> bool, limit: Duration) -> bool {
    let started = Instant::now();
    while !condition() {
        if started.elapsed() > limit {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    true
}

async fn ask(
    server: &OpencodeServer,
    session: &str,
    collector: &Collector,
    text: &str,
    lines: &mut Vec<String>,
) -> anyhow::Result<String> {
    let ends = collector.ends();
    let texts = collector.texts.lock().unwrap().len();
    let started = Instant::now();
    server
        .post(
            &format!("/api/session/{session}/prompt"),
            &json!({ "text": text, "delivery": "queue" }),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    let ended = wait_for(|| collector.ends() > ends, Duration::from_secs(120)).await;
    let answer = collector.answer_since(texts);
    lines.push(format!(
        "turn \"{text}\": {} in {} ms: {answer}",
        collector
            .ends
            .lock()
            .unwrap()
            .last()
            .filter(|_| ended)
            .cloned()
            .unwrap_or_else(|| "no end".to_owned()),
        started.elapsed().as_millis()
    ));
    Ok(answer)
}

pub fn smoke(args: &[String]) -> anyhow::Result<Outcome> {
    // OPENCODE_EXECUTABLE names another program, as `opencode.executable` does in the app.
    let configured = std::env::var("OPENCODE_EXECUTABLE").unwrap_or_default();
    let executable =
        find_opencode(&configured).ok_or_else(|| anyhow::anyhow!("OpenCode not found"))?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut details = vec![format!("opencode: {}", executable.display())];

    let root = std::env::current_dir()?.join("target/opencode-smoke");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let workspace = root.join(format!("workspace-{stamp}"));
    std::fs::create_dir_all(&workspace)?;
    std::fs::write(
        workspace.join("secret.txt"),
        format!("The code is {SECRET}.\n"),
    )?;
    let config = root.join("opencode.json");
    let search = search_provider(|key| std::env::var(key).ok());
    std::fs::write(
        &config,
        serde_json::to_string_pretty(&config_document(
            "Always begin every reply with the single word BANANA, then a colon.",
            search,
        ))?,
    )?;
    details.push(format!("web search provider: {search}"));

    let outcome = runtime.block_on(run(&executable, &workspace, &config, args.first()));
    let _ = std::fs::remove_dir_all(&workspace);
    let (mut lines, pass) = outcome?;
    details.append(&mut lines);
    Ok(if pass {
        Outcome::pass(details)
    } else {
        Outcome::fail(details)
    })
}

async fn run(
    executable: &Path,
    workspace: &Path,
    config: &Path,
    wanted: Option<&String>,
) -> anyhow::Result<(Vec<String>, bool)> {
    let mut lines = Vec::new();
    let started = Instant::now();
    let server = OpencodeServer::start(
        &tokio::runtime::Handle::current(),
        executable,
        workspace,
        &server_env(config),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    lines.push(format!(
        "server: 127.0.0.1:{}, version {}, ready in {} ms",
        server.port,
        server.version.as_deref().unwrap_or("unknown"),
        started.elapsed().as_millis()
    ));
    let kinds = Arc::new(Kinds::default());
    server.listen(
        EVERY_EVENT,
        Arc::downgrade(&kinds) as Weak<dyn OpencodeListener>,
    );
    let collector = Arc::new(Collector::default());
    let refused = without_password(server.port);
    lines.push(format!("without the password: {refused}"));
    let private = refused.contains(" 401");

    let providers = server
        .get(PROVIDERS_ROUTE)
        .await
        .map(|reply| connected_providers(&reply))
        .map_err(anyhow::Error::msg)?;
    let models: Vec<String> = server
        .get("/api/model")
        .await
        .map(|reply| model_names(&reply))
        .map_err(anyhow::Error::msg)?;
    let default_model = server
        .get("/api/model/default")
        .await
        .ok()
        .and_then(|reply| model_name(reply.get("data").unwrap_or(&reply)));
    lines.push(format!(
        "providers: {}; {} models (first: {}); default: {}",
        if providers.is_empty() {
            "none".to_owned()
        } else {
            providers.join(", ")
        },
        models.len(),
        models
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        default_model.as_deref().unwrap_or("none")
    ));
    let model = wanted.cloned().or(default_model);
    let Some(model) = model.filter(|model| models.contains(model)) else {
        lines.push("no usable model: run `opencode auth login` or name a model".to_owned());
        server.kill();
        return Ok((lines, false));
    };
    lines.push(format!("model: {model}"));

    let mut body = json!({
        "title": "VoiceBro probe",
        "agent": AGENT,
        "location": { "directory": workspace.display().to_string() },
    });
    if let Some(model) = OpencodeTurns::model_body(&model) {
        body["model"] = model;
    }
    let reply = server
        .post("/api/session", &body)
        .await
        .map_err(anyhow::Error::msg)?;
    let session = reply
        .get("data")
        .unwrap_or(&reply)
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("the session has no id: {reply}"))?
        .to_owned();
    lines.push(format!("session: {session}"));
    server.listen(
        &session,
        Arc::downgrade(&collector) as Weak<dyn OpencodeListener>,
    );

    let first = ask(
        &server,
        &session,
        &collector,
        "Remember the word cobalt. Reply in five words.",
        &mut lines,
    )
    .await?;
    let second = ask(
        &server,
        &session,
        &collector,
        "Which word did I ask you to remember? One word.",
        &mut lines,
    )
    .await?;
    let file = ask(
        &server,
        &session,
        &collector,
        "Read the file secret.txt in your working folder and tell me the code in it.",
        &mut lines,
    )
    .await?;
    // A web search through the configured provider (design D8); the answer printed with
    // every character outside printable ASCII escaped, to show the marks the model leaves.
    let weather = ask(
        &server,
        &session,
        &collector,
        "What is the weather in London today? Search the web.",
        &mut lines,
    )
    .await?;
    lines.push(format!(
        "weather answer escaped: {}",
        weather
            .chars()
            .map(|c| if c == ' ' || c.is_ascii_graphic() {
                c.to_string()
            } else {
                format!("\\u{{{:x}}}", c as u32)
            })
            .collect::<String>()
    ));
    let searched = collector
        .tools
        .lock()
        .unwrap()
        .iter()
        .any(|tool| tool.contains("websearch"));

    // An interrupt of a long answer, after its first words.
    let ends = collector.ends();
    let texts_before = collector.deltas.lock().unwrap().len();
    server
        .post(
            &format!("/api/session/{session}/prompt"),
            &json!({ "text": "Count from one to three hundred in words, one number per line.", "delivery": "queue" }),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    let speaking = wait_for(
        || collector.deltas.lock().unwrap().len() > texts_before + 20,
        Duration::from_secs(60),
    )
    .await;
    let interrupt_at = Instant::now();
    let interrupted = server
        .post(&format!("/api/session/{session}/interrupt"), &json!({}))
        .await
        .map_err(anyhow::Error::msg)?;
    let ended = wait_for(|| collector.ends() > ends, Duration::from_secs(10)).await;
    let end = collector.ends.lock().unwrap().last().cloned();
    lines.push(format!(
        "interrupt (answer streaming: {speaking}): reply {interrupted}, end {} after {} ms",
        end.as_deref().filter(|_| ended).unwrap_or("none"),
        interrupt_at.elapsed().as_millis()
    ));
    let interrupt_ok = ended && end.as_deref() == Some("session.execution.interrupted");

    let deleted = server.delete(&format!("/api/session/{session}")).await;
    lines.push(format!("session deleted: {deleted:?}"));
    for tool in collector.tools.lock().unwrap().iter() {
        lines.push(format!("tool event: {tool}"));
    }
    lines.push(format!(
        "event kinds: {}",
        kinds
            .kinds
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    ));
    for sample in kinds.first.lock().unwrap().iter() {
        lines.push(format!("first of its kind: {sample}"));
    }
    let left: Vec<String> = std::fs::read_dir(workspace)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("working folder holds: {}", left.join(", ")));
    server.kill();

    let canary = [&first, &second, &file]
        .iter()
        .all(|answer| answer.trim_start().starts_with("BANANA"));
    let remembered = second.to_lowercase().contains("cobalt");
    let locked = !file.contains(SECRET);
    lines.push(format!(
        "private: {private}; canary followed: {canary}; remembered: {remembered}; file not read: {locked}; searched: {searched}; interrupted: {interrupt_ok}; deleted: {}",
        deleted.is_ok()
    ));
    let pass =
        private && canary && remembered && locked && searched && interrupt_ok && deleted.is_ok();
    Ok((lines, pass))
}
