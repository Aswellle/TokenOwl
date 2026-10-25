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

/// Geometry the dashboard needs to start its expansion from the bubble.
///
/// The two windows are separate native surfaces, so the "morph" is a handoff:
/// Rust reports where the bubble sits (in dashboard-window coordinates, which
/// is what the webview can draw against) and the dashboard grows out of that
/// point while the bubble fades. Sending the rect rather than a bare point
/// keeps the origin correct on multi-monitor setups and after a drag.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MorphAnchor {
    /// Bubble center X in dashboard client coordinates (CSS pixels).
    x: f64,
    /// Bubble center Y in dashboard client coordinates (CSS pixels).
    y: f64,
    /// Bubble diameter in CSS pixels.
    diameter: f64,
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

/// Where the bubble currently sits, expressed in dashboard-window CSS pixels.
///
/// Both windows report *physical* positions, so the math is: bubble center in
/// physical space → subtract the dashboard's client origin (its outer position
/// plus decorations, which are none here) → divide by the scale factor of the
/// dashboard's monitor. Falls back to the bubble's saved position when the
/// window is not realized yet (e.g. the tray opened the dashboard first).
fn morph_anchor(app: &AppHandle) -> Option<MorphAnchor> {
    let bubble = app.get_webview_window("bubble")?;
    let dashboard = app.get_webview_window("main")?;
    let bubble_position = bubble.outer_position().ok()?;
    let bubble_size = bubble.outer_size().ok()?;
    let dashboard_position = dashboard.outer_position().ok()?;
    let scale = dashboard.scale_factor().ok()?;
    let scale = if scale > 0.0 { scale } else { 1.0 };

    let bubble_center_x = f64::from(bubble_position.x) + f64::from(bubble_size.width) / 2.0;
    let bubble_center_y = f64::from(bubble_position.y) + f64::from(bubble_size.height) / 2.0;
    let dashboard_origin_x = f64::from(dashboard_position.x);
    let dashboard_origin_y = f64::from(dashboard_position.y);

    Some(MorphAnchor {
        x: (bubble_center_x - dashboard_origin_x) / scale,
        y: (bubble_center_y - dashboard_origin_y) / scale,
        diameter: f64::from(bubble_size.width) / scale,
    })
}

/// Whether the bubble window is currently visible, so the dashboard knows if
/// it should play the expand-from-bubble morph or just open normally (tray,
/// second launch, deep link).
fn bubble_is_visible(app: &AppHandle) -> bool {
    app.get_webview_window("bubble")
        .and_then(|bubble| bubble.is_visible().ok())
        .unwrap_or(false)
}

#[tauri::command]
fn open_dashboard(app: AppHandle) -> Result<(), String> {
    // The dashboard replaces the bubble: expanded and collapsed are exclusive.
    // The bubble stays visible for one animation beat so the dashboard can
    // grow out of it; the frontend calls `finish_bubble_handoff` when its
    // expansion lands, which is what actually hides the bubble. Hiding here
    // would leave a frame with neither surface on screen — the gap this
    // transition exists to remove.
    let anchor = morph_anchor(&app);
    let from_bubble = bubble_is_visible(&app);
    let dashboard = app
        .get_webview_window("main")
        .ok_or_else(|| "dashboard window unavailable".to_string())?;
    dashboard.show().map_err(|error| error.to_string())?;
    dashboard.set_focus().map_err(|error| error.to_string())?;
    if from_bubble {
        if let Some(anchor) = anchor {
            let _ = app.emit("window:morph-from-bubble", anchor);
        }
    }
    Ok(())
}

/// Hides the bubble once the dashboard has visually taken over.
///
/// The dashboard calls this after its expansion animation settles. It is
/// idempotent and safe to call when no morph ran (the bubble is already
/// hidden), so the frontend never needs to track handoff state.
#[tauri::command]
// Tauri commands must return a `Result` to surface IPC errors; hiding is
// best-effort by design, so the signature stays for the command contract.
#[allow(clippy::unnecessary_wraps)]
fn finish_bubble_handoff(app: AppHandle) -> Result<(), String> {
    if let Some(bubble) = app.get_webview_window("bubble") {
        let _ = bubble.hide();
    }
    Ok(())
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
    // The bubble is placed before it is shown so it materializes at its
    // docked spot, not wherever it was left. It appears immediately while the
    // dashboard runs its collapse animation; the dashboard hides itself when
    // that animation lands (see `finish_dashboard_handoff`), so the two
    // surfaces overlap for one beat instead of leaving a blank frame between
    // them.
    bubble.show().map_err(|error| error.to_string())?;
    if let Some(anchor) = morph_anchor(&app) {
        let _ = app.emit("window:morph-to-bubble", anchor);
    }
    Ok(())
}

/// Hides the dashboard once its collapse animation has visually handed over
/// to the bubble. Called by the dashboard frontend; idempotent.
#[tauri::command]
// Tauri commands must return a `Result` to surface IPC errors; hiding is
// best-effort by design, so the signature stays for the command contract.
#[allow(clippy::unnecessary_wraps)]
fn finish_dashboard_handoff(app: AppHandle) -> Result<(), String> {
    if let Some(dashboard) = app.get_webview_window("main") {
        let _ = dashboard.hide();
    }
    Ok(())
}

/// Reads the bubble window's current position and stores it for the next
/// `show_bubble`, so the frontend never needs position permissions. The
/// position is snapped to the nearest screen edge within reach, so a drag
/// near any border makes the bubble hug that edge persistently.
#[tauri::command]
fn persist_bubble_position(
    app: AppHandle,
    state: State<'_, Arc<Mutex<RuntimeState>>>,
) -> Result<(), String> {
    let bubble = app
        .get_webview_window("bubble")
        .ok_or_else(|| "bubble window unavailable".to_string())?;
    let position = bubble.outer_position().map_err(|error| error.to_string())?;
    let (snapped_x, snapped_y) = snap_to_edge(&bubble, position.x, position.y);
    // Move the window onto the snap target so what the user sees matches
    // what will be restored on the next `show_bubble`.
    bubble
        .set_position(tauri::PhysicalPosition::new(snapped_x, snapped_y))
        .map_err(|error| error.to_string())?;
    let x = f64::from(snapped_x);
    let y = f64::from(snapped_y);
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

/// Distance beyond which a dragged position no longer snaps to an edge.
const EDGE_SNAP_THRESHOLD_PX: f64 = 48.0;

/// Snaps a physical position to the nearest monitor edge per axis when it is
/// within [`EDGE_SNAP_THRESHOLD_PX`]. Dragging near a border (or corner)
/// therefore docks the bubble flush against it, on any of the four edges.
fn snap_to_edge(bubble: &tauri::WebviewWindow, x: i32, y: i32) -> (i32, i32) {
    let Ok(Some(monitor)) = bubble.current_monitor() else {
        return (x, y);
    };
    let Ok(size) = bubble.outer_size() else {
        return (x, y);
    };
    let monitor_origin = monitor.position();
    let monitor_size = monitor.size();
    let width = i32::try_from(size.width).unwrap_or(0);
    let height = i32::try_from(size.height).unwrap_or(0);
    let monitor_width = i32::try_from(monitor_size.width).unwrap_or(i32::MAX);
    let monitor_height = i32::try_from(monitor_size.height).unwrap_or(i32::MAX);
    let left = monitor_origin.x;
    let top = monitor_origin.y;
    let right = monitor_origin.x + monitor_width - width;
    let bottom = monitor_origin.y + monitor_height - height;

    let mut snapped_x = x;
    let mut snapped_y = y;
    let distance_left = (x - left).abs();
    let distance_right = (right - x).abs();
    if distance_left <= distance_right {
        if f64::from(distance_left) <= EDGE_SNAP_THRESHOLD_PX {
            snapped_x = left;
        }
    } else if f64::from(distance_right) <= EDGE_SNAP_THRESHOLD_PX {
        snapped_x = right;
    }
    let distance_top = (y - top).abs();
    let distance_bottom = (bottom - y).abs();
    if distance_top <= distance_bottom {
        if f64::from(distance_top) <= EDGE_SNAP_THRESHOLD_PX {
            snapped_y = top;
        }
    } else if f64::from(distance_bottom) <= EDGE_SNAP_THRESHOLD_PX {
        snapped_y = bottom;
    }
    (snapped_x, snapped_y)
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

/// The two global hotkeys the app registers.
///
/// `Alt+Shift+T` toggles between the dashboard and the bubble — the single
/// action a user reaches for while working in another app. `Alt+Shift+R`
/// refreshes every account without opening the dashboard first. Both are
/// global (they work while `TokenOwl` is unfocused) and deliberately avoid
/// `Ctrl+Shift` combinations that Windows reserves for input-language
/// switching and that many editors already use.
const HOTKEY_TOGGLE: &str = "Alt+Shift+T";
const HOTKEY_REFRESH: &str = "Alt+Shift+R";

/// Registers the global hotkeys, tolerating a failed registration.
///
/// A hotkey another application already owns fails to register; that must not
/// abort startup — the app is fully usable from the tray and the bubble. The
/// failure is reported to stderr once so a user who presses the hotkey and
/// sees nothing can find out why.
fn setup_shortcuts(app: &AppHandle) {
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    let toggle = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyT);
    let refresh = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyR);
    let toggle_for_handler = toggle;
    let refresh_for_handler = refresh;

    let result = app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                if shortcut == &toggle_for_handler {
                    toggle_dashboard(app);
                } else if shortcut == &refresh_for_handler {
                    refresh_all_accounts(app);
                }
            })
            .build(),
    );

    if let Err(error) = result {
        eprintln!("global shortcuts unavailable: {error}");
        return;
    }

    if let Err(error) = app.global_shortcut().register(toggle) {
        eprintln!("{HOTKEY_TOGGLE} could not be registered: {error}");
    }
    if let Err(error) = app.global_shortcut().register(refresh) {
        eprintln!("{HOTKEY_REFRESH} could not be registered: {error}");
    }
}

/// Shows the dashboard when the bubble (or nothing) is in front, and collapses
/// back to the bubble when the dashboard already has focus.
fn toggle_dashboard(app: &AppHandle) {
    let dashboard_visible = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if dashboard_visible {
        let _ = show_bubble(app.clone(), app.state::<Arc<Mutex<RuntimeState>>>());
    } else {
        let _ = open_dashboard(app.clone());
    }
}

/// Refreshes every account from the background task, then emits the usual
/// `quota:updated` event so both windows redraw.
fn refresh_all_accounts(app: &AppHandle) {
    let Some(runtime) = app.try_state::<Arc<commands::AppRuntime>>() else {
        return;
    };
    let runtime = Arc::clone(runtime.inner());
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = commands::refresh_accounts(&runtime, Some(&handle), None).await {
            eprintln!("hotkey refresh failed: {error}");
        }
        let _ = handle.emit("quota:updated", ());
    });
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
    // The accelerators are display hints: the plugin already owns these keys
    // globally, so the menu shows them rather than binding a second handler.
    let open = MenuItemBuilder::with_id("open", "Open Dashboard")
        .accelerator(HOTKEY_TOGGLE)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit TokenOwl").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("TokenOwl")
        // Left click opens the dashboard through the handler below; popping
        // the menu over it (the builder's default) would fight that action.
        // Right click keeps the menu.
        .show_menu_on_left_click(false)
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
        });
    // Without an icon the tray entry renders as an empty slot on Windows and
    // is effectively invisible; the app icon is the tray icon.
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;

    Ok(())
}

/// Opt-in QA drills, selected by environment variables so ordinary runs are
/// untouched. `TOKENOWL_UPDATER_DRILL=1` exercises the real updater flow
/// (endpoint check → download → signature verify → install → restart) a few
/// seconds after startup; `TOKENOWL_TEST_BANNER=1` sends one synthetic
/// notification through the production dispatch chain to verify the OS
/// banner appears on screen.
fn start_drill_hooks(app: &AppHandle) {
    if std::env::var("TOKENOWL_UPDATER_DRILL").is_ok_and(|value| value == "1") {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            updater_drill(handle).await;
        });
    }
    if std::env::var("TOKENOWL_TEST_BANNER").is_ok_and(|value| value == "1") {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(6)).await;
            let state = handle.state::<Arc<commands::AppRuntime>>();
            commands::run_test_banner_drill(state.inner(), &handle);
        });
    }
}

#[cfg(desktop)]
async fn updater_drill(app: AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    tracing::info!(target: "drill", "updater drill: checking endpoint");
    let update = match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => update,
            Ok(None) => {
                tracing::info!(target: "drill", "updater drill: endpoint reported no update");
                return;
            }
            Err(error) => {
                tracing::error!(target: "drill", error = %error, "updater drill: check failed");
                return;
            }
        },
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "updater drill: updater unavailable");
            return;
        }
    };
    tracing::info!(target: "drill", version = %update.version, "updater drill: downloading and installing");
    if let Err(error) = update.download_and_install(|_, _| {}, || {}).await {
        tracing::error!(target: "drill", error = %error, "updater drill: install failed");
        return;
    }
    tracing::info!(target: "drill", "updater drill: signature verified and installed; restarting");
    app.restart();
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
                commands::refresh_accounts(&runtime, Some(&handle), None).await
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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let database_path = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?
                .join("tokenowl.sqlite3");
            let runtime =
                commands::AppRuntime::new(database_path).map_err(std::io::Error::other)?;
            // Retry credential deletions a previous run left queued before
            // anything else touches the credential store.
            runtime.run_startup_cleanup();
            app.manage(Arc::new(runtime));
            app.manage(Arc::new(Mutex::new(RuntimeState::default())));
            // tao registers only a 16px ICON_SMALL from the embedded resource
            // and never an ICON_BIG, so the taskbar (and Alt-Tab) upscale the
            // 16px bitmap and render the app icon blurry on any scaled display.
            // Handing every window a high-resolution icon makes each shell
            // surface downscale from real pixels instead.
            let window_icon = tauri::include_image!("icons/128x128@2x.png");
            for label in ["main", "bubble"] {
                if let Some(window) = app.get_webview_window(label) {
                    let _ = window.set_icon(window_icon.clone());
                }
            }
            setup_tray(app)?;
            setup_shortcuts(app.handle());
            start_background_tasks(app.handle());
            start_drill_hooks(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bubble_position,
            save_bubble_position,
            open_dashboard,
            finish_bubble_handoff,
            finish_dashboard_handoff,
            hide_dashboard,
            minimize_dashboard,
            show_bubble,
            persist_bubble_position,
            commands::accounts_list,
            commands::accounts_create,
            commands::account_target_is_lan,
            commands::accounts_delete,
            commands::accounts_refresh,
            commands::manifest_profiles,
            commands::lan_consent_begin,
            commands::notifications_list,
            commands::notifications_mark_read,
            commands::notification_preferences_get,
            commands::notification_preferences_set,
            commands::history_summary,
            commands::usage_windows,
            commands::history_breakdown,
            commands::provider_matrix,
            commands::open_project_page,
            #[cfg(desktop)]
            updater_check,
            #[cfg(desktop)]
            updater_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TokenOwl");
}
