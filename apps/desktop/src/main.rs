//! VoiceBro desktop shell: wires `voicebro-core` to Tauri commands and events, installs the
//! file log, and hosts the static UI. The core never calls into Tauri.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod log_ring;
mod memory;
mod system;

use log_ring::LogRing;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use voicebro_core::assistant::chat_log::ChatEntry;
use voicebro_core::assistant::codex::CodexAssistant;
use voicebro_core::assistant::{Assistant, Capabilities, StubAssistant};
use voicebro_core::settings::store::SettingsStore;
use voicebro_core::status::{ConnectionErrors, HotkeyStatus};
use voicebro_core::{paths, secrets, AppEvent, EventBus, Runtime, Settings, Status};

const BUILD_STAMP: &str = env!("VOICEBRO_BUILD_STAMP");
/// The file log rotates at 1 MB and keeps one archived file: at most about 2 MB.
const LOG_FILE_LIMIT: u128 = 1_000_000;
const LOG_FILES_KEPT: usize = 1;
/// This app's own log targets; every other crate logs at Warn and above.
const APP_TARGETS: &[&str] = &["voicebro", "voicebro_core", "voicebro_desktop", "detail"];
/// songbird's receive task logs an error for every packet it cannot decode; the window
/// keeps them, the release file does not.
const RECEIVE_TASK: &str = "songbird::driver::tasks::udp_rx";
/// `diagnostics.verbose_log`: detail lines and third-party warnings go to the file too.
static VERBOSE_LOG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn is_app_target(target: &str) -> bool {
    APP_TARGETS.iter().any(|name| {
        target == *name
            || target
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with("::"))
    })
}

/// Whether a record that passed the levels also goes to the file. A debug build and the
/// verbose log write everything; otherwise the file keeps this app's event lines and
/// third-party errors, without detail lines and the receive task's per-packet errors.
fn keep_in_file(target: &str, level: log::Level, debug: bool, verbose: bool) -> bool {
    if debug || verbose {
        return true;
    }
    if target.starts_with(voicebro_core::DETAIL_TARGET_PREFIX) {
        return false;
    }
    if is_app_target(target) {
        return true;
    }
    level <= log::Level::Error && !target.starts_with(RECEIVE_TASK)
}
/// The levels on top of the global Warn: this app at Info, songbird's receive task at
/// Error (it warns about every RTCP packet it cannot decrypt, about 700 lines in 2.5
/// minutes of a DAVE call, which carries no action).
fn log_levels() -> Vec<(&'static str, log::LevelFilter)> {
    APP_TARGETS
        .iter()
        .map(|name| (*name, log::LevelFilter::Info))
        .chain(std::iter::once((RECEIVE_TASK, log::LevelFilter::Error)))
        .collect()
}

struct AppState {
    runtime: Arc<Runtime>,
    /// The Codex engine, for its own commands (sign-in, check, models, chat log). `None`
    /// when the settings name another engine and the stub runs instead.
    codex: Option<CodexAssistant>,
    /// The shortcut currently registered with the system, if any.
    hotkey: Mutex<Option<Shortcut>>,
    /// The last log lines, for the window's log pane.
    log: Arc<LogRing>,
    /// When the app started, for the uptime.
    started: Instant,
    /// The latest memory reading of the app's processes, renewed every 3 s while the
    /// reader runs; `None` otherwise.
    memory: memory::Slot,
    /// The reading thread, present while `diagnostics.memory` is on.
    memory_reader: Mutex<Option<memory::Reader>>,
}

/// Starts or stops the memory reader so that it runs exactly while `diagnostics.memory`
/// is on. Called at start-up and after every settings change.
/// Follows `diagnostics.verbose_log`: the file filter reads the flag on every record.
fn apply_verbose_log(state: &AppState) {
    set_verbose_log(state.runtime.settings().diagnostics.verbose_log);
}

/// Returns whether the value changed; logs the change.
fn set_verbose_log(on: bool) -> bool {
    let was = VERBOSE_LOG.swap(on, std::sync::atomic::Ordering::Relaxed);
    if was != on {
        log::info!(
            "verbose log {}",
            if on {
                "on: detail lines go to the file too"
            } else {
                "off"
            }
        );
    }
    was != on
}

fn apply_memory_reader(state: &AppState) {
    let wanted = state.runtime.settings().diagnostics.memory;
    let mut reader = state.memory_reader.lock().expect("memory reader lock");
    match (wanted, reader.take()) {
        (true, None) => match memory::Reader::start(Arc::clone(&state.memory)) {
            Ok(started) => {
                log::info!(
                    "memory reading started (every {} s)",
                    memory::INTERVAL.as_secs()
                );
                *reader = Some(started);
            }
            Err(error) => log::warn!("memory reading not started: {error}"),
        },
        (false, Some(running)) => {
            running.stop();
            log::info!("memory reading stopped");
        }
        (_, kept) => *reader = kept,
    }
}

fn codex(state: &AppState) -> Result<CodexAssistant, String> {
    state
        .codex
        .clone()
        .ok_or_else(|| "the Codex assistant is not the configured engine".to_owned())
}

#[tauri::command]
async fn codex_sign_in(state: State<'_, AppState>) -> Result<String, String> {
    codex(&state)?.sign_in().await
}

#[tauri::command]
async fn codex_check(state: State<'_, AppState>) -> Result<Value, String> {
    let account = codex(&state)?.check().await?;
    Ok(json!({
        "signed_in": account.signed_in,
        "kind": account.kind,
        "email": account.email,
        "plan": account.plan,
    }))
}

#[tauri::command]
async fn codex_models(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let models = codex(&state)?.models().await?;
    Ok(models
        .into_iter()
        .map(|model| serde_json::to_value(model).unwrap_or(Value::Null))
        .collect())
}

#[tauri::command]
fn capabilities(state: State<'_, AppState>) -> Capabilities {
    state.runtime.assistant().capabilities()
}

/// The wake models for the Settings tab: the catalog with whether each is downloaded, and
/// the models folder's models found at start.
#[tauri::command]
fn wake_models(state: State<'_, AppState>) -> Vec<voicebro_core::wake::ModelInfo> {
    state.runtime.wake().models()
}

/// The user agreed to download what the selected wake model needs.
#[tauri::command]
fn wake_download(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.wake().download()
}

/// Opens the models folder, where the user puts Vosk models for other languages.
#[tauri::command]
fn open_models_folder(state: State<'_, AppState>) -> Result<String, String> {
    let dir = state.runtime.wake().models_dir().to_path_buf();
    open_folder(&dir)
}

#[tauri::command]
fn chat_log(state: State<'_, AppState>) -> Vec<ChatEntry> {
    state
        .codex
        .as_ref()
        .map(|codex| codex.chat_log().entries())
        .unwrap_or_default()
}

/// Opens the log folder (app log and chat logs) in Explorer.
#[tauri::command]
fn open_log_folder(state: State<'_, AppState>) -> Result<String, String> {
    let dir = paths::logs_dir(std::path::Path::new(&state.runtime.status().data_dir));
    open_folder(&dir)
}

/// Creates `dir` when missing and opens it in Explorer.
fn open_folder(dir: &std::path::Path) -> Result<String, String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map_err(|error| format!("could not open {}: {error}", dir.display()))?;
    Ok(dir.display().to_string())
}

/// Registers the mute hotkey from the settings, unregistering the previous one first. An
/// empty setting registers nothing. The outcome goes to the runtime's status.
fn apply_hotkey(app: &AppHandle, state: &AppState) {
    let wanted = state.runtime.settings().hotkey.mute.trim().to_owned();
    let shortcuts = app.global_shortcut();
    let mut current = state.hotkey.lock().expect("hotkey lock");
    if let Some(previous) = current.take() {
        if let Err(error) = shortcuts.unregister(previous) {
            log::warn!("hotkey: could not unregister {previous}: {error}");
        }
    }
    if wanted.is_empty() {
        state.runtime.set_hotkey_status(HotkeyStatus::default());
        return;
    }
    let status = match wanted.parse::<Shortcut>() {
        Ok(shortcut) => match shortcuts.register(shortcut) {
            Ok(()) => {
                *current = Some(shortcut);
                HotkeyStatus {
                    shortcut: wanted,
                    registered: true,
                    error: None,
                }
            }
            Err(error) => HotkeyStatus {
                shortcut: wanted,
                registered: false,
                error: Some(error.to_string()),
            },
        },
        Err(error) => HotkeyStatus {
            shortcut: wanted,
            registered: false,
            error: Some(format!("not a valid shortcut: {error}")),
        },
    };
    state.runtime.set_hotkey_status(status);
}

#[tauri::command]
fn toggle_mute(state: State<'_, AppState>) -> bool {
    state.runtime.toggle_mute()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Value {
    let status = state.runtime.status();
    json!({ "settings": status.settings, "secrets": status.secrets })
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, patch: Value) -> Result<Settings, String> {
    state
        .runtime
        .save_settings(&patch)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_secret(state: State<'_, AppState>, name: String, value: String) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("the value is empty".to_owned());
    }
    state
        .runtime
        .set_secret(&name, &value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_secret(state: State<'_, AppState>, name: String) -> Result<(), String> {
    state
        .runtime
        .clear_secret(&name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn status(state: State<'_, AppState>) -> Status {
    state.runtime.status()
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> Value {
    let status = state.runtime.status();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "build_stamp": BUILD_STAMP,
        "data_dir": status.data_dir,
    })
}

#[tauri::command]
fn set_muted(state: State<'_, AppState>, muted: bool) -> bool {
    state.runtime.set_muted(muted)
}

#[tauri::command]
fn discord_connect(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.connect().map_err(|error| error.to_string())
}

#[tauri::command]
fn discord_disconnect(state: State<'_, AppState>) {
    state.runtime.disconnect();
}

/// The log lines written before the window asked, oldest first.
#[tauri::command]
fn log_history(state: State<'_, AppState>) -> Vec<String> {
    state.log.history()
}

/// The System tab and the status bar: the process and the operating system.
#[tauri::command]
fn system_status(state: State<'_, AppState>) -> system::SystemFacts {
    system::facts(BUILD_STAMP, state.started, Instant::now())
}

/// The latest memory reading (private working set of the app, WebView2 and Codex); `None`
/// while memory measurement is off and until the first reading after it is turned on.
#[tauri::command]
fn memory_status(state: State<'_, AppState>) -> Option<memory::MemoryReading> {
    state.memory.lock().expect("memory slot lock").clone()
}

/// The main window, built here rather than in `tauri.conf.json` so its WebView2 data stays
/// next to the app (`webview-data`) instead of under `%LOCALAPPDATA%`.
fn build_main_window(app: &tauri::App, data_dir: &std::path::Path) -> tauri::Result<()> {
    tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title("VoiceBro")
        .inner_size(1040.0, 720.0)
        .min_inner_size(860.0, 560.0)
        .resizable(true)
        .data_directory(paths::webview_data_dir(data_dir))
        .build()?;
    Ok(())
}

/// The file, the window's ring and, in debug builds, stdout: one level table for all three,
/// and on the file a filter that leaves out detail lines and third-party warnings in a
/// release build unless the verbose log is on.
fn log_plugin(
    data_dir: &std::path::Path,
    ring: Arc<LogRing>,
) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::fern;
    let window = fern::Dispatch::new().chain(fern::Output::call(move |record| {
        ring.push(record.args().to_string());
    }));
    let mut targets = vec![
        Target::new(TargetKind::Folder {
            path: paths::logs_dir(data_dir),
            file_name: Some("voicebro".to_owned()),
        })
        .filter(|metadata| {
            keep_in_file(
                metadata.target(),
                metadata.level(),
                cfg!(debug_assertions),
                VERBOSE_LOG.load(std::sync::atomic::Ordering::Relaxed),
            )
        }),
        Target::new(TargetKind::Dispatch(window)),
    ];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    let mut builder = tauri_plugin_log::Builder::new()
        .targets(targets)
        .level(log::LevelFilter::Warn)
        .max_file_size(LOG_FILE_LIMIT)
        .rotation_strategy(RotationStrategy::KeepSome(LOG_FILES_KEPT))
        .timezone_strategy(TimezoneStrategy::UseLocal);
    for (name, level) in log_levels() {
        builder = builder.level_for(name, level);
    }
    builder.build()
}

fn main() {
    let data_dir = paths::data_dir();
    let handle = tauri::async_runtime::handle().inner().clone();
    let events = EventBus::default();
    let errors = Arc::new(ConnectionErrors::new(events.clone()));
    let store = SettingsStore::new(paths::settings_path(&data_dir));
    // The engine is chosen from the settings file before the runtime loads it, so the
    // runtime can own it as a trait object from the start.
    let initial_settings = store.load().ok().flatten().unwrap_or_default();
    let codex = (initial_settings.assistant.engine == "codex").then(|| {
        CodexAssistant::new(
            handle.clone(),
            events.clone(),
            Arc::clone(&errors),
            &data_dir,
            &initial_settings,
        )
    });
    let assistant: Arc<dyn Assistant> = match codex.clone() {
        Some(codex) => Arc::new(codex),
        None => Arc::new(StubAssistant::default()),
    };
    let runtime = Arc::new(Runtime::with_assistant(
        BUILD_STAMP,
        data_dir.clone(),
        store,
        Arc::from(secrets::platform_store()),
        events,
        handle,
        errors,
        assistant,
    ));

    let log = Arc::new(LogRing::default());
    tauri::Builder::default()
        .plugin(log_plugin(&data_dir, Arc::clone(&log)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Toggle only, on the key-down edge; the hotkey is the mute switch.
                    if event.state() == ShortcutState::Pressed {
                        if let Some(state) = app.try_state::<AppState>() {
                            let muted = state.runtime.toggle_mute();
                            log::info!("mute {} from the hotkey", if muted { "on" } else { "off" });
                        }
                    }
                })
                .build(),
        )
        .manage(AppState {
            runtime: Arc::clone(&runtime),
            codex,
            hotkey: Mutex::new(None),
            log: Arc::clone(&log),
            started: Instant::now(),
            memory: Arc::new(Mutex::new(None)),
            memory_reader: Mutex::new(None),
        })
        .setup(move |app| {
            log.attach(app.handle().clone());
            // The first log line identifies the build.
            log::info!("VoiceBro {BUILD_STAMP}");
            let facts = system::facts(BUILD_STAMP, Instant::now(), Instant::now());
            log::info!(
                "{}, {}, {} CPU cores, process {}",
                facts.platform,
                facts.architecture,
                facts.cpu_cores,
                facts.process_id
            );
            build_main_window(app, &data_dir)?;
            if let Err(error) = runtime.start() {
                log::error!("runtime start: {error}");
                runtime.set_connection_error("runtime", Some(error.to_string()));
            }
            apply_hotkey(app.handle(), &app.state::<AppState>());
            apply_memory_reader(&app.state::<AppState>());
            apply_verbose_log(&app.state::<AppState>());

            // Forward every core event to the webview as one Tauri event, and re-register
            // the hotkey and the memory reader when the settings change.
            let app_handle = app.handle().clone();
            let mut events = runtime.events().subscribe();
            let mut last_hotkey = runtime.settings().hotkey.mute.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            if matches!(event, AppEvent::SettingsChanged) {
                                let state = app_handle.state::<AppState>();
                                let now = state.runtime.settings().hotkey.mute.clone();
                                if now != last_hotkey {
                                    last_hotkey = now;
                                    apply_hotkey(&app_handle, &state);
                                }
                                apply_memory_reader(&state);
                                apply_verbose_log(&state);
                            }
                            if let Err(error) = app_handle.emit("app-event", &event) {
                                log::warn!("event bridge: {error}");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            log::warn!("event bridge lagged, {skipped} events dropped");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_secret,
            clear_secret,
            status,
            app_info,
            set_muted,
            toggle_mute,
            discord_connect,
            discord_disconnect,
            codex_sign_in,
            codex_check,
            codex_models,
            capabilities,
            wake_models,
            wake_download,
            chat_log,
            log_history,
            system_status,
            memory_status,
            open_log_folder,
            open_models_folder
        ])
        .build(tauri::generate_context!())
        .expect("error while building the VoiceBro application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.runtime.shutdown();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_logs_at_info_and_songbirds_receive_task_at_error() {
        let levels = log_levels();
        let udp_rx = levels
            .iter()
            .find(|(name, _)| *name == "songbird::driver::tasks::udp_rx")
            .expect("the receive task has its own level");
        assert_eq!(udp_rx.1, log::LevelFilter::Error);
        for app in ["voicebro", "voicebro_core", "voicebro_desktop", "detail"] {
            let level = levels.iter().find(|(name, _)| *name == app).unwrap();
            assert_eq!(level.1, log::LevelFilter::Info, "{app}");
        }
        assert!(
            !levels.iter().any(|(name, _)| name.starts_with("rtc")),
            "every other crate, the WebRTC sub-crates included, stays at the global Warn"
        );
    }

    #[test]
    fn the_release_file_keeps_events_and_errors_only() {
        use log::Level::{Error, Info, Warn};
        let release = |target, level| keep_in_file(target, level, false, false);
        assert!(release("voicebro_core::runtime", Info), "an app event line");
        assert!(release("voicebro", Info), "the shell's own lines");
        assert!(release("voicebro_core::events", Warn));
        assert!(!release("detail::wake", Info), "a detail line");
        assert!(!release("rtc_ice::agent", Warn), "a third-party warning");
        assert!(
            !release("rtc::peer_connection::handler", Warn),
            "a third-party warning"
        );
        assert!(release("serenity::gateway", Error), "a third-party error");
        assert!(
            !release("songbird::driver::tasks::udp_rx::ssrc_state", Error),
            "the receive task's per-packet errors"
        );
        assert!(
            !release("voicebrother::x", Info),
            "a prefix is not an app target"
        );
        for target in [
            "detail::wake",
            "rtc_ice::agent",
            "songbird::driver::tasks::udp_rx",
        ] {
            assert!(keep_in_file(target, Warn, false, true), "verbose: {target}");
            assert!(keep_in_file(target, Warn, true, false), "debug: {target}");
        }
    }

    #[test]
    fn the_verbose_switch_flips_the_flag_the_file_filter_reads() {
        assert!(set_verbose_log(true), "off to on is a change");
        assert!(VERBOSE_LOG.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!set_verbose_log(true), "on again is none");
        assert!(set_verbose_log(false));
        assert!(!VERBOSE_LOG.load(std::sync::atomic::Ordering::Relaxed));
    }
}
