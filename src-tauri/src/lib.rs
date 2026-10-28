mod bridge;
mod connections;
mod monitor;
pub use connections::run_hook_cli_if_requested;

use bridge::{BridgeInfo, ExternalTask};
use serde::Serialize;
use std::{collections::HashMap, sync::{atomic::{AtomicBool, Ordering}, Mutex}};
use tauri::{menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem}, tray::TrayIconBuilder, Emitter, Manager};

pub(crate) struct NativeState {
    monitor: Mutex<monitor::Monitor>,
    scan_cache: Mutex<HashMap<std::path::PathBuf,connections::CachedRollout>>,
    sync_info: Mutex<connections::SyncInfo>,
    attention_evidence: Mutex<HashMap<String,String>>,
    connections: Mutex<connections::ConnectionSettings>,
    connection_errors: Mutex<HashMap<String,String>>,
    bridge: Mutex<BridgeInfo>,
    tasks: Mutex<HashMap<String, ExternalTask>>,
    pinned: AtomicBool,
    pin_menu: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
}

impl Default for NativeState {
    fn default() -> Self {
        Self {
            monitor: Mutex::new(monitor::Monitor::default()),
            scan_cache: Mutex::new(HashMap::new()),
            sync_info: Mutex::new(connections::SyncInfo::default()),
            attention_evidence: Mutex::new(HashMap::new()),
            connections: Mutex::new(connections::ConnectionSettings::default()),
            connection_errors: Mutex::new(HashMap::new()),
            bridge: Mutex::new(BridgeInfo::unavailable("Bridge is starting")),
            tasks: Mutex::new(HashMap::new()),
            pinned: AtomicBool::new(true),
            pin_menu: Mutex::new(None),
        }
    }
}

fn window(app: &tauri::AppHandle, label: &str) -> Result<tauri::WebviewWindow, String> {
    app.get_webview_window(label).ok_or_else(|| format!("Window {label} is unavailable"))
}

#[tauri::command]
fn show_panel(app: tauri::AppHandle) -> Result<(), String> {
    let panel = window(&app, "panel")?;
    panel.show().map_err(|e| e.to_string())?;
    panel.unminimize().map_err(|e| e.to_string())?;
    panel.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_panel(app: tauri::AppHandle) -> Result<(), String> {
    window(&app, "panel")?.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_widget(app: tauri::AppHandle) -> Result<(), String> {
    let widget = window(&app, "widget")?;
    widget.show().map_err(|e| e.to_string())?;
    let pinned = app.state::<NativeState>().pinned.load(Ordering::Relaxed);
    widget.set_always_on_top(pinned).map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_widget(app: tauri::AppHandle) -> Result<(), String> {
    window(&app, "widget")?.hide().map_err(|e| e.to_string())
}

fn set_pin(app: &tauri::AppHandle, pinned: bool) -> Result<(), String> {
    window(app, "widget")?.set_always_on_top(pinned).map_err(|e| e.to_string())?;
    let state = app.state::<NativeState>();
    state.pinned.store(pinned, Ordering::Relaxed);
    if let Some(item) = state.pin_menu.lock().map_err(|e| e.to_string())?.as_ref() {
        item.set_checked(pinned).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn set_widget_preferences(app: tauri::AppHandle, always_on_top: bool, scale: f64) -> Result<(), String> {
    if !scale.is_finite() || !(0.7..=1.8).contains(&scale) {
        return Err("Scale must be between 0.7 and 1.8".into());
    }
    set_pin(&app, always_on_top)?;
    window(&app, "widget")?.set_size(tauri::LogicalSize::new(150.0 * scale, 334.0 * scale)).map_err(|e| e.to_string())
}

#[tauri::command]
fn bridge_info(state: tauri::State<'_, NativeState>) -> Result<BridgeInfo, String> {
    state.bridge.lock().map(|info| info.clone()).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_external_tasks(state: tauri::State<'_, NativeState>) -> Result<Vec<ExternalTask>, String> {
    let mut tasks: Vec<_> = state.tasks.lock().map_err(|e| e.to_string())?.values().cloned().collect();
    tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(tasks)
}

#[tauri::command]
fn remove_external_task(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if id.is_empty() || id.len() > 160 { return Err("Invalid task id".into()); }
    app.state::<NativeState>().tasks.lock().map_err(|e| e.to_string())?.remove(&id);
    app.emit("external-task-removed", id).map_err(|e| e.to_string())
}

#[tauri::command]
fn play_signal_sound(kind: &str) -> Result<(), String> {
    if !matches!(kind, "attention" | "completed") { return Err("Unknown sound".into()); }
    #[cfg(target_os = "macos")]
    {
        let path = if kind == "attention" { "/System/Library/Sounds/Ping.aiff" } else { "/System/Library/Sounds/Glass.aiff" };
        let mut child = std::process::Command::new("/usr/bin/afplay").arg(path).spawn().map_err(|e| e.to_string())?;
        std::thread::spawn(move || { let _ = child.wait(); });
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" { fn MessageBeep(kind: u32) -> i32; }
        let result = unsafe { MessageBeep(if kind == "attention" { 0x30 } else { 0x40 }) };
        if result == 0 { return Err("Windows could not play the signal sound".into()); }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { return Err("Native signal sounds are available on macOS and Windows".into()); }
    #[allow(unreachable_code)]
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PinPreferences { always_on_top: bool }

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show-widget", "显示悬浮灯", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide-widget", "隐藏悬浮灯", true, None::<&str>)?;
    let panel = MenuItem::with_id(app, "open-panel", "打开控制台", true, None::<&str>)?;
    let pin = CheckMenuItem::with_id(app, "pin-widget", "悬浮灯置顶", true, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 AI Signal", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &panel, &pin, &separator, &quit])?;
    *app.state::<NativeState>().pin_menu.lock().expect("pin menu lock") = Some(pin);
    TrayIconBuilder::with_id("ai-signal")
        .icon(tray_icon())
        .tooltip("AI Signal · AI 工作状态")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "show-widget" => show_widget(app.clone()),
                "hide-widget" => hide_widget(app.clone()),
                "open-panel" => show_panel(app.clone()),
                "pin-widget" => {
                    let pinned = !app.state::<NativeState>().pinned.load(Ordering::Relaxed);
                    set_pin(app, pinned).and_then(|()| app.emit("preferences-changed", PinPreferences { always_on_top: pinned }).map_err(|e| e.to_string()))
                }
                "quit" => { app.exit(0); Ok(()) }
                _ => Ok(()),
            };
            if let Err(error) = result { eprintln!("AI Signal tray: {error}"); }
        })
        .build(app)?;
    Ok(())
}

fn tray_icon() -> tauri::image::Image<'static> {
    let mut rgba = vec![0u8; 24 * 24 * 4];
    for y in 1..23 { for x in 6..18 {
        let pixel = (y * 24 + x) * 4;
        let mut color = [38, 45, 44, 255];
        for (cy, lamp) in [(6i32, [242, 100, 91, 255]), (12, [240, 190, 65, 255]), (18, [78, 178, 130, 255])] {
            if (x as i32 - 12).pow(2) + (y as i32 - cy).pow(2) <= 6 { color = lamp; }
        }
        rgba[pixel..pixel + 4].copy_from_slice(&color);
    }}
    tauri::image::Image::new_owned(rgba, 24, 24)
}

pub fn run() {
    tauri::Builder::default()
        .manage(NativeState::default())
        .invoke_handler(tauri::generate_handler![show_panel, hide_panel, show_widget, hide_widget, set_widget_preferences, bridge_info, list_external_tasks, remove_external_task, play_signal_sound, connections::list_connections, connections::task_sync_info, connections::refresh_connections, connections::resolve_codex_question, connections::set_connection, monitor::list_monitor_tasks, monitor::acknowledge_monitor])
        .setup(|app| {
            setup_tray(app)?;
            bridge::start(app.handle().clone());
            connections::initialize(app.handle());
            monitor::initialize(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(error) = window.hide() { eprintln!("AI Signal hide: {error}"); }
            }
        })
        .run(tauri::generate_context!())
        .expect("AI Signal could not start");
}
