#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg(target_os = "windows")]

mod ai;
mod cache;
mod commands;
mod db;
mod indexer;
mod settings;
mod search;
mod state;

use std::{
    collections::HashMap,
    env, fs, io,
    sync::{
        atomic::AtomicBool,
        Mutex,
    },
    time::Duration,
};

use cache::FileCache;
use state::{gemini_api_key, AppState};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn position_spotlight<R: Runtime>(window: &WebviewWindow<R>) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let width = (660.0 * scale).round() as u32;
    let height = (82.0 * scale).round() as u32;
    let x = monitor_position.x + ((monitor_size.width.saturating_sub(width)) / 2) as i32;
    let y = monitor_position.y + (100.0 * scale).round() as i32;
    let _ = window.set_size(PhysicalSize::new(width, height));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_spotlight(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("spotlight")
        .ok_or_else(|| "The Eidos command bar is unavailable.".to_string())?;
    position_spotlight(&window);
    window.emit("spotlight-reset", ()).map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn toggle_spotlight(app: &AppHandle) {
    let Some(window) = app.get_webview_window("spotlight") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = show_spotlight(app);
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<tauri::tray::TrayIcon> {
    let spotlight = MenuItem::with_id(app, "spotlight", "Show Eidos", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&spotlight, &quit])?;
    let mut builder = TrayIconBuilder::with_id("eidos-tray")
        .tooltip("Eidos")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "spotlight" => {
                let _ = show_spotlight(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_spotlight(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)
}

fn io_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, message.into())
}

fn app_data_dir() -> io::Result<std::path::PathBuf> {
    let appdata = env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| io_error("APPDATA is not set"))?;
    Ok(appdata.join("Eidos"))
}

fn main() {
    let _ = dotenvy::dotenv();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_spotlight(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::search_files,
            commands::search_with_filters,
            commands::parse_natural_language,
            commands::execute_windows_search,
            commands::search_content_phrase,
            commands::index_drives,
            commands::open_file,
            commands::open_file_or_folder,
            commands::open_split_view,
            commands::create_file,
            commands::create_file_void,
            commands::get_pending_split_view,
            commands::ai_available,
            commands::read_file_content,
            commands::file_data_url,
            commands::list_folder,
            commands::get_file_metadata,
            commands::write_file,
            commands::ai_chat_message,
            commands::abort_ai,
            commands::return_to_spotlight,
            commands::hide_spotlight,
            commands::rebuild_search_index,
            commands::cache_size,
            commands::refresh_file_in_index,
            commands::get_settings,
            commands::set_settings,
            commands::available_drives,
            commands::show_settings
        ])
        .setup(|app| {
            let data_dir = app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let available_drives = indexer::fixed_drive_roots();
            let settings_path = data_dir.join("settings.json");
            let settings = settings::load_settings(&settings_path, &available_drives).map_err(io_error)?;
            let connection = db::initialize_database(&data_dir.join("eidos.db")).map_err(io_error)?;
            let mut records = db::all_records(&connection).map_err(io_error)?;
            records.retain(|record| {
                settings
                    .selected_drives
                    .iter()
                    .any(|drive| record.path.to_ascii_lowercase().starts_with(&drive.to_ascii_lowercase()))
            });
            let mut cache = FileCache::default();
            cache.replace(&records);
            let needs_initial_index = cache.is_empty();

            let gemini_api_key = if settings.gemini_api_key.trim().is_empty() {
                gemini_api_key()
            } else {
                Some(settings.gemini_api_key.trim().to_string())
            };
            if gemini_api_key.is_none() {
                log::info!("GEMINI_API_KEY is not configured; AI pane and Void file generation will be disabled.");
            }
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .user_agent("Eidos/1.0 Windows")
                .build()
                .map_err(|error| io_error(error.to_string()))?;
            app.manage(AppState {
                db: tokio::sync::Mutex::new(connection),
                cache: tokio::sync::RwLock::new(cache),
                settings: tokio::sync::RwLock::new(settings),
                settings_path,
                indexing: AtomicBool::new(false),
                gemini_api_key: tokio::sync::RwLock::new(gemini_api_key),
                abort_handles: tokio::sync::Mutex::new(HashMap::new()),
                natural_query_cache: tokio::sync::Mutex::new(HashMap::new()),
                http,
                pending_split_view: tokio::sync::Mutex::new(None),
                tray: Mutex::new(None),
            });

            let tray = build_tray(app)?;
            *app
                .state::<AppState>()
                .tray
                .lock()
                .map_err(|error| io_error(error.to_string()))? = Some(tray);

            let shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::Space);
            app.global_shortcut().register(shortcut)?;

            if let Some(spotlight) = app.get_webview_window("spotlight") {
                position_spotlight(&spotlight);
            }
            if needs_initial_index {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    if !state.try_begin_indexing() {
                        return;
                    }
                    let Some(window) = app_handle.get_webview_window("spotlight") else {
                        state.finish_indexing();
                        return;
                    };
                    if let Err(error) = indexer::index_priority_roots(
                        app_handle.clone(),
                        window.clone(),
                        indexer::priority_roots(),
                    ).await {
                        log::warn!("Priority-folder indexing failed: {error}");
                    }
                    let full_roots = state.settings.read().await.selected_drives.iter()
                        .map(std::path::PathBuf::from)
                        .filter(|path| path.is_dir())
                        .collect::<Vec<_>>();
                    if let Err(error) = indexer::index_roots(app_handle.clone(), window, full_roots).await {
                        log::warn!("Background full-drive indexing failed: {error}");
                    }
                    state.finish_indexing();
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(false) if window.label() == "spotlight" => {
                let _ = window.hide();
            }
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "settings" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("Eidos failed to start");
}
