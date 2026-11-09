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
const LOG_FILE_LIMIT: u128 = 4_000_000;
const LOG_FILES_KEPT: usize = 2;
/// Third-party crates that log a lot at Info; they stay at Warn.
const CHATTY_CRATES: &[&str] = &[
    "serenity",
    "songbird",
    "tracing",
    "hyper",
    "hyper_util",
    "h2",
    "rustls",
    "webrtc",
    "rtc",
    "reqwest",
    "tungstenite",
    "tokio_tungstenite",
    "tao",
    "wry",
];
/// Modules quieter still: songbird's receive task warns about every RTCP packet it cannot
/// decrypt (about 700 lines in 2.5 minutes of a DAVE call), which carries no action.
const QUIET_MODULES: &[&str] = &["songbird::driver::tasks::udp_rx"];

/// Every per-module level the log uses, most specific last.
fn log_levels() -> Vec<(&'static str, log::LevelFilter)> {
    CHATTY_CRATES
        .iter()
        .map(|name| (*name, log::LevelFilter::Warn))
        .chain(
            QUIET_MODULES
                .iter()
                .map(|name| (*name, log::LevelFilter::Error)),
        )
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
    /// The latest memory reading of the app's processes, renewed every 3 s.
    memory: Arc<Mutex<Option<memory::MemoryReading>>>,
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

/// The wake-model catalog for the Settings tab.
#[tauri::command]
fn wake_models() -> Vec<Value> {
    voicebro_core::wake::catalog::entries()
        .iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "engine": entry.engine,
                "language": entry.language,
                "display_name": entry.display_name,
                "license": entry.license,
            })
        })
        .collect()
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
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    std::process::Command::new("explorer.exe")
        .arg(&dir)
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
/// until the first reading is taken.
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

/// The file, the window's ring and, in debug builds, stdout: one level table for all three.
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
        }),
        Target::new(TargetKind::Dispatch(window)),
    ];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    let mut builder = tauri_plugin_log::Builder::new()
        .targets(targets)
        .level(log::LevelFilter::Info)
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
            memory: memory::start(),
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

            // Forward every core event to the webview as one Tauri event, and re-register
            // the hotkey when the settings change.
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
            chat_log,
            log_history,
            system_status,
            memory_status,
            open_log_folder
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
    fn songbird_receive_warnings_are_filtered_and_its_errors_kept() {
        let levels = log_levels();
        let udp_rx = levels
            .iter()
            .find(|(name, _)| *name == "songbird::driver::tasks::udp_rx")
            .expect("the receive task has its own level");
        assert_eq!(udp_rx.1, log::LevelFilter::Error);
        let songbird = levels.iter().find(|(name, _)| *name == "songbird").unwrap();
        assert_eq!(
            songbird.1,
            log::LevelFilter::Warn,
            "the rest of songbird stays at Warn"
        );
    }
}
