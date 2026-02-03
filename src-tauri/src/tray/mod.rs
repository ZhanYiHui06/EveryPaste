//! EveryPaste - System tray module
//! 
//! Manages system tray icon and menu

use tauri::{
    AppHandle, Manager, Emitter,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
};

/// Create system tray
pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Create tray menu
    let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "设置").build(app)?;
    let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    
    let menu = MenuBuilder::new(app)
        .item(&show_item)
        .item(&settings_item)
        .item(&separator)
        .item(&quit_item)
        .build()?;

    // Create tray icon
    // Use app-level icon (from tauri.conf.json bundle.icon configuration)
    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("EveryPaste - 剪贴板管理器")
        .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
            // If no default icon, create a simple placeholder icon
            tauri::image::Image::new_owned(
                vec![0u8; 16 * 16 * 4], // 16x16 transparent icon
                16,
                16,
            )
        }))
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "show" => {
                    show_window(app);
                }
                "settings" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        // Emit window shown event for animation
                        let _ = app.emit("window-shown", ());
                        let _ = app.emit("open-settings", ());
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;

    log::info!("Tray icon created");
    Ok(())
}

/// Show window and emit animation event
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        // Emit window shown event for animation
        let _ = app.emit("window-shown", ());
        let _ = app.emit("focus-first-item", ());
    }
}
