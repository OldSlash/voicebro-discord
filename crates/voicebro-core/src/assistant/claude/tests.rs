//! Claude Code's engine against the Node stand-in in `testdata/fake-claude` (change
//! `claude-and-opencode-engines`, task 5). The tests that need it check nothing where Node
//! is not installed.

use super::*;
use crate::assistant::{AgentAnswer, AnswerSink, UserText};
use crate::audio::codec::CodecFailures;
use crate::audio::{frame_channel, output_pair, INPUT_CAPACITY};
use crate::tts::manager::{Loader, TtsManager};
use crate::tts::{Synth, TtsError};
use std::time::Instant;

#[test]
fn the_sign_in_kinds_map_as_design_d5_says() {
    let max = parse_auth_status(&json!({
        "loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty",
        "email": "me@example.com", "subscriptionType": "max",
    }));
    assert_eq!(max.kind, SignInKind::Subscription);
    assert_eq!(max.summary(), "Claude subscription (Max)");
    assert_eq!(max.email.as_deref(), Some("me@example.com"));
    assert!(max.signed_in());
    // Team and Enterprise are claude.ai sign-ins too (the user's decision of 2026-11-19).
    let team = parse_auth_status(&json!({
        "loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "team",
    }));
    assert_eq!(team.kind, SignInKind::Subscription);
    let key = parse_auth_status(&json!({
        "loggedIn": true, "authMethod": "api_key", "apiProvider": "firstParty",
        "apiKeySource": "ANTHROPIC_API_KEY",
    }));
    assert_eq!(key.kind, SignInKind::ApiKey);
    assert_eq!(key.summary(), "API key");
    assert_eq!(key.plan, None);
    let bedrock = parse_auth_status(&json!({ "loggedIn": true, "apiProvider": "bedrock" }));
    assert_eq!(bedrock.kind, SignInKind::Cloud);
    assert_eq!(bedrock.summary(), "Amazon Bedrock");
    let out = parse_auth_status(&json!({ "loggedIn": false }));
    assert_eq!(out.kind, SignInKind::None);
    assert!(!out.signed_in());
    assert_eq!(parse_auth_status(&json!({})).kind, SignInKind::Unknown);
    assert_eq!(SignInKind::Subscription.name(), "subscription");
}

/// Requirement "Finding Claude Code".
#[test]
fn claude_code_is_found_where_its_installers_put_it() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("profile");
    let bin = profile.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("claude.exe"), b"").unwrap();
    assert_eq!(
        find_claude_in("", &[], None, Some(&profile)),
        Some(bin.join("claude.exe"))
    );
    let on_path = dir.path().join("path");
    std::fs::create_dir_all(&on_path).unwrap();
    std::fs::write(on_path.join("claude.exe"), b"").unwrap();
    assert_eq!(
        find_claude_in("", std::slice::from_ref(&on_path), None, Some(&profile)),
        Some(on_path.join("claude.exe")),
        "the PATH first"
    );
    let missing = dir.path().join("nowhere/claude.exe");
    assert_eq!(
        find_claude_in(
            &missing.display().to_string(),
            &[on_path],
            None,
            Some(&profile)
        ),
        None,
        "a configured path is the only place looked at"
    );
}

/// Requirement "Claude Code runs locked down" (design D4).
#[test]
fn the_arguments_lock_claude_code_down() {
    let args = claude_args(
        "sonnet",
        Some(Path::new("C:/data/claude/instructions.txt")),
        Path::new("C:/data/claude/mcp.json"),
    );
    let after = |flag: &str| {
        let index = args
            .iter()
            .position(|arg| arg == flag)
            .unwrap_or_else(|| panic!("{flag}"));
        args[index + 1].clone()
    };
    assert_eq!(after("--tools"), "WebSearch,WebFetch");
    // Offered and allowed in advance: the don't-ask mode denies an offered tool otherwise.
    assert_eq!(after("--allowedTools"), "WebSearch,WebFetch");
    assert_eq!(after("--permission-mode"), "dontAsk");
    assert_eq!(after("--setting-sources"), "");
    assert_eq!(after("--mcp-config"), "C:/data/claude/mcp.json");
    assert_eq!(after("--model"), "sonnet");
    assert_eq!(
        after("--append-system-prompt-file"),
        "C:/data/claude/instructions.txt"
    );
    for flag in [
        "-p",
        "--strict-mcp-config",
        "--no-session-persistence",
        "--include-partial-messages",
        "--verbose",
    ] {
        assert!(args.iter().any(|arg| arg == flag), "{flag}");
    }
    assert!(
        !args.iter().any(|arg| arg == "--bare"),
        "--bare refuses a subscription"
    );
    let plain = claude_args("", None, Path::new("m.json"));
    assert!(!plain
        .iter()
        .any(|arg| arg == "--model" || arg == "--append-system-prompt-file"));
}

#[test]
fn the_models_of_an_initialize_answer() {
    let models = parse_models(&json!({ "models": [
        { "value": "default", "displayName": "Default (recommended)" },
        { "value": "haiku", "displayName": "Haiku 5.5", "description": "Fast" },
    ]}));
    assert_eq!(models.len(), 2);
    assert!(models[0].is_default);
    assert_eq!(models[1].id, "haiku");
    assert_eq!(models[1].description, "Fast");
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

/// A `claude.cmd` in `dir` that runs the fake with `extra` and logs to `dir\claude.log`;
/// `None` without Node.
fn logging_fake(dir: &Path, extra: &str) -> Option<String> {
    let node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .ok()?;
    if !node.status.success() {
        return None;
    }
    let script = format!(
        "{}/testdata/fake-claude/fake_claude.js",
        env!("CARGO_MANIFEST_DIR")
    );
    let path = dir.join("claude.cmd");
    std::fs::write(
        &path,
        format!(
            "@node \"{script}\" {extra} --fake-log \"{}\" %*\r\n",
            dir.join("claude.log").display()
        ),
    )
    .unwrap();
    Some(path.display().to_string())
}

fn log_lines(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("claude.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn whisper_settings(executable: &str) -> Settings {
    let mut settings = Settings::default();
    settings.assistant.engine = crate::settings::AssistantEngine::Claude;
    settings.assistant.mode = AssistantMode::Whisper;
    settings.assistant.instructions = "Answer briefly.".to_owned();
    settings.claude.executable = executable.to_owned();
    settings.tts.voice = "windows:test".to_owned();
    settings
}

struct Rig {
    runtime: tokio::runtime::Runtime,
    dir: tempfile::TempDir,
    assistant: ClaudeAssistant,
    errors: Arc<ConnectionErrors>,
    session: Arc<dyn AssistantSession>,
    answers: tokio::sync::mpsc::Receiver<AgentAnswer>,
    read: Arc<Mutex<Vec<String>>>,
    /// The voice's output; the live check reads it as songbird does.
    source: Option<crate::audio::PcmSource>,
}

impl Rig {
    fn process(&self) -> Arc<ClaudeProcess> {
        self.assistant
            .inner
            .processes
            .lock()
            .unwrap()
            .get(&1)
            .cloned()
            .expect("the guild's process")
    }

    fn control(&self, request: Value) -> Value {
        let process = self.process();
        self.runtime.block_on(process.control(request)).unwrap()
    }

    fn answer(&mut self) -> AgentAnswer {
        self.runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(10), self.answers.recv()).await
            })
            .expect("an answer in time")
            .unwrap()
    }
}

fn assistant_in(
    runtime: &tokio::runtime::Runtime,
    dir: &Path,
    settings: &Settings,
) -> (ClaudeAssistant, Arc<ConnectionErrors>) {
    let events = EventBus::new(64);
    let errors = Arc::new(ConnectionErrors::new(events.clone()));
    let chat = Arc::new(ChatLog::new(events.clone(), dir.join("logs")));
    let assistant = ClaudeAssistant::new(
        runtime.handle().clone(),
        events,
        Arc::clone(&errors),
        chat,
        dir,
        settings,
    );
    (assistant, errors)
}

fn rig(one_at_a_time: bool) -> Option<Rig> {
    let dir = tempfile::tempdir().unwrap();
    let fake = logging_fake(dir.path(), "")?;
    rig_on(dir, fake, one_at_a_time, "")
}

/// As [`rig`], on `executable` (the real Claude Code for the live check) with `model`.
fn rig_on(
    dir: tempfile::TempDir,
    executable: String,
    one_at_a_time: bool,
    model: &str,
) -> Option<Rig> {
    let mut settings = whisper_settings(&executable);
    settings.claude.model = model.to_owned();
    settings.assistant.one_speaker_at_a_time = one_at_a_time;
    rig_with(dir, &settings)
}

/// As [`rig_on`], with the whole settings given (the text answers mode).
fn rig_with(dir: tempfile::TempDir, settings: &Settings) -> Option<Rig> {
    let settings = settings.clone();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (assistant, errors) = assistant_in(&runtime, dir.path(), &settings);
    assistant.configure(&settings);
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
        .expect("the fake prepares");
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
    Some(Rig {
        runtime,
        dir,
        assistant,
        errors,
        session,
        answers,
        read,
        source: Some(source),
    })
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

/// Requirements "Questions are Claude Code turns", "Claude Code context and history" and
/// "Claude Code runs locked down": answered, spoken, posted with the channel facts.
#[test]
fn a_question_is_answered_spoken_and_posted() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    assert!(rig.session.ready());
    rig.session
        .user_text(question("Alex", "what time is it in London?"));
    let answer = rig.answer();
    assert_eq!(answer.engine, "Claude");
    assert_eq!(answer.task.as_deref(), Some("what time is it in London?"));
    let chat = rig.assistant.inner.chat.entries();
    assert!(
        chat.iter().all(|entry| entry.engine == "Claude"),
        "{chat:?}"
    );
    assert!(
        chat.iter()
            .any(|entry| entry.kind == "tool" && entry.text == "Web search: London weather"),
        "{chat:?}"
    );
    // The search ran: the fake, like Claude Code, denies it unless it is allowed in advance.
    assert!(
        !chat
            .iter()
            .any(|entry| entry.kind == "tool" && entry.text.contains("failed")),
        "{chat:?}"
    );
    assert!(
        answer.text.contains("Alex: what time is it in London?"),
        "{}",
        answer.text
    );
    wait(|| rig.read.lock().unwrap().len() >= 2);
    assert_eq!(rig.read.lock().unwrap()[0], "Let me check that.");
    let last = rig.control(json!({ "subtype": "fake_last_user" }));
    let sent = last["content"].as_str().unwrap();
    assert!(
        sent.starts_with(
            "[Discord voice channel. In the channel: Alex, Sam. Addressed by: Alex.]\n"
        ),
        "{sent}"
    );
    let args = log_lines(rig.dir.path())
        .into_iter()
        .find(|line| line.starts_with("ARGS "))
        .unwrap();
    for flag in [
        "\"--tools\",\"WebSearch,WebFetch\"",
        "\"--allowedTools\",\"WebSearch,WebFetch\"",
        "\"--setting-sources\",\"\"",
        "\"--strict-mcp-config\"",
        "\"--no-session-persistence\"",
        "\"--permission-mode\",\"dontAsk\"",
        "\"--append-system-prompt-file\"",
    ] {
        assert!(args.contains(flag), "{flag}: {args}");
    }
    let instructions = std::fs::read_to_string(
        crate::paths::claude_config_dir(rig.dir.path()).join("instructions.txt"),
    )
    .unwrap();
    assert_eq!(instructions, "Answer briefly.");
    rig.assistant.detach(1);
    rig.assistant.stop();
}

/// Change `text-answers-mode`, D2 and D4: in the text answers mode the answer is posted and
/// nothing is spoken, and the latency line has no voice times.
#[test]
fn the_text_answers_mode_posts_and_speaks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let Some(fake) = logging_fake(dir.path(), "") else {
        return;
    };
    let mut settings = whisper_settings(&fake);
    settings.assistant.mode = AssistantMode::Text;
    let Some(mut rig) = rig_with(dir, &settings) else {
        return;
    };
    let asked = question("Alex", "what time is it in London?");
    let latency = Arc::clone(&asked.latency);
    rig.session.user_text(asked);
    let answer = rig.answer();
    assert_eq!(answer.task.as_deref(), Some("what time is it in London?"));
    wait(|| !latency.lock().unwrap().recent().is_empty());
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        rig.read.lock().unwrap().is_empty(),
        "the voice read {:?}",
        rig.read.lock().unwrap()
    );
    let report = latency.lock().unwrap().recent()[0].clone();
    assert!(
        report.line().ends_with("no voice (text answers)"),
        "{}",
        report.line()
    );
    assert!(!rig.session.working(), "nothing waits to be spoken");
    rig.assistant.detach(1);
    rig.assistant.stop();
}

/// Requirement "Barge-in interrupts Claude Code".
#[test]
fn a_second_question_interrupts_the_first_and_only_it_is_posted() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    rig.control(json!({ "subtype": "fake_delay", "ms": 40 }));
    rig.session.user_text(question("Alex", "first question"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    rig.session.user_text(question("Sam", "second question"));
    let answer = rig.answer();
    assert_eq!(answer.task.as_deref(), Some("second question"));
    let lines = log_lines(rig.dir.path());
    let interrupt = lines.iter().position(|line| line.contains("\"interrupt\""));
    let second = lines
        .iter()
        .position(|line| line.contains("Sam: second question"));
    assert!(
        interrupt.is_some() && interrupt < second,
        "the interrupt goes out before the new question: {lines:?}"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        rig.answers.try_recv().is_err(),
        "the interrupted turn is not posted"
    );
    rig.assistant.stop();
}

/// Change `interrupt-on-speech`, D3: `stop_answer` stops the voice and interrupts the
/// running turn without starting another; nothing is posted for it, and the question that
/// follows starts a new turn and is answered.
#[test]
fn stop_answer_interrupts_the_turn_and_the_next_question_is_answered() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    rig.control(json!({ "subtype": "fake_delay", "ms": 40 }));
    rig.session.user_text(question("Alex", "first question"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    assert!(rig.session.working());
    rig.session.stop_answer();
    wait(|| {
        log_lines(rig.dir.path())
            .iter()
            .any(|line| line.contains("\"interrupt\""))
    });
    assert!(!rig.session.working(), "the voice and the turn stopped");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        rig.answers.try_recv().is_err(),
        "the stopped turn is not posted"
    );
    let questions = log_lines(rig.dir.path())
        .iter()
        .filter(|line| line.contains("question"))
        .count();
    rig.session.user_text(question("Alex", "second question"));
    let answer = rig.answer();
    assert_eq!(answer.task.as_deref(), Some("second question"));
    assert!(
        log_lines(rig.dir.path())
            .iter()
            .filter(|line| line.contains("question"))
            .count()
            > questions,
        "the second question started a new turn"
    );
    rig.assistant.stop();
}

/// Requirement "Claude Code questions wait their turn".
#[test]
fn with_one_at_a_time_the_second_question_is_answered_after_the_first() {
    let Some(mut rig) = rig(true) else {
        return;
    };
    rig.control(json!({ "subtype": "fake_delay", "ms": 30 }));
    rig.session
        .user_text(question("Alex", "What time is it in Bangkok?"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    rig.session
        .user_text(question("Sam", "And what is the weather in London?"));
    let first = rig.answer();
    assert_eq!(first.task.as_deref(), Some("What time is it in Bangkok?"));
    let second = rig.answer();
    assert_eq!(
        second.task.as_deref(),
        Some("And what is the weather in London?")
    );
    let lines = log_lines(rig.dir.path());
    assert!(
        !lines.iter().any(|line| line.contains("\"interrupt\"")),
        "nothing is interrupted: {lines:?}"
    );
    rig.assistant.stop();
}

/// Requirement "Barge-in interrupts Claude Code": the end of a session stops the answer.
#[test]
fn a_deactivation_during_a_turn_interrupts_it_and_posts_nothing() {
    let Some(mut rig) = rig(false) else {
        return;
    };
    rig.control(json!({ "subtype": "fake_delay", "ms": 40 }));
    rig.session.user_text(question("Alex", "a long question"));
    wait(|| !rig.read.lock().unwrap().is_empty());
    rig.session.session_wanted(false);
    wait(|| {
        log_lines(rig.dir.path())
            .iter()
            .any(|line| line.contains("\"interrupt\""))
    });
    std::thread::sleep(Duration::from_millis(400));
    assert!(rig.answers.try_recv().is_err());
    assert!(!rig.session.working());
    rig.assistant.stop();
}

/// The process ends under the session: the error says to join again.
#[test]
fn a_process_exit_sets_the_error() {
    let Some(rig) = rig(false) else {
        return;
    };
    rig.control(json!({ "subtype": "fake_exit" }));
    wait(|| rig.errors.snapshot().contains_key(ERROR_SOURCE));
    assert_eq!(
        rig.errors.snapshot()[ERROR_SOURCE],
        "Claude Code stopped; run /join again"
    );
    assert!(!rig.session.ready());
    rig.assistant.stop();
}

/// Requirement "Claude Code stops after the last leave".
#[test]
fn the_leave_stops_the_process() {
    let Some(rig) = rig(false) else {
        return;
    };
    assert!(rig.assistant.status().running);
    rig.assistant.detach(1);
    wait(|| !rig.assistant.status().running);
    assert!(rig.assistant.inner.processes.lock().unwrap().is_empty());
}

/// Requirements "Join prepares Claude Code" and "The sign-in kind is known", and Check's
/// model list.
#[test]
fn refusals_sign_in_kinds_and_the_model_list() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let Some(_) = logging_fake(dir.path(), "") else {
        return;
    };
    let missing = dir.path().join("nowhere/claude.exe").display().to_string();
    let (assistant, _) = assistant_in(&runtime, dir.path(), &whisper_settings(&missing));
    let refused = runtime.block_on(assistant.prepare_guild(1)).unwrap_err();
    assert_eq!(refused, PrepareError::NotFound);
    assert!(
        refused.to_string().contains("Install Claude Code"),
        "{refused}"
    );

    let out = logging_fake(dir.path(), "--kind=none").unwrap();
    let (assistant, _) = assistant_in(&runtime, dir.path(), &whisper_settings(&out));
    assert_eq!(
        runtime.block_on(assistant.prepare_guild(1)).unwrap_err(),
        PrepareError::SignedOut
    );
    assert_eq!(assistant.status().sign_in_kind.as_deref(), Some("none"));

    let key = logging_fake(dir.path(), "--kind=api_key").unwrap();
    let (assistant, _) = assistant_in(&runtime, dir.path(), &whisper_settings(&key));
    let sign_in = runtime.block_on(assistant.check()).unwrap();
    assert_eq!(sign_in.kind, SignInKind::ApiKey);
    assert_eq!(assistant.status().summary, "API key");

    let max = logging_fake(dir.path(), "").unwrap();
    let (assistant, _) = assistant_in(&runtime, dir.path(), &whisper_settings(&max));
    let sign_in = runtime.block_on(assistant.check()).unwrap();
    assert_eq!(sign_in.kind, SignInKind::Subscription);
    let status = assistant.status();
    assert_eq!(status.email.as_deref(), Some("fake@example.com"));
    assert_eq!(status.summary, "Claude subscription (Max)");
    assert_eq!(status.sign_in_kind.as_deref(), Some("subscription"));
    assert_eq!(status.version.as_deref(), Some("0.0.0-fake (Claude Code)"));
    let models = assistant.capabilities().models;
    assert!(
        models.iter().any(|model| model.id == "sonnet"),
        "{models:?}"
    );
    assert!(
        !log_lines(dir.path())
            .iter()
            .any(|line| line.contains("\"type\":\"user\"")),
        "Check sends no question"
    );
    assert!(!assistant.status().running, "Check's process is gone");
}

/// The live check of task 5.6, with the Claude Code of this PC and its sign-in, skipped
/// silently without `VOICEBRO_CLAUDE_LIVE=1`. `VOICEBRO_CLAUDE_MODEL` picks the model
/// (`haiku` by default, the cheapest). The questions go in as Whisper hands them over
/// (text with the speaker), the answers are read by a stand-in voice. Checks the channel
/// facts, a rejoin that keeps the conversation through the history, and that Claude has
/// nothing that acts on the PC.
#[test]
fn claude_text_turn_when_enabled() {
    if std::env::var("VOICEBRO_CLAUDE_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let executable = find_claude("").expect("Claude Code on this PC");
    let model = std::env::var("VOICEBRO_CLAUDE_MODEL").unwrap_or_else(|_| "haiku".to_owned());
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_on(dir, executable.display().to_string(), false, &model).unwrap();
    eprintln!("sign-in: {}", rig.assistant.status().summary);
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
    assert!(line.contains("text -> Claude first words"), "{line}");
    assert!(
        !rig.read.lock().unwrap().is_empty(),
        "the answer was spoken"
    );
    stop.store(true, std::sync::atomic::Ordering::Release);
    let _ = reading.join();

    rig.session
        .user_text(question("Sam", "Please remember the number forty two."));
    eprintln!("remember: {}", rig.answer().text);

    rig.session
        .user_text(question("Alex", "Please list the files on this computer."));
    let files = rig.answer();
    eprintln!("files: {}", files.text);
    rig.session.user_text(question(
        "Sam",
        "What is the weather in London today? Search the web.",
    ));
    eprintln!("weather: {}", rig.answer().text);
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
    // The search ran, not only was asked for: Claude Code's don't-ask mode denies a tool
    // that is not allowed in advance (2026-11-20).
    assert!(
        tools
            .iter()
            .any(|tool| tool.len() > "Web search: ".len() && tool.starts_with("Web search: ")),
        "a web search with its query: {tools:?}"
    );
    assert!(
        !tools.iter().any(|tool| tool.contains(" failed: ")),
        "no tool failed: {tools:?}"
    );

    // Leave and join again: a new process, the history carries the number over.
    rig.assistant.detach(1);
    std::thread::sleep(Duration::from_millis(800));
    assert!(
        !rig.assistant.status().running,
        "the process stopped after the leave"
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
