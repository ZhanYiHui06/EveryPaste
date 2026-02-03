//! EveryPaste - User settings module
//! 
//! Manages application user configuration

use serde::{Deserialize, Serialize};
use parking_lot::RwLock;
use once_cell::sync::Lazy;

use crate::storage;

/// Global settings instance
static SETTINGS: Lazy<RwLock<Settings>> = Lazy::new(|| RwLock::new(Settings::default()));

/// Theme type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Light
    }
}

/// Storage limit options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StorageLimit {
    /// 100 records
    Limit100 = 100,
    /// 200 records
    Limit200 = 200,
    /// 500 records
    Limit500 = 500,
    /// Unlimited
    Unlimited = -1,
}

// Custom serialization: serialize as numeric value
impl Serialize for StorageLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(self.as_i32())
    }
}

// Custom deserialization: deserialize from numeric value
impl<'de> Deserialize<'de> for StorageLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = i32::deserialize(deserializer)?;
        Ok(StorageLimit::from_i32(value))
    }
}

impl Default for StorageLimit {
    fn default() -> Self {
        StorageLimit::Limit100
    }
}

impl StorageLimit {
    /// Get numeric value (-1 means unlimited)
    pub fn as_i32(&self) -> i32 {
        match self {
            StorageLimit::Limit100 => 100,
            StorageLimit::Limit200 => 200,
            StorageLimit::Limit500 => 500,
            StorageLimit::Unlimited => -1,
        }
    }

    /// Create from numeric value
    pub fn from_i32(value: i32) -> Self {
        match value {
            100 => StorageLimit::Limit100,
            200 => StorageLimit::Limit200,
            500 => StorageLimit::Limit500,
            _ => StorageLimit::Unlimited,
        }
    }
}

/// User settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Theme
    pub theme: Theme,
    /// Storage limit
    pub storage_limit: StorageLimit,
    /// Auto-start on boot
    pub auto_start: bool,
    /// Preview text length
    pub preview_length: usize,

    /// Global shortcut
    pub shortcut: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            storage_limit: StorageLimit::Limit100,
            auto_start: false,
            preview_length: 100,
            shortcut: "Alt+V".to_string(),
        }
    }
}

impl Settings {
    /// Load settings (from database)
    pub fn load() -> Self {
        let mut settings = Settings::default();
        
        // Load theme
        if let Ok(Some(theme_str)) = storage::get_setting("theme") {
            settings.theme = match theme_str.as_str() {
                "dark" => Theme::Dark,
                _ => Theme::Light,
            };
        }
        
        // Load storage limit
        if let Ok(Some(limit_str)) = storage::get_setting("storage_limit") {
            if let Ok(limit) = limit_str.parse::<i32>() {
                settings.storage_limit = StorageLimit::from_i32(limit);
            }
        }
        
        // Load auto-start setting
        if let Ok(Some(auto_start_str)) = storage::get_setting("auto_start") {
            settings.auto_start = auto_start_str == "true";
        }
        

        // Load shortcut
        if let Ok(Some(shortcut)) = storage::get_setting("shortcut") {
            settings.shortcut = shortcut;
        }
        
        settings
    }

    /// Save settings (to database)
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let theme_str = match self.theme {
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        storage::save_setting("theme", theme_str)?;
        storage::save_setting("storage_limit", &self.storage_limit.as_i32().to_string())?;
        storage::save_setting("auto_start", &self.auto_start.to_string())?;

        storage::save_setting("shortcut", &self.shortcut)?;
        
        Ok(())
    }
}

/// Get current settings
pub fn get_settings() -> Settings {
    SETTINGS.read().clone()
}

/// Update settings
pub fn update_settings(settings: Settings) -> Result<(), Box<dyn std::error::Error>> {
    settings.save()?;
    *SETTINGS.write() = settings;
    Ok(())
}

/// Initialize settings (load from database)
pub fn init_settings() {
    let settings = Settings::load();
    *SETTINGS.write() = settings;
    log::info!("Settings initialized");
}

/// Check if this is the first run
pub fn is_first_run() -> bool {
    match storage::get_setting("first_run_completed") {
        Ok(Some(val)) => val != "true",
        _ => true, // No record means first run
    }
}

/// Mark first run as completed
pub fn mark_first_run_completed() -> Result<(), Box<dyn std::error::Error>> {
    storage::save_setting("first_run_completed", "true")?;
    Ok(())
}
