//! VoiceBro desktop shell: wires `voicebro-core` to Tauri commands and events, installs the
//! file log, and hosts the static UI. The core never calls into Tauri.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use voicebro_core::settings::store::SettingsStore;
use voicebro_core::{paths, secrets, EventBus, Runtime, Settings, Status};

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

struct AppState {
    runtime: Arc<Runtime>,
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

fn log_plugin(data_dir: &std::path::Path) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let mut targets = vec![Target::new(TargetKind::Folder {
        path: paths::logs_dir(data_dir),
        file_name: Some("voicebro".to_owned()),
    })];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    let mut builder = tauri_plugin_log::Builder::new()
        .targets(targets)
        .level(log::LevelFilter::Info)
        .max_file_size(LOG_FILE_LIMIT)
        .rotation_strategy(RotationStrategy::KeepSome(LOG_FILES_KEPT))
        .timezone_strategy(TimezoneStrategy::UseLocal);
    for name in CHATTY_CRATES {
        builder = builder.level_for(*name, log::LevelFilter::Warn);
    }
    builder.build()
}

fn main() {
    let data_dir = paths::data_dir();
    let handle = tauri::async_runtime::handle().inner().clone();
    let runtime = Arc::new(Runtime::new(
        BUILD_STAMP,
        data_dir.clone(),
        SettingsStore::new(paths::settings_path(&data_dir)),
        Arc::from(secrets::platform_store()),
        EventBus::default(),
        handle,
    ));

    tauri::Builder::default()
        .plugin(log_plugin(&data_dir))
        .manage(AppState {
            runtime: Arc::clone(&runtime),
        })
        .setup(move |app| {
            // The first log line identifies the build.
            log::info!("VoiceBro {BUILD_STAMP}");
            if let Err(error) = runtime.start() {
                log::error!("runtime start: {error}");
                runtime.set_connection_error("runtime", Some(error.to_string()));
            }

            // Forward every core event to the webview as one Tauri event.
            let app_handle = app.handle().clone();
            let mut events = runtime.events().subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(event) => {
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
            discord_connect,
            discord_disconnect
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
