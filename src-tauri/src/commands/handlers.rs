//! EveryPaste - Tauri command handlers
//! 
//! Defines Rust commands callable from frontend

use tauri::{AppHandle, Manager, Emitter};
use arboard::Clipboard;
use serde::{Deserialize, Serialize};

use crate::clipboard::{ClipboardItem, ClipboardItemView, ContentType};
use crate::storage;
use crate::config::{self, Settings, Theme, StorageLimit};

/// Command execution result
#[derive(Debug, Serialize)]
pub struct CommandResult<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> CommandResult<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

/// Get clipboard history list
#[tauri::command]
pub fn get_clipboard_history(limit: Option<i32>) -> CommandResult<Vec<ClipboardItemView>> {
    match storage::get_all_items(limit) {
        Ok(items) => {
            let views: Vec<ClipboardItemView> = items.into_iter().map(|i| i.into()).collect();
            CommandResult::ok(views)
        }
        Err(e) => CommandResult::err(format!("Failed to get clipboard history: {}", e)),
    }
}

/// Get complete content of a single clipboard record
#[tauri::command]
pub fn get_clipboard_item(id: i64) -> CommandResult<ClipboardItem> {
    match storage::get_item_by_id(id) {
        Ok(Some(item)) => CommandResult::ok(item),
        Ok(None) => CommandResult::err(format!("Item not found: {}", id)),
        Err(e) => CommandResult::err(format!("Failed to get item: {}", e)),
    }
}

/// Paste specified record (copy to system clipboard)
#[tauri::command]
pub fn paste_item(app: AppHandle, id: i64, as_plain_text: bool) -> CommandResult<bool> {
    // Get record
    let item = match storage::get_item_by_id(id) {
        Ok(Some(item)) => item,
        Ok(None) => return CommandResult::err(format!("Item not found: {}", id)),
        Err(e) => return CommandResult::err(format!("Failed to get item: {}", e)),
    };

    // Create clipboard instance
    let mut clipboard = match Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => return CommandResult::err(format!("Failed to access clipboard: {}", e)),
    };

    // Paste according to content type
    match item.content_type {
        ContentType::Text => {
            if let Some(text) = &item.plain_text {
                if let Err(e) = clipboard.set_text(text) {
                    return CommandResult::err(format!("Failed to set clipboard text: {}", e));
                }
            }
        }
        ContentType::RichText => {
            // If plain text requested or no rich text, use plain text
            let text = if as_plain_text {
                item.plain_text.as_deref()
            } else {
                // Prefer plain text (rich text not directly supported yet)
                item.plain_text.as_deref()
            };
            
            if let Some(text) = text {
                if let Err(e) = clipboard.set_text(text) {
                    return CommandResult::err(format!("Failed to set clipboard text: {}", e));
                }
            }
        }
        ContentType::Image => {
            // Image needs to be loaded from file
            if let Some(image_path) = &item.image_path {
                // Parse full image path
                let full_path = match app.path().app_data_dir() {
                    Ok(path_buf) => path_buf.join(image_path),
                    Err(e) => return CommandResult::err(format!("Failed to get app data dir: {}", e)),
                };
                
                log::info!("Pasting image from: {:?}", full_path);
                
                // Read image from file
                let img = match image::open(&full_path) {
                    Ok(i) => i.into_rgba8(),
                    Err(e) => return CommandResult::err(format!("Failed to read image file: {}", e)),
                };
                
                let (width, height) = img.dimensions();
                let image_data = arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: std::borrow::Cow::Owned(img.into_raw()),
                };
                
                if let Err(e) = clipboard.set_image(image_data) {
                     return CommandResult::err(format!("Failed to set clipboard image: {}", e));
                }
            } else {
                 return CommandResult::err("Image path is missing".to_string());
            }
        }
    }

    CommandResult::ok(true)
}

/// Delete specified record
#[tauri::command]
pub fn delete_item(id: i64) -> CommandResult<bool> {
    match storage::delete_item(id) {
        Ok(deleted) => CommandResult::ok(deleted),
        Err(e) => CommandResult::err(format!("Failed to delete item: {}", e)),
    }
}

/// Clear all history records
#[tauri::command]
pub fn clear_all_history() -> CommandResult<bool> {
    match storage::clear_all_items() {
        Ok(()) => CommandResult::ok(true),
        Err(e) => CommandResult::err(format!("Failed to clear history: {}", e)),
    }
}

/// Search clipboard records
#[tauri::command]
pub fn search_clipboard(query: String, limit: Option<i32>) -> CommandResult<Vec<ClipboardItemView>> {
    if query.is_empty() {
        return get_clipboard_history(limit);
    }
    
    match storage::search_items(&query, limit) {
        Ok(items) => {
            let views: Vec<ClipboardItemView> = items.into_iter().map(|i| i.into()).collect();
            CommandResult::ok(views)
        }
        Err(e) => CommandResult::err(format!("Search failed: {}", e)),
    }
}

/// Get current settings
#[tauri::command]
pub fn get_settings(app: AppHandle) -> CommandResult<Settings> {
    let mut settings = config::get_settings();
    
    // Get actual auto-start status from autostart plugin
    use tauri_plugin_autostart::ManagerExt;
    if let Ok(is_enabled) = app.autolaunch().is_enabled() {
        settings.auto_start = is_enabled;
    }
    
    CommandResult::ok(settings)
}

/// Settings update request from frontend
#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub theme: Option<String>,
    pub storage_limit: Option<i32>,
    pub auto_start: Option<bool>,
    pub shortcut: Option<String>,
}

/// Update settings
#[tauri::command]
pub fn update_settings(updates: SettingsUpdate, _app: AppHandle) -> CommandResult<Settings> {
    let mut settings = config::get_settings();
    
    // Update theme
    if let Some(theme) = updates.theme {
        settings.theme = match theme.as_str() {
            "dark" => Theme::Dark,
            _ => Theme::Light,
        };
    }
    
    // Update storage limit
    if let Some(limit) = updates.storage_limit {
        settings.storage_limit = StorageLimit::from_i32(limit);
        
        // Cleanup old records exceeding limit
        if limit > 0 {
            if let Err(e) = storage::cleanup_old_items(limit) {
                log::warn!("Failed to cleanup old items: {}", e);
            }
        }
    }
    
    // Update auto-start
    if let Some(auto_start) = updates.auto_start {
        settings.auto_start = auto_start;
        
        // Use tauri-plugin-autostart plugin
        use tauri_plugin_autostart::ManagerExt;
        
        let autostart_manager = _app.autolaunch();
        let result = if auto_start {
            autostart_manager.enable()
        } else {
            autostart_manager.disable()
        };
        
        match result {
            Ok(_) => {
                log::info!("Autostart {} successfully", if auto_start { "enabled" } else { "disabled" });
            },
            Err(e) => {
                log::error!("Failed to update autostart: {}", e);
                return CommandResult::err(format!("Failed to update autostart: {}", e));
            }
        }
    }
    
    // Update shortcut
    if let Some(ref shortcut) = updates.shortcut {
        settings.shortcut = shortcut.clone();
        
        // Dynamically register new shortcut
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
        
        // Parse shortcut string, convert "Win" to "super" as required by Tauri
        let shortcut_str = shortcut.to_lowercase().replace("win", "super");
        if let Ok(new_shortcut) = shortcut_str.parse::<Shortcut>() {
            // Unregister all possible old shortcuts first
            let _ = _app.global_shortcut().unregister_all();
            
            // Register new shortcut
            let result = _app.global_shortcut().on_shortcut(new_shortcut, move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    log::debug!("Custom shortcut pressed - toggling window");
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = app.emit("window-shown", ());
                            let _ = app.emit("focus-first-item", ());
                        }
                    }
                }
            });
            
            match result {
                Ok(()) => log::info!("Custom shortcut '{}' registered successfully", shortcut),
                Err(e) => {
                    log::error!("Failed to register custom shortcut '{}': {}", shortcut, e);
                    return CommandResult::err(format!("Failed to register shortcut: {}", e));
                }
            }
        } else {
            log::error!("Invalid shortcut format: {}", shortcut);
            return CommandResult::err(format!("Invalid shortcut format: {}", shortcut));
        }
    }
    
    // Save settings
    match config::update_settings(settings.clone()) {
        Ok(()) => CommandResult::ok(settings),
        Err(e) => CommandResult::err(format!("Failed to save settings: {}", e)),
    }
}


/// Show main window
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> CommandResult<bool> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        CommandResult::ok(true)
    } else {
        CommandResult::err("Main window not found".to_string())
    }
}

/// Hide main window
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> CommandResult<bool> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        CommandResult::ok(true)
    } else {
        CommandResult::err("Main window not found".to_string())
    }
}

/// Get total record count
#[tauri::command]
pub fn get_history_count() -> CommandResult<i64> {
    match storage::get_item_count() {
        Ok(count) => CommandResult::ok(count),
        Err(e) => CommandResult::err(format!("Failed to get count: {}", e)),
    }
}

/// Check if this is the first run
#[tauri::command]
pub fn is_first_run() -> CommandResult<bool> {
    CommandResult::ok(config::is_first_run())
}

/// Mark first run as completed
#[tauri::command]
pub fn complete_first_run() -> CommandResult<bool> {
    match config::mark_first_run_completed() {
        Ok(()) => CommandResult::ok(true),
        Err(e) => CommandResult::err(format!("Failed to mark first run completed: {}", e)),
    }
}
