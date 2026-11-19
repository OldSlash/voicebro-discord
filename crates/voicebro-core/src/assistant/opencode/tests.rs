//! OpenCode's engine against the Node stand-in in `testdata/fake-opencode`, which serves the
//! V2 API as read from OpenCode 2.3.2's source (change `claude-and-opencode-engines`, task
//! 6). The tests that need it check nothing where Node is not installed.

use super::*;
use crate::assistant::{AgentAnswer, AnswerSink, UserText};
use crate::audio::codec::CodecFailures;
use crate::audio::{frame_channel, output_pair, INPUT_CAPACITY};
use crate::tts::manager::{Loader, TtsManager};
use crate::tts::{Synth, TtsError};
use std::time::Instant;

/// Requirement "Finding OpenCode": npm's native program, never its shim.
#[test]
fn opencode_is_found_as_the_native_program() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path().join("Roaming");
    let shim_dir = app_data.join("npm");
    std::fs::create_dir_all(&shim_dir).unwrap();
    std::fs::write(shim_dir.join("opencode.cmd"), b"").unwrap();
    assert_eq!(
        find_opencode_in("", &[], None, Some(&app_data)),
        None,
        "the shim is never used"
    );
    let native =
        app_data.join("npm/node_modules/opencode-ai/node_modules/opencode-windows-x64/bin");
    std::fs::create_dir_all(&native).unwrap();
    std::fs::write(native.join("opencode.exe"), b"").unwrap();
    assert_eq!(
        find_opencode_in("", &[], None, Some(&app_data)),
        Some(native.join("opencode.exe"))
    );
    let local = dir.path().join("Local");
    // winget's package of OpenCode 2.3.2 adds its own folder to the PATH, with no link.
    let package = local
        .join("Microsoft/WinGet/Packages/Anomaly.OpenCode_Microsoft.Winget.Source_8wekyb3d8bbwe");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("opencode.exe"), b"").unwrap();
    assert_eq!(
        find_opencode_in("", &[], Some(&local), Some(&app_data)),
        Some(package.join("opencode.exe")),
        "winget's package folder before npm"
    );
    let links = local.join("Microsoft/WinGet/Links");
    std::fs::create_dir_all(&links).unwrap();
    std::fs::write(links.join("opencode.exe"), b"").unwrap();
    assert_eq!(
        find_opencode_in("", &[], Some(&local), Some(&app_data)),
        Some(links.join("opencode.exe")),
        "winget's link first"
    );
}

/// Designs D7 and D8: the agent may only fetch and search, nothing is shared or updated,
/// the instruction only when set, and the search provider never left to a question.
#[test]
fn the_configuration_locks_opencode_down() {
    let document = config_document("  Answer briefly.  ", "exa");
    assert_eq!(document["share"], "disabled");
    assert_eq!(document["update"], "disable");
    assert_eq!(document["default_agent"], AGENT);
    assert_eq!(document["websearch"]["provider"], "exa");
    let agent = &document["agents"][AGENT];
    assert_eq!(agent["mode"], "primary");
    assert_eq!(agent["system"], "Answer briefly.");
    assert_eq!(
        agent["permissions"],
        json!([
            { "action": "*", "resource": "*", "effect": "deny" },
            { "action": "webfetch", "resource": "*", "effect": "allow" },
            { "action": "websearch", "resource": "*", "effect": "allow" },
        ])
    );
    assert!(config_document("", "exa")["agents"][AGENT]
        .get("system")
        .is_none());

    assert_eq!(search_provider(|_| None), "exa");
    assert_eq!(
        search_provider(|key| (key == "TAVILY_API_KEY").then(|| "k".to_owned())),
        "tavily"
    );
    assert_eq!(
        search_provider(|key| (key == "EXA_API_KEY").then(String::new)),
        "exa"
    );

    let env = server_env(Path::new("C:/app/opencode/opencode.json"));
    for (key, value) in [
        ("OPENCODE_CONFIG", "C:/app/opencode/opencode.json"),
        ("OPENCODE_DISABLE_CLAUDE_CODE", "1"),
        ("OPENCODE_DISABLE_PROJECT_CONFIG", "1"),
        ("OPENCODE_DISABLE_AUTOUPDATE", "1"),
    ] {
        assert!(env.iter().any(|(k, v)| k == key && v == value), "{key}");
    }
}

#[test]
fn the_providers_and_the_models_of_the_server() {
    // The shape of OpenCode 2.3.2's `GET /api/provider` with an OpenAI sign-in (2026-11-19).
    let providers = json!({ "location": { "directory": "C:/work" }, "data": [
        { "id": "opencode", "integrationID": "opencode", "name": "OpenCode", "activation": "enabled" },
        { "id": "openai", "integrationID": "openai", "name": "OpenAI", "activation": "auto",
          "headers": { "chatgpt-account-id": "not read" } },
        { "id": "google", "integrationID": "google", "activation": "disabled" },
    ]});
    assert_eq!(connected_providers(&providers), vec!["opencode", "openai"]);
    // `id` names the model; `modelID` is shared by its faster variants.
    assert_eq!(
        model_name(
            &json!({ "providerID": "openai", "modelID": "gpt-6.2-sol", "id": "gpt-6.2-sol-fast" })
        ),
        Some("openai/gpt-6.2-sol-fast".to_owned())
    );
    assert_eq!(
        model_name(&json!({ "providerID": "openai", "id": "openai/gpt-5" })),
        Some("openai/gpt-5".to_owned())
    );
    let models = json!({ "data": [
        { "providerID": "openai", "id": "gpt-6.2-sol", "modelID": "gpt-6.2-sol" },
        { "providerID": "openai", "id": "gpt-6.2-sol-fast", "modelID": "gpt-6.2-sol" },
        { "providerID": "openai", "id": "gpt-6.2-sol", "modelID": "gpt-6.2-sol" },
        { "providerID": "opencode", "id": "old-free", "enabled": false },
        { "providerID": "opencode", "id": "big-pickle", "enabled": true },
    ]});
    assert_eq!(
        model_names(&models),
        vec![
            "openai/gpt-6.2-sol",
            "openai/gpt-6.2-sol-fast",
            "opencode/big-pickle"
        ]
    );
}

/// 0.2 s of a tone per piece, recording what it read.
struct Reader(Arc<Mutex<Vec<String>>>);

impl Synth for Reader {
    fn sample_rate(&self) -> u32 {
        24_000
    }

    fn synthesize(
        &mut self,
        text: &str,
        _: Option<&str>,
        _: f32,
        _: &dyn Fn() -> bool,
    ) -> Result<Option<Vec<f32>>, TtsError> {
        self.0.lock().unwrap().push(text.to_owned());
        Ok(Some(
            (0..4_800)
                .map(|index| (index as f32 * 0.1).sin() * 0.5)
                .collect(),
        ))
    }
}

struct ReaderLoader(Arc<Mutex<Vec<String>>>);

impl Loader for ReaderLoader {
    fn load(&self, _: &str, _: &Path) -> Result<Box<dyn Synth>, TtsError> {
        Ok(Box::new(Reader(Arc::clone(&self.0))))
    }
}

/// An `opencode.cmd` in `dir` that runs the fake with the `env` lines; `None` without Node.
fn fake(dir: &Path, env: &[(&str, &str)]) -> Option<String> {
    let node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .ok()?;
    if !node.status.success() {
        return None;
    }
    let script = format!(
        "{}/testdata/fake-opencode/fake_opencode.js",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut text = String::new();
    for (key, value) in env {
        text.push_str(&format!("@set {key}={value}\r\n"));
    }
    text.push_str(&format!("@node \"{script}\" %*\r\n"));
    let path = dir.join("opencode.cmd");
    std::fs::write(&path, text).unwrap();
    Some(path.display().to_string())
}

fn settings_for(executable: &str) -> Settings {
    let mut settings = Settings::default();
    settings.assistant.engine = crate::settings::AssistantEngine::Opencode;
    settings.assistant.mode = AssistantMode::Whisper;
    settings.assistant.instructions = "Answer briefly.".to_owned();
    settings.opencode.executable = executable.to_owned();
    settings.tts.voice = "windows:test".to_owned();
    settings
}

fn assistant_in(
    runtime: &tokio::runtime::Runtime,
    dir: &Path,
    settings: &Settings,
) -> (OpencodeAssistant, Arc<ConnectionErrors>) {
    let events = EventBus::new(64);
    let errors = Arc::new(ConnectionErrors::new(events.clone()));
    let chat = Arc::new(ChatLog::new(events.clone(), dir.join("logs")));
    let assistant = OpencodeAssistant::new(
        runtime.handle().clone(),
        events,
        Arc::clone(&errors),
        chat,
        dir,
        settings,
    );
    assistant.configure(settings);
    (assistant, errors)
}

struct Rig {
    runtime: tokio::runtime::Runtime,
    _dir: tempfile::TempDir,
    assistant: OpencodeAssistant,
    errors: Arc<ConnectionErrors>,
    session: Arc<dyn AssistantSession>,
    answers: tokio::sync::mpsc::Receiver<AgentAnswer>,
    read: Arc<Mutex<Vec<String>>>,
    source: Option<crate::audio::PcmSource>,
}

impl Rig {
    fn server(&self) -> Arc<OpencodeServer> {
        self.assistant
            .inner
            .current
            .lock()
            .unwrap()
            .clone()
            .expect("the server")
    }

    fn state(&self) -> Value {
        let server = self.server();
        self.runtime.block_on(server.get("/fake/state")).unwrap()
    }

    fn delay(&self, ms: u64) {
        let server = self.server();
        self.runtime
            .block_on(server.post("/fake/delay", &json!({ "ms": ms })))
            .unwrap();
    }

    fn answer(&mut self) -> AgentAnswer {
        self.runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(120), self.answers.recv()).await
            })
            .expect("an answer in time")
            .unwrap()
    }
}

fn rig_with(one_at_a_time: bool, env: &[(&str, &str)]) -> Option<Rig> {
    let dir = tempfile::tempdir().unwrap();
    let executable = fake(dir.path(), env)?;
    Some(rig_on(dir, &executable, one_at_a_time, ""))
}

/// The engine on `executable` with `model` (empty: OpenCode's default), joined to guild 1.
fn rig_on(dir: tempfile::TempDir, executable: &str, one_at_a_time: bool, model: &str) -> Rig {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut settings = settings_for(executable);
    settings.assistant.one_speaker_at_a_time = one_at_a_time;
    settings.opencode.model = model.to_owned();
    let (assistant, errors) = assistant_in(&runtime, dir.path(), &settings);
    let read = Arc::new(Mutex::new(Vec::new()));
    let tts = TtsManager::with_parts(
        Arc::new(ReaderLoader(Arc::clone(&read))),
        Arc::new(|_: &crate::assets::Asset, _: &Path, _: &dyn Fn(crate::assets::Progress)| Ok(())),
        dir.path().to_path_buf(),
        EventBus::new(64),
    );
    tts.configure(&settings);
    tts.prepare_join().unwrap();
    read.lock().unwrap().clear();
    assistant.use_tts(tts);
    runtime
        .block_on(assistant.prepare_guild(1))
        .expect("OpenCode prepares");
    let (sink, answers) = AnswerSink::channel();
    let (_sender, input) = frame_channel(INPUT_CAPACITY);
    let (output, source) = output_pair();
    let session = assistant
        .attach(
            1,
            AudioPorts {
                input,
                output,
                failures: CodecFailures::none(),
                answers: sink,
                heard: crate::assistant::no_heard(),
            },
        )
        .unwrap();
    session.call_context(
        Some("Alex".to_owned()),
        &["Alex".to_owned(), "Sam".to_owned()],
    );
    session.session_wanted(true);
    Rig {
        runtime,
        _dir: dir,
        assistant,
        errors,
        session,
        answers,
        read,
        source: Some(source),
    }
}

fn rig(one_at_a_time: bool) -> Option<Rig> {
    rig_with(one_at_a_time, &[])
}

fn question(speaker: &str, text: &str) -> UserText {
    UserText {
        speaker: Some(speaker.to_owned()),
        text: text.to_owned(),
        language: Some("en".to_owned()),
        speech_end: Instant::now(),
        text_at: Instant::now(),
        audio_seconds: 2.0,
        model: "whisper-base-q8_0".to_owned(),
        device: "cpu".to_owned(),
        latency: Arc::default(),
    }
}

fn wait(condition: impl Fn() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Requirements "Questions are OpenCode turns", "OpenCode context and history" and "OpenCode
/// runs locked down".
#[test]
fn a_question_is_answered_spoken_and_posted() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    assert!(rig.session.ready());
    rig.session
        .user_text(question("Alex", "what time is it in London?"));
    let answer = rig.answer();
    assert_eq!(answer.engine, "OpenCode");
    assert_eq!(answer.task.as_deref(), Some("what time is it in London?"));
    assert!(
        answer.text.contains("Alex: what time is it in London?")
            && answer.text.ends_with("9 degrees."),
        "{}",
        answer.text
    );
    wait(|| {
        rig.read
            .lock()
            .unwrap()
            .iter()
            .any(|text| text.contains("9 degrees"))
    });
    assert_eq!(rig.read.lock().unwrap()[0], "Let me check that.");
    // The citation mark at the answer's end is neither read out nor posted.
    let marked = |text: &str| {
        text.contains("cite") || text.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
    };
    assert!(!marked(&answer.text), "{}", answer.text);
    assert!(
        !rig.read.lock().unwrap().iter().any(|text| marked(text)),
        "{:?}",
        rig.read.lock().unwrap()
    );
    let state = rig.state();
    let prompt = &state["prompts"][0];
    assert_eq!(prompt["delivery"], "queue");
    assert!(
        prompt["text"].as_str().unwrap().starts_with(
            "[Discord voice channel. In the channel: Alex, Sam. Addressed by: Alex.]\n"
        ),
        "{prompt}"
    );
    let session = &state["sessions"][0];
    assert_eq!(session["agent"], AGENT);
    assert_eq!(session["title"], "VoiceBro 1");
    assert!(session["location"]["directory"]
        .as_str()
        .unwrap()
        .ends_with("opencode-workspace"));
    assert_eq!(state["env"]["disableClaudeCode"], "1");
    assert_eq!(state["env"]["disableProjectConfig"], "1");
    assert_eq!(
        state["configFile"]["agents"][AGENT]["system"],
        "Answer briefly."
    );
    assert_eq!(state["configFile"]["websearch"]["provider"], "exa");
    let chat = rig.assistant.inner.chat.entries();
    assert!(
        chat.iter().any(|entry| entry.kind == "tool"
            && entry.text == "Web search: London weather"
            && entry.engine == "OpenCode"),
        "{chat:?}"
    );
    rig.assistant.detach(1);
    rig.assistant.stop();
}

/// Requirement "Barge-in aborts OpenCode": the interrupt goes before the new question.
#[test]
fn a_second_question_interrupts_the_first_and_only_it_is_posted() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    rig.delay(40);
    rig.session.user_text(question("Alex", "first question"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    rig.session.user_text(question("Sam", "second question"));
    let answer = rig.answer();
    assert_eq!(answer.task.as_deref(), Some("second question"));
    assert!(
        answer.text.contains("Sam: second question"),
        "{}",
        answer.text
    );
    assert!(!answer.text.contains("first question"), "{}", answer.text);
    let state = rig.state();
    assert_eq!(state["interrupts"], 1);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        rig.answers.try_recv().is_err(),
        "the interrupted answer is not posted"
    );
    rig.assistant.stop();
}

/// Requirement "OpenCode questions wait their turn": a question joins the running one.
#[test]
fn with_one_at_a_time_a_second_question_joins_the_running_answer() {
    let Some(mut rig) = rig(true) else {
        return;
    };
    rig.delay(40);
    rig.session
        .user_text(question("Alex", "What time is it in Bangkok?"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    rig.session
        .user_text(question("Sam", "And what is the weather in London?"));
    let answer = rig.answer();
    assert!(
        answer.text.contains("Bangkok") && answer.text.contains("weather in London"),
        "{}",
        answer.text
    );
    assert_eq!(
        answer.task.as_deref(),
        Some("What time is it in Bangkok? / And what is the weather in London?")
    );
    let state = rig.state();
    assert_eq!(state["interrupts"], 0, "nothing interrupted");
    assert_eq!(state["prompts"][1]["delivery"], "steer");
    rig.assistant.stop();
}

/// The end of a session aborts the answer.
#[test]
fn a_deactivation_during_a_turn_interrupts_it_and_posts_nothing() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    rig.delay(40);
    rig.session.user_text(question("Alex", "a long question"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    rig.session.session_wanted(false);
    wait(|| rig.state()["interrupts"] == 1);
    std::thread::sleep(Duration::from_millis(400));
    assert!(rig.answers.try_recv().is_err());
    assert!(!rig.session.working());
    rig.assistant.stop();
}

/// Requirement "The local server is private".
#[test]
fn a_call_without_the_password_is_refused() {
    let Some(rig) = rig(false) else {
        return;
    };
    let port = rig.server().port;
    let status = rig.runtime.block_on(async {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://127.0.0.1:{port}/api/info"))
            .send()
            .await
            .unwrap()
            .status()
    });
    assert_eq!(status.as_u16(), 401);
    rig.assistant.stop();
}

/// Requirement "OpenCode stops after the last leave": the session deleted, the server gone.
#[test]
fn the_leave_deletes_the_session_and_stops_the_server() {
    let Some(rig) = rig(false) else {
        return;
    };
    let server = rig.server();
    let session = rig.assistant.inner.guild_sessions.lock().unwrap()[&1]
        .0
        .clone();
    rig.assistant.detach(1);
    wait(|| !server.is_alive());
    assert!(!rig.assistant.status().running);
    assert!(
        rig.assistant
            .inner
            .guild_sessions
            .lock()
            .unwrap()
            .is_empty(),
        "the session {session} is forgotten"
    );
}

/// The server ends under the session: the error says to join again.
#[test]
fn a_server_exit_sets_the_error() {
    let Some(rig) = rig(false) else {
        return;
    };
    let server = rig.server();
    let _ = rig.runtime.block_on(server.post("/fake/exit", &json!({})));
    wait(|| rig.errors.snapshot().contains_key(ERROR_SOURCE));
    assert_eq!(
        rig.errors.snapshot()[ERROR_SOURCE],
        "OpenCode stopped; run /join again"
    );
    assert!(!rig.session.ready());
}

/// A model chosen while the bot is in the channel applies from the next question.
#[test]
fn a_model_change_applies_from_the_next_question() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    let mut settings = settings_for("");
    settings.opencode.model = "opencode/big-pickle".to_owned();
    rig.assistant.configure(&settings);
    rig.session.user_text(question("Alex", "hello"));
    rig.answer();
    let state = rig.state();
    assert_eq!(
        state["modelChanges"][0]["model"],
        json!({ "providerID": "opencode", "id": "big-pickle" })
    );
    rig.assistant.stop();
}

/// A start that does not become ready is tried once more on another port.
#[test]
fn a_failed_start_is_tried_again() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("failed-once");
    let marker_text = marker.display().to_string();
    let Some(rig) = rig_with(false, &[("FAKE_OPENCODE_FAIL_FIRST", &marker_text)]) else {
        return;
    };
    assert!(marker.exists(), "the first start failed");
    assert!(rig.session.ready(), "the second start serves");
    rig.assistant.stop();
}

/// Requirement "Join prepares OpenCode", and Check's providers and models.
#[test]
fn refusals_and_the_check() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let Some(executable) = fake(dir.path(), &[]) else {
        return;
    };
    let missing = dir
        .path()
        .join("nowhere/opencode.exe")
        .display()
        .to_string();
    let (assistant, _) = assistant_in(&runtime, dir.path(), &settings_for(&missing));
    let refused = runtime.block_on(assistant.prepare_guild(1)).unwrap_err();
    assert_eq!(refused, PrepareError::NotFound);
    assert!(
        refused.to_string().contains("Install OpenCode"),
        "{refused}"
    );

    let (assistant, _) = assistant_in(&runtime, dir.path(), &settings_for(&executable));
    let check = runtime.block_on(assistant.check()).unwrap();
    assert_eq!(check.providers, vec!["opencode", "openai"]);
    assert_eq!(
        check.models,
        vec![
            "openai/gpt-5.4-mini",
            "openai/gpt-5.4-mini-fast",
            "opencode/big-pickle"
        ]
    );
    assert_eq!(check.default_model.as_deref(), Some("openai/gpt-5.4-mini"));
    assert!(!assistant.status().running, "Check's server is gone");
    assert_eq!(assistant.status().summary, "providers: opencode, openai");
    assert_eq!(assistant.capabilities().models.len(), 3);

    let mut wrong = settings_for(&executable);
    wrong.opencode.model = "anthropic/claude-sonnet-4-5".to_owned();
    let (assistant, _) = assistant_in(&runtime, dir.path(), &wrong);
    let refused = runtime.block_on(assistant.prepare_guild(1)).unwrap_err();
    assert!(
        matches!(&refused, PrepareError::NoModel(detail) if detail.contains("anthropic/claude-sonnet-4-5")),
        "{refused}"
    );
    assistant.stop();

    let none_dir = tempfile::tempdir().unwrap();
    let none = fake(
        none_dir.path(),
        &[
            ("FAKE_OPENCODE_MODELS", "none"),
            ("FAKE_OPENCODE_PROVIDERS", "none"),
        ],
    )
    .unwrap();
    let (assistant, _) = assistant_in(&runtime, none_dir.path(), &settings_for(&none));
    let refused = runtime.block_on(assistant.prepare_guild(1)).unwrap_err();
    assert!(matches!(refused, PrepareError::NoModel(_)), "{refused}");
    assert!(
        refused.to_string().contains("opencode auth login"),
        "{refused}"
    );
    assistant.stop();
}

/// Task 6.6, live: OpenCode of this PC with the provider the user set up (OpenCode's default
/// model unless `VOICEBRO_OPENCODE_MODEL` names one); skipped without
/// `VOICEBRO_OPENCODE_LIVE=1`. The channel facts, the voice and the latency line; a command
/// it must not run; a web search for today's weather; a rejoin that keeps 42 through the
/// history.
#[test]
fn opencode_text_turn_when_enabled() {
    if std::env::var("VOICEBRO_OPENCODE_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let executable = find_opencode("").expect("OpenCode on this PC");
    let model = std::env::var("VOICEBRO_OPENCODE_MODEL").unwrap_or_default();
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_on(dir, &executable.display().to_string(), false, &model);
    eprintln!("OpenCode: {}", rig.assistant.status().summary);
    // Read the voice every 20 ms as songbird does, so the first frame played makes the
    // latency line.
    let mut source = rig.source.take().unwrap();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reading = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut bytes = vec![0u8; crate::audio::FRAME_SAMPLES * 4];
            while !stop.load(std::sync::atomic::Ordering::Acquire) {
                if !matches!(source.read(&mut bytes), Ok(read) if read > 0) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        })
    };
    let latency: crate::latency::SharedLatency = Arc::default();
    let started = Instant::now();
    rig.session.user_text(UserText {
        latency: Arc::clone(&latency),
        ..question("Alex", "Who is in the voice channel right now?")
    });
    let who = rig.answer();
    eprintln!("who ({} ms): {}", started.elapsed().as_millis(), who.text);
    assert!(
        who.text.contains("Alex") && who.text.contains("Sam"),
        "{}",
        who.text
    );
    wait(|| !latency.lock().unwrap().recent().is_empty());
    let report = latency.lock().unwrap().recent()[0].clone();
    let line = crate::latency::log_line(1, &crate::latency::LatencyEvent::Report(report));
    eprintln!("{line}");
    assert!(line.contains("text -> OpenCode first words"), "{line}");
    assert!(
        !rig.read.lock().unwrap().is_empty(),
        "the answer was spoken"
    );
    stop.store(true, std::sync::atomic::Ordering::Release);
    let _ = reading.join();

    rig.session
        .user_text(question("Sam", "Please remember the number forty two."));
    eprintln!("remember: {}", rig.answer().text);

    rig.session.user_text(question(
        "Alex",
        "Please run the command dir in your working folder and tell me what it prints.",
    ));
    eprintln!("command: {}", rig.answer().text);

    rig.session.user_text(question(
        "Sam",
        "What is the weather in London today? Search the web.",
    ));
    let weather = rig.answer();
    eprintln!("weather: {}", weather.text);
    assert!(
        !weather
            .text
            .chars()
            .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)),
        "no citation mark: {:?}",
        weather.text
    );
    let tools: Vec<String> = rig
        .assistant
        .inner
        .chat
        .entries()
        .into_iter()
        .filter(|entry| entry.kind == "tool")
        .map(|entry| entry.text)
        .collect();
    eprintln!("tools: {tools:?}");
    assert!(
        tools.iter().all(|tool| tool.starts_with("Web ")),
        "only web tools: {tools:?}"
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.len() > "Web search: ".len() && tool.starts_with("Web search: ")),
        "a web search with its query: {tools:?}"
    );

    // Leave and join again: a new session, the history carries the number over.
    rig.assistant.detach(1);
    std::thread::sleep(Duration::from_millis(1_500));
    assert!(
        !rig.assistant.status().running,
        "the server stopped after the leave"
    );
    rig.runtime
        .block_on(rig.assistant.prepare_guild(1))
        .expect("prepares again");
    let (sink, answers) = AnswerSink::channel();
    let (_sender, input) = frame_channel(INPUT_CAPACITY);
    let (output, _source) = output_pair();
    let session = rig
        .assistant
        .attach(
            1,
            AudioPorts {
                input,
                output,
                failures: CodecFailures::none(),
                answers: sink,
                heard: crate::assistant::no_heard(),
            },
        )
        .unwrap();
    rig.answers = answers;
    session.session_wanted(true);
    session.user_text(question("Sam", "Which number did I ask you to remember?"));
    let number = rig.answer();
    eprintln!("number: {}", number.text);
    let lower = number.text.to_lowercase();
    assert!(
        lower.contains("42") || lower.contains("forty-two") || lower.contains("forty two"),
        "{}",
        number.text
    );
    for entry in rig.assistant.inner.chat.entries() {
        eprintln!("chat {} {}: {}", entry.engine, entry.kind, entry.text);
    }
    rig.assistant.detach(1);
    rig.assistant.stop();
}
