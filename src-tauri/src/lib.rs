// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod wm;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_config() -> wm::WmConfig {
    wm::get_config()
}

#[tauri::command]
fn set_config(config: wm::WmConfig) {
    wm::set_config(config);
}

#[tauri::command]
fn get_debug_state() -> bool {
    wm::DEBUG_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

#[tauri::command]
fn get_debug_snapshot() -> wm::DebugSnapshot {
    if let Ok(snap) = wm::DEBUG_SNAPSHOT.lock() {
        snap.clone()
    } else {
        wm::DebugSnapshot::default()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    wm::start_wm();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Settings", true, None::<&str>)?;
            let debug_i = MenuItem::with_id(app, "debug", "Toggle Debug Overlay", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &debug_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "debug" => {
                        let current = wm::DEBUG_ENABLED.load(std::sync::atomic::Ordering::Relaxed);
                        wm::DEBUG_ENABLED.store(!current, std::sync::atomic::Ordering::Relaxed);
                        let _ = app.emit("debug-toggle", !current);

                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_config, set_config, get_debug_state, get_debug_snapshot])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
