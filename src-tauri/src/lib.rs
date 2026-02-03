//! EveryPaste - A lightweight clipboard manager
//!
//! A lightweight clipboard management tool designed for Windows,
//! supporting rich text, images, and persistent history storage.

pub mod clipboard;
pub mod commands;
pub mod config;
pub mod storage;
pub mod tray;

use std::sync::Arc;
use std::path::PathBuf;

use tauri::{AppHandle, Manager, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use parking_lot::Mutex;

use clipboard::{ClipboardMonitor, ClipboardSnapshot, ClipboardItem, ContentType};
use storage::init_database;
use config::init_settings;

/// Global clipboard monitor instance
static CLIPBOARD_MONITOR: once_cell::sync::Lazy<Arc<Mutex<Option<ClipboardMonitor>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Store the previous active window handle (used to restore focus)
#[cfg(target_os = "windows")]
static PREVIOUS_WINDOW: once_cell::sync::Lazy<Arc<Mutex<isize>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(0)));

/// Get the application data directory
fn get_data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Handle new clipboard content
fn handle_new_clipboard_content(app: &AppHandle, snapshot: ClipboardSnapshot) {
    log::info!("[Handler] Processing new clipboard content: {:?}, hash: {}", snapshot.content_type, &snapshot.hash[..8]);
    
    // Check if content already exists
    match storage::hash_exists(&snapshot.hash) {
        Ok(true) => {
            log::info!("[Handler] Content already exists (hash: {}), skipping", &snapshot.hash[..8]);
            return;
        },
        Ok(false) => {
            log::info!("[Handler] New content, proceeding to save (hash: {})", &snapshot.hash[..8]);
        },
        Err(e) => {
            log::error!("[Handler] Failed to check hash existence: {}", e);
            return;
        }
    }
    
    // Create clipboard record
    let item: ClipboardItem = match snapshot.content_type {
        ContentType::Text => {
            if let Some(text) = snapshot.plain_text {
                ClipboardItem::new_text(0, text, snapshot.hash)
            } else {
                return;
            }
        }
        ContentType::RichText => {
            if let (Some(plain), Some(html)) = (snapshot.plain_text, snapshot.rich_text) {
                ClipboardItem::new_rich_text(0, plain, html, snapshot.hash)
            } else {
                return;
            }
        }
        ContentType::Image => {
            if let Some(image_data) = snapshot.image_data {
                // Save image to file
                let data_dir = get_data_dir(app);
                let images_dir = data_dir.join("images");
                std::fs::create_dir_all(&images_dir).ok();
                
                let filename = format!("{}.png", uuid::Uuid::new_v4());
                let image_path = images_dir.join(&filename);
                
                if std::fs::write(&image_path, &image_data).is_err() {
                    log::error!("Failed to save image");
                    return;
                }
                
                // Generate thumbnail (Base64)
                let thumbnail = generate_thumbnail(&image_data);
                
                ClipboardItem::new_image(
                    0,
                    format!("images/{}", filename),
                    thumbnail,
                    snapshot.hash,
                )
            } else {
                return;
            }
        }
    };
    
    // Save to database
    match storage::insert_clipboard_item(&item) {
        Ok(id) => {
            log::info!("Saved clipboard item with id: {}", id);
            
            // Check and cleanup records exceeding limit
            let settings = config::get_settings();
            let limit = settings.storage_limit.as_i32();
            if limit > 0 {
                if let Err(e) = storage::cleanup_old_items(limit) {
                    log::warn!("Failed to cleanup old items: {}", e);
                }
            }
            
            // Notify frontend to refresh
            if let Err(e) = app.emit("clipboard-updated", ()) {
                log::warn!("Failed to emit clipboard-updated event: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to save clipboard item: {}", e);
        }
    }
}

/// Generate image thumbnail
fn generate_thumbnail(image_data: &[u8]) -> Option<String> {
    use image::ImageReader;
    use std::io::Cursor;
    use base64::Engine;
    
    let img = match ImageReader::new(Cursor::new(image_data))
        .with_guessed_format() {
            Ok(reader) => match reader.decode() {
                Ok(img) => img,
                Err(e) => {
                    log::error!("Failed to decode image for thumbnail: {}", e);
                    return None;
                }
            },
            Err(e) => {
                log::error!("Failed to guess image format: {}", e);
                return None;
            }
        };
    
    // Generate 64x64 thumbnail
    let thumbnail = img.thumbnail(64, 64);
    
    let mut png_data = Vec::new();
    if let Err(e) = thumbnail.write_to(&mut Cursor::new(&mut png_data), image::ImageFormat::Png) {
        log::error!("Failed to write thumbnail PNG: {}", e);
        return None;
    }
    
    let base64_str = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Some(format!("data:image/png;base64,{}", base64_str))
}

/// Toggle window visibility
fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            // Save current active window handle before showing window
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
                let hwnd = unsafe { GetForegroundWindow() };
                let mut prev = PREVIOUS_WINDOW.lock();
                *prev = hwnd.0 as isize;
                log::debug!("Saved previous window handle: {}", *prev);
            }
            
            let _ = window.show();
            let _ = window.set_focus();
            // Emit event to notify frontend
            let _ = app.emit("window-shown", ());
            let _ = app.emit("focus-first-item", ());
        }
    }
}

/// Register global shortcut
fn setup_global_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Load user-saved shortcut from settings
    let settings = config::get_settings();
    // Convert "Win" to "super" as required by Tauri
    let user_shortcut_str = settings.shortcut.to_lowercase().replace("win", "super");
    
    // Default shortcuts
    let default_shortcut: Shortcut = "super+v".parse()?;
    let fallback_shortcut: Shortcut = "ctrl+shift+v".parse()?;
    
    // Unregister any existing shortcuts first
    let _ = app.global_shortcut().unregister_all();
    
    // If user has set a custom shortcut (not the default Super+V)
    if user_shortcut_str != "super+v" {
        if let Ok(custom_shortcut) = user_shortcut_str.parse::<Shortcut>() {
            let result = app.global_shortcut().on_shortcut(custom_shortcut, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    log::debug!("Custom shortcut pressed - toggling window");
                    toggle_window(app);
                }
            });
            
            if result.is_ok() {
                log::info!("Custom shortcut '{}' registered successfully", settings.shortcut);
                return Ok(());
            }
            log::warn!("Failed to register custom shortcut '{}', falling back to defaults", settings.shortcut);
        }
    }
    
    // Try to register default shortcut Win+V
    let primary_result = app.global_shortcut().on_shortcut(default_shortcut, |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            log::debug!("Win+V pressed - toggling window");
            toggle_window(app);
        }
    });
    
    if primary_result.is_ok() {
        log::info!("Global shortcut Win+V registered successfully");
        return Ok(());
    }
    
    // Win+V registration failed (likely occupied by Windows), try fallback shortcut
    log::warn!("Win+V registration failed (likely occupied by Windows), trying Ctrl+Shift+V...");
    
    app.global_shortcut().on_shortcut(fallback_shortcut, |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            log::debug!("Ctrl+Shift+V pressed - toggling window");
            toggle_window(app);
        }
    })?;
    
    log::info!("Global shortcut Ctrl+Shift+V registered as fallback");
    Ok(())
}

/// Start clipboard monitoring
fn start_clipboard_monitor(app: AppHandle) {
    let monitor = ClipboardMonitor::new(150);
    
    let app_clone = app.clone();
    monitor.start(move |snapshot| {
        handle_new_clipboard_content(&app_clone, snapshot);
    });
    
    *CLIPBOARD_MONITOR.lock() = Some(monitor);
    log::info!("Clipboard monitor started");
}

/// Restore focus to previous window and simulate paste
#[tauri::command]
#[cfg(target_os = "windows")]
async fn restore_and_paste(app: AppHandle) -> Result<(), String> {
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    // 1. Hide EveryPaste window
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    // 2. Wait for window to hide
    thread::sleep(Duration::from_millis(50));

    // 3. Restore focus to previous window
    let prev_hwnd = *PREVIOUS_WINDOW.lock();
    if prev_hwnd != 0 {
        log::debug!("Restoring focus to window: {}", prev_hwnd);
        unsafe {
            let hwnd = HWND(prev_hwnd as *mut _);
            let _ = SetForegroundWindow(hwnd);
        }
    }

    // 4. Wait for focus to restore
    thread::sleep(Duration::from_millis(100));

    // 5. Simulate Ctrl+V paste
    unsafe {
        let mut inputs: [INPUT; 4] = std::mem::zeroed();

        // Ctrl press
        inputs[0].r#type = INPUT_KEYBOARD;
        inputs[0].Anonymous.ki = KEYBDINPUT {
            wVk: VK_CONTROL,
            wScan: 0,
            dwFlags: Default::default(),
            time: 0,
            dwExtraInfo: 0,
        };

        // V press
        inputs[1].r#type = INPUT_KEYBOARD;
        inputs[1].Anonymous.ki = KEYBDINPUT {
            wVk: VK_V,
            wScan: 0,
            dwFlags: Default::default(),
            time: 0,
            dwExtraInfo: 0,
        };

        // V release
        inputs[2].r#type = INPUT_KEYBOARD;
        inputs[2].Anonymous.ki = KEYBDINPUT {
            wVk: VK_V,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        };

        // Ctrl release
        inputs[3].r#type = INPUT_KEYBOARD;
        inputs[3].Anonymous.ki = KEYBDINPUT {
            wVk: VK_CONTROL,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        };

        let result = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if result == 0 {
            log::error!("SendInput failed");
            return Err("Failed to simulate paste".to_string());
        }
        log::debug!("Simulated Ctrl+V paste");
    }

    Ok(())
}

/// Modify Windows clipboard policy (Win+V support)
#[tauri::command]
async fn set_win_v_policy(enable: bool) -> Result<(), String> {
    let script = if enable {
        // Restore (delete policy items)
        "Remove-ItemProperty -Path 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\System' -Name 'AllowClipboardHistory' -ErrorAction SilentlyContinue; Remove-ItemProperty -Path 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\System' -Name 'AllowCrossDeviceClipboard' -ErrorAction SilentlyContinue;"
    } else {
        // Disable (set policy items to 0, must specify -Type DWord for correct type)
        "New-Item -Path 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\System' -Force -ErrorAction SilentlyContinue | Out-Null; Set-ItemProperty -Path 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\System' -Name 'AllowClipboardHistory' -Value 0 -Type DWord; Set-ItemProperty -Path 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\System' -Name 'AllowCrossDeviceClipboard' -Value 0 -Type DWord;"
    };
    
    let safe_script = script.replace("\"", "`\"");
    let final_script = format!(
        "Start-Process powershell -Verb RunAs -ArgumentList \"-NoProfile -Command \\\"{}\\\"\"", 
        safe_script
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &final_script])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err("Failed to launch admin process".to_string());
    }

    Ok(())
}

/// Application main entry point
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    
    log::info!("EveryPaste starting...");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When attempting to start a second instance, show the existing window
            log::info!("Second instance detected, focusing existing window");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = app.emit("window-shown", ());
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_clipboard_history,
            commands::get_clipboard_item,
            commands::paste_item,
            commands::delete_item,
            commands::clear_all_history,
            commands::search_clipboard,
            commands::get_settings,
            commands::update_settings,
            commands::show_main_window,
            commands::hide_main_window,
            commands::get_history_count,
            commands::is_first_run,
            commands::complete_first_run,
            set_win_v_policy,
            restore_and_paste, // Restore focus and simulate paste
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            
            // Initialize data directory
            let data_dir = get_data_dir(&app_handle);
            log::info!("Data directory: {:?}", data_dir);
            
            // Initialize database
            if let Err(e) = init_database(&data_dir) {
                log::error!("Failed to initialize database: {}", e);
                return Err(e.into());
            }
            
            // Initialize settings
            init_settings();
            
            // Create system tray
            if let Err(e) = tray::create_tray(&app_handle) {
                log::error!("Failed to create tray: {}", e);
            }
            
            // Register global shortcut
            if let Err(e) = setup_global_shortcut(&app_handle) {
                log::error!("Failed to register global shortcut: {}", e);
            }
            
            // Start clipboard monitoring
            start_clipboard_monitor(app_handle.clone());
            
            // Show main window on first run (display welcome page)
            if config::is_first_run() {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    log::info!("First run detected, showing welcome window");
                }
            }
            
            log::info!("EveryPaste initialized successfully");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
