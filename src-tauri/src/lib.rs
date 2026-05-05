mod settings;
mod strategies;
mod zapret;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};

use crate::settings::{autodetect_zapret_path, inspect_path, looks_like_zapret, PathInspection, Settings};
use crate::strategies::{list_strategies, Strategy};
use crate::zapret::{current_status, start as zapret_start, stop as zapret_stop, Status, ZapretState};

/// Shared application state behind a Mutex; all Tauri commands take it via `State`.
pub struct AppState {
    pub settings: Mutex<Settings>,
    pub zapret: ZapretState,
}

impl AppState {
    fn new() -> Self {
        Self {
            settings: Mutex::new(Settings::load()),
            zapret: ZapretState::default(),
        }
    }
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> Result<(), String> {
    settings.save()?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn detect_zapret_path() -> Option<String> {
    autodetect_zapret_path()
}

#[tauri::command]
fn validate_zapret_path(path: String) -> bool {
    !path.is_empty() && looks_like_zapret(&path)
}

#[tauri::command]
fn inspect_zapret_path(path: String) -> PathInspection {
    inspect_path(&path)
}

#[tauri::command]
fn list_strategies_cmd(zapret_path: String) -> Vec<Strategy> {
    if zapret_path.is_empty() {
        return Vec::new();
    }
    list_strategies(zapret_path)
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Status {
    current_status(&state.zapret)
}

#[tauri::command]
async fn start_zapret(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Status, String> {
    let s = state.settings.lock().unwrap().clone();
    let zapret_path = s
        .zapret_path
        .clone()
        .ok_or_else(|| "Не указана папка с zapret".to_string())?;
    let strategy = s
        .strategy
        .clone()
        .ok_or_else(|| "Не выбрана стратегия".to_string())?;
    let dir = PathBuf::from(&zapret_path);
    if !looks_like_zapret(&dir) {
        return Err(format!(
            "В папке {} не найдены winws.exe и general.bat",
            zapret_path
        ));
    }
    zapret_start(&state.zapret, &dir, &strategy, &s.game_filter)?;
    let status = current_status(&state.zapret);
    let _ = app.emit("zapret-status", &status);
    refresh_tray(&app, status.active);
    Ok(status)
}

#[tauri::command]
async fn stop_zapret(app: AppHandle, state: State<'_, AppState>) -> Result<Status, String> {
    zapret_stop(&state.zapret)?;
    let status = current_status(&state.zapret);
    let _ = app.emit("zapret-status", &status);
    refresh_tray(&app, status.active);
    Ok(status)
}

#[tauri::command]
fn is_admin() -> bool {
    is_elevated()
}

#[tauri::command]
fn relaunch_as_admin(app: AppHandle) -> Result<(), String> {
    relaunch_elevated()?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    // Make sure zapret is stopped so we don't leave winws.exe running silently.
    if let Some(state) = app.try_state::<AppState>() {
        let _ = zapret_stop(&state.zapret);
    }
    app.exit(0);
}

fn refresh_tray(app: &AppHandle, active: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        let tip = if active {
            "ZapretGUI — активно"
        } else {
            "ZapretGUI — отключено"
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Focus the existing window on second-instance launch.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            detect_zapret_path,
            validate_zapret_path,
            inspect_zapret_path,
            list_strategies_cmd,
            get_status,
            start_zapret,
            stop_zapret,
            is_admin,
            relaunch_as_admin,
            quit_app,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let minimize = app
                    .try_state::<AppState>()
                    .map(|s| s.settings.lock().unwrap().minimize_to_tray)
                    .unwrap_or(true);
                if minimize {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        });

    builder = builder.setup(|app| {
        // Build tray menu and tray icon at runtime so localized labels work.
        let show = MenuItem::with_id(app, "show", "Показать", true, None::<&str>)?;
        let start = MenuItem::with_id(app, "start", "Старт", true, None::<&str>)?;
        let stop = MenuItem::with_id(app, "stop", "Стоп", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &start, &stop, &quit])?;

        let _tray = TrayIconBuilder::with_id("main")
            .tooltip("ZapretGUI")
            .icon(app.default_window_icon().cloned().unwrap())
            .menu(&menu)
            .on_menu_event(|app, event| match event.id.as_ref() {
                "show" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.unminimize();
                        let _ = win.set_focus();
                    }
                }
                "start" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(state) = app.try_state::<AppState>() {
                            let s = state.settings.lock().unwrap().clone();
                            if let (Some(p), Some(strat)) = (s.zapret_path.clone(), s.strategy.clone()) {
                                let dir = PathBuf::from(&p);
                                let _ = zapret_start(&state.zapret, &dir, &strat, &s.game_filter);
                                let status = current_status(&state.zapret);
                                let _ = app.emit("zapret-status", &status);
                                refresh_tray(&app, status.active);
                            }
                        }
                    });
                }
                "stop" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(state) = app.try_state::<AppState>() {
                            let _ = zapret_stop(&state.zapret);
                            let status = current_status(&state.zapret);
                            let _ = app.emit("zapret-status", &status);
                            refresh_tray(&app, status.active);
                        }
                    });
                }
                "quit" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let _ = zapret_stop(&state.zapret);
                    }
                    app.exit(0);
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.unminimize();
                        let _ = win.set_focus();
                    }
                }
            })
            .build(app)?;

        // If autostart is enabled, kick off zapret in the background.
        let app_handle = app.handle().clone();
        let auto = app
            .state::<AppState>()
            .settings
            .lock()
            .unwrap()
            .autostart_zapret;
        if auto {
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let s = state.settings.lock().unwrap().clone();
                    if let (Some(p), Some(strat)) = (s.zapret_path.clone(), s.strategy.clone()) {
                        let dir = PathBuf::from(&p);
                        let _ = zapret_start(&state.zapret, &dir, &strat, &s.game_filter);
                        let status = current_status(&state.zapret);
                        let _ = app_handle.emit("zapret-status", &status);
                        refresh_tray(&app_handle, status.active);
                    }
                }
            });
        }
        Ok(())
    });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------------------
// Privilege detection / re-launch
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn is_elevated() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size: u32 = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
        let res = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size,
            &mut size,
        );
        let _ = windows::Win32::Foundation::CloseHandle(token);
        if res.is_err() {
            return false;
        }
        elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
fn is_elevated() -> bool {
    true
}

#[cfg(windows)]
fn relaunch_elevated() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_w: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let verb_w: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();

    let h = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if h.0 as isize <= 32 {
        return Err("Не удалось запросить повышение прав (UAC отклонён)".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn relaunch_elevated() -> Result<(), String> {
    Err("Только Windows".into())
}
