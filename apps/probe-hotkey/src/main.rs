//! P0.9: does `tauri-plugin-global-shortcut` deliver Ctrl+Alt+M while a full-screen game
//! has focus? Every press is printed with a timestamp; the user judges the result.

#![cfg_attr(not(debug_assertions), windows_subsystem = "console")]

use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn main() {
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyM);
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|_app, pressed, event| {
                    if event.state() == ShortcutState::Pressed {
                        println!(
                            "{} hotkey {pressed} pressed",
                            chrono::Local::now().format("%H:%M:%S%.3f")
                        );
                    }
                })
                .build(),
        )
        .setup(move |app| {
            app.global_shortcut().register(shortcut)?;
            println!("registered Ctrl+Alt+M; press it with a game in front");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("probe-hotkey failed to start");
}
