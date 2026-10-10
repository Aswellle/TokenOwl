use crate::commands;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, State,
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_updater::UpdaterExt;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
    published_at: Option<String>,
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
    // The dashboard replaces the bubble: expanded and collapsed are exclusive.
    if let Some(bubble) = app.get_webview_window("bubble") {
        let _ = bubble.hide();
    }
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

#[tauri::command]
fn minimize_dashboard(app: AppHandle) -> Result<(), String> {
    let dashboard = app
        .get_webview_window("main")
        .ok_or_else(|| "dashboard window unavailable".to_string())?;
    dashboard.minimize().map_err(|error| error.to_string())
}

/// Collapses the dashboard into the floating bubble window at its saved
/// position (or the bottom-right corner of the current monitor on first use).
#[tauri::command]
fn show_bubble(app: AppHandle, state: State<'_, Arc<Mutex<RuntimeState>>>) -> Result<(), String> {
    let bubble = app
        .get_webview_window("bubble")
        .ok_or_else(|| "bubble window unavailable".to_string())?;

    let saved = state
        .lock()
        .ok()
        .and_then(|runtime| runtime.bubble_position.clone());

    let position = saved.or_else(|| {
        app.try_state::<Arc<commands::AppRuntime>>()
            .and_then(|runtime| {
                let runtime = Arc::clone(runtime.inner());
                runtime.load_bubble_position().ok().flatten()
            })
            .map(|(x, y)| WindowPosition { x, y })
    });

    match position {
        Some(pos) => {
            let _ = bubble.set_position(tauri::PhysicalPosition::new(
                coordinate_to_i32(pos.x),
                coordinate_to_i32(pos.y),
            ));
        }
        None => place_bubble_bottom_right(&bubble),
    }
    bubble.show().map_err(|error| error.to_string())?;
    if let Some(dashboard) = app.get_webview_window("main") {
        let _ = dashboard.hide();
    }
    Ok(())
}

/// Reads the bubble window's current position and stores it for the next
/// `show_bubble`, so the frontend never needs position permissions.
#[tauri::command]
fn persist_bubble_position(
    app: AppHandle,
    state: State<'_, Arc<Mutex<RuntimeState>>>,
) -> Result<(), String> {
    let bubble = app
        .get_webview_window("bubble")
        .ok_or_else(|| "bubble window unavailable".to_string())?;
    let position = bubble.outer_position().map_err(|error| error.to_string())?;
    let x = f64::from(position.x);
    let y = f64::from(position.y);
    let mut runtime = state
        .lock()
        .map_err(|_| "runtime state unavailable".to_string())?;
    runtime.bubble_position = Some(WindowPosition { x, y });

    if let Some(runtime) = app.try_state::<Arc<commands::AppRuntime>>() {
        let runtime = Arc::clone(runtime.inner());
        runtime
            .save_bubble_position(x, y)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn place_bubble_bottom_right(bubble: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = bubble.current_monitor() else {
        return;
    };
    let Ok(size) = bubble.outer_size() else {
        return;
    };
    let monitor_size = monitor.size();
    let monitor_position = monitor.position();
    let margin = coordinate_to_i32(24.0 * monitor.scale_factor());
    let width = i32::try_from(size.width).unwrap_or(320);
    let height = i32::try_from(size.height).unwrap_or(480);
    let monitor_width = i32::try_from(monitor_size.width).unwrap_or(i32::MAX);
    let monitor_height = i32::try_from(monitor_size.height).unwrap_or(i32::MAX);
    let x = monitor_position.x + monitor_width - width - margin;
    let y = monitor_position.y + monitor_height - height - margin;
    let _ = bubble.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Converts a saved physical coordinate to `i32`, saturating instead of
/// wrapping. Persisted values come from `outer_position()` and are whole
/// pixels, so the clamped cast never loses meaningful precision.
fn coordinate_to_i32(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    #[allow(clippy::cast_possible_truncation)] // clamped to the i32 range first
    let result = clamped.round() as i32;
    result
}

#[cfg(desktop)]
#[tauri::command]
async fn updater_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    Ok(update.map(|update| UpdateInfo {
        version: update.version,
        notes: update.body,
        published_at: update.date.map(|date| date.to_string()),
    }))
}

#[cfg(desktop)]
#[tauri::command]
async fn updater_install(app: AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let Some(update) = update else {
        return Err("no update is available".to_owned());
    };
    update
        .download_and_install(|_chunk_length, _content_length| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    app.restart();
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

fn start_background_tasks(app: &AppHandle) {
    let handle = app.clone();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let updater_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Store the shutdown sender so other tasks can signal shutdown.
    let _ = app.manage(shutdown_tx);

    tauri::async_runtime::spawn(async move {
        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            // Prevent concurrent updater checks.
            if !updater_running.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let running = updater_running.clone();
                let update_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    emit_update_if_available(&update_handle);
                    running.store(false, std::sync::atomic::Ordering::SeqCst);
                });
            }

            let result = if let Some(runtime) = handle.try_state::<Arc<commands::AppRuntime>>() {
                let runtime = Arc::clone(runtime.inner());
                commands::refresh_accounts(runtime.as_ref(), None).await
            } else {
                Ok(Vec::new())
            };
            if result.is_ok() {
                let _ = handle.emit("quota:updated", ());
            }
            let sleep_duration = handle.try_state::<Arc<commands::AppRuntime>>().map_or(
                std::time::Duration::from_secs(600),
                |runtime| {
                    let policy = &runtime.freshness_policy;
                    let stale_after = policy.stale_after.as_secs();
                    let fresh_for = policy.fresh_for.as_secs();
                    if stale_after > 0 {
                        std::time::Duration::from_secs(stale_after)
                    } else {
                        std::time::Duration::from_secs(fresh_for)
                    }
                },
            );
            tokio::time::timeout(sleep_duration, shutdown_rx.changed())
                .await
                .ok();
        }
    });
}

fn emit_update_if_available(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = handle.updater() else {
            return;
        };
        let Ok(Some(update)) = updater.check().await else {
            return;
        };
        let _ = handle.emit(
            "app:update-available",
            UpdateInfo {
                version: update.version,
                notes: update.body,
                published_at: update.date.map(|date| date.to_string()),
            },
        );
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the `TokenOwl` desktop application.
///
/// # Panics
///
/// Panics when the Tauri runtime fails to start, which is fatal for the process.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let database_path = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?
                .join("tokenowl.sqlite3");
            let runtime =
                commands::AppRuntime::new(database_path).map_err(std::io::Error::other)?;
            app.manage(Arc::new(runtime));
            app.manage(Arc::new(Mutex::new(RuntimeState::default())));
            setup_tray(app)?;
            start_background_tasks(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bubble_position,
            save_bubble_position,
            open_dashboard,
            hide_dashboard,
            minimize_dashboard,
            show_bubble,
            persist_bubble_position,
            commands::accounts_list,
            commands::accounts_create,
            commands::accounts_delete,
            commands::accounts_refresh,
            #[cfg(desktop)]
            updater_check,
            #[cfg(desktop)]
            updater_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TokenOwl");
}
