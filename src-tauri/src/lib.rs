pub mod provider;
pub mod state;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, State,
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Default)]
struct RuntimeState {
    bubble_position: Option<WindowPosition>,
}

#[tauri::command]
fn get_bubble_position(state: State<'_, Arc<Mutex<RuntimeState>>>) -> Option<WindowPosition> {
    state.lock().ok()?.bubble_position.clone()
}

#[tauri::command]
fn save_bubble_position(
    position: WindowPosition,
    state: State<'_, Arc<Mutex<RuntimeState>>>,
) -> Result<(), String> {
    let mut runtime = state
        .lock()
        .map_err(|_| "runtime state unavailable".to_string())?;
    runtime.bubble_position = Some(position);
    Ok(())
}

#[tauri::command]
fn open_dashboard(app: AppHandle) -> Result<(), String> {
    let dashboard = app
        .get_webview_window("main")
        .ok_or_else(|| "dashboard window unavailable".to_string())?;
    dashboard.show().map_err(|error| error.to_string())?;
    dashboard.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn hide_dashboard(app: AppHandle) -> Result<(), String> {
    let dashboard = app
        .get_webview_window("main")
        .ok_or_else(|| "dashboard window unavailable".to_string())?;
    dashboard.hide().map_err(|error| error.to_string())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open Dashboard").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit TokenOwl").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("TokenOwl")
        .on_menu_event(
            |app: &AppHandle, event: MenuEvent| match event.id().as_ref() {
                "open" => {
                    let _ = open_dashboard(app.clone());
                }
                "quit" => app.exit(0),
                _ => {}
            },
        )
        .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = open_dashboard(tray.app_handle().clone());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = Arc::new(Mutex::new(RuntimeState::default()));

    tauri::Builder::default()
        .manage(runtime)
        .plugin(tauri_plugin_opener::init())
        .setup(|app| Ok(setup_tray(app)?))
        .invoke_handler(tauri::generate_handler![
            get_bubble_position,
            save_bubble_position,
            open_dashboard,
            hide_dashboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TokenOwl");
}
