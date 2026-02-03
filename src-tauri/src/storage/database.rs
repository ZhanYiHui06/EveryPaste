//! EveryPaste - Database operations module
//! 
//! Uses SQLite to store clipboard history records

use std::path::PathBuf;
use std::fs;

use rusqlite::{Connection, params, OptionalExtension};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use once_cell::sync::Lazy;

use crate::clipboard::{ClipboardItem, ContentType};

/// Global database connection
static DB: Lazy<Mutex<Option<Connection>>> = Lazy::new(|| Mutex::new(None));

/// Database error type
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database not initialized")]
    NotInitialized,
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Initialize database
/// 
/// Called at application startup, creates database file and table structure
pub fn init_database(data_dir: &PathBuf) -> Result<(), DatabaseError> {
    // Ensure data directory exists
    fs::create_dir_all(data_dir)?;
    
    let db_path = data_dir.join("data.db");
    log::info!("Initializing database at: {:?}", db_path);
    
    let conn = Connection::open(&db_path)?;
    
    // Create table structure
    conn.execute_batch(
        r#"
        -- Clipboard history table
        CREATE TABLE IF NOT EXISTS clipboard_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content_type TEXT NOT NULL,
            plain_text TEXT,
            rich_text TEXT,
            image_path TEXT,
            preview TEXT NOT NULL,
            hash TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            is_pinned INTEGER DEFAULT 0
        );

        -- User settings table
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        -- Indexes
        CREATE INDEX IF NOT EXISTS idx_created_at ON clipboard_history(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_content_type ON clipboard_history(content_type);
        CREATE INDEX IF NOT EXISTS idx_hash ON clipboard_history(hash);
        "#
    )?;

    // Database migration: add image_thumbnail column (if not exists)
    // Ignore error (if column already exists)
    let _ = conn.execute("ALTER TABLE clipboard_history ADD COLUMN image_thumbnail TEXT", []);
    
    // Store connection
    let mut db = DB.lock();
    *db = Some(conn);
    
    log::info!("Database initialized successfully");
    Ok(())
}

/// Helper macro to get database connection
macro_rules! with_db {
    ($db:ident => $body:expr) => {{
        let guard = DB.lock();
        let $db = guard.as_ref().ok_or(DatabaseError::NotInitialized)?;
        $body
    }};
}

/// Insert new clipboard record
pub fn insert_clipboard_item(item: &ClipboardItem) -> Result<i64, DatabaseError> {
    with_db!(conn => {
        conn.execute(
            r#"
            INSERT OR REPLACE INTO clipboard_history 
            (content_type, plain_text, rich_text, image_path, preview, hash, created_at, is_pinned, image_thumbnail)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                item.content_type.as_str(),
                item.plain_text,
                item.rich_text,
                item.image_path,
                item.preview,
                item.hash,
                item.created_at.to_rfc3339(),
                item.is_pinned as i32,
                item.image_thumbnail,
            ],
        )?;
        
        Ok(conn.last_insert_rowid())
    })
}

/// Get all clipboard history records
pub fn get_all_items(limit: Option<i32>) -> Result<Vec<ClipboardItem>, DatabaseError> {
    with_db!(conn => {
        let sql = match limit {
            Some(n) if n > 0 => format!(
                "SELECT id, content_type, plain_text, rich_text, image_path, preview, hash, created_at, is_pinned, image_thumbnail 
                 FROM clipboard_history 
                 ORDER BY is_pinned DESC, created_at DESC 
                 LIMIT {}", n
            ),
            _ => "SELECT id, content_type, plain_text, rich_text, image_path, preview, hash, created_at, is_pinned, image_thumbnail 
                  FROM clipboard_history 
                  ORDER BY is_pinned DESC, created_at DESC".to_string(),
        };
        
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt.query_map([], |row| {
            Ok(ClipboardItem {
                id: row.get(0)?,
                content_type: ContentType::from_str(row.get::<_, String>(1)?.as_str())
                    .unwrap_or(ContentType::Text),
                plain_text: row.get(2)?,
                rich_text: row.get(3)?,
                image_path: row.get(4)?,
                image_thumbnail: row.get(9)?,
                preview: row.get(5)?,
                hash: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                is_pinned: row.get::<_, i32>(8)? != 0,
            })
        })?.filter_map(|r| r.ok()).collect();
        
        Ok(items)
    })
}

/// Get single record by ID
pub fn get_item_by_id(id: i64) -> Result<Option<ClipboardItem>, DatabaseError> {
    with_db!(conn => {
        let mut stmt = conn.prepare(
            "SELECT id, content_type, plain_text, rich_text, image_path, preview, hash, created_at, is_pinned, image_thumbnail
             FROM clipboard_history WHERE id = ?1"
        )?;
        
        let item = stmt.query_row([id], |row| {
            Ok(ClipboardItem {
                id: row.get(0)?,
                content_type: ContentType::from_str(row.get::<_, String>(1)?.as_str())
                    .unwrap_or(ContentType::Text),
                plain_text: row.get(2)?,
                rich_text: row.get(3)?,
                image_path: row.get(4)?,
                image_thumbnail: row.get(9)?,
                preview: row.get(5)?,
                hash: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                is_pinned: row.get::<_, i32>(8)? != 0,
            })
        }).optional()?;
        
        Ok(item)
    })
}

/// Delete specified record
pub fn delete_item(id: i64) -> Result<bool, DatabaseError> {
    with_db!(conn => {
        let affected = conn.execute("DELETE FROM clipboard_history WHERE id = ?1", [id])?;
        Ok(affected > 0)
    })
}

/// Clear all records
pub fn clear_all_items() -> Result<(), DatabaseError> {
    with_db!(conn => {
        conn.execute("DELETE FROM clipboard_history", [])?;
        Ok(())
    })
}

/// Check if hash already exists
pub fn hash_exists(hash: &str) -> Result<bool, DatabaseError> {
    with_db!(conn => {
        let mut stmt = conn.prepare("SELECT 1 FROM clipboard_history WHERE hash = ?1 LIMIT 1")?;
        let exists = stmt.exists([hash])?;
        Ok(exists)
    })
}

/// Get total record count
pub fn get_item_count() -> Result<i64, DatabaseError> {
    with_db!(conn => {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history",
            [],
            |row| row.get(0)
        )?;
        Ok(count)
    })
}

/// Cleanup old records exceeding limit
/// 
/// Keep the latest max_count records, delete the rest
pub fn cleanup_old_items(max_count: i32) -> Result<i64, DatabaseError> {
    if max_count <= 0 {
        return Ok(0); // Unlimited mode
    }
    
    with_db!(conn => {
        // Delete old records exceeding limit (keep pinned ones)
        let deleted = conn.execute(
            r#"
            DELETE FROM clipboard_history 
            WHERE id NOT IN (
                SELECT id FROM clipboard_history 
                WHERE is_pinned = 1
                UNION ALL
                SELECT id FROM clipboard_history 
                WHERE is_pinned = 0 
                ORDER BY created_at DESC 
                LIMIT ?1
            )
            "#,
            [max_count],
        )?;
        
        Ok(deleted as i64)
    })
}

/// Search clipboard records
pub fn search_items(query: &str, limit: Option<i32>) -> Result<Vec<ClipboardItem>, DatabaseError> {
    with_db!(conn => {
        let search_pattern = format!("%{}%", query);
        let limit_clause = match limit {
            Some(n) if n > 0 => format!("LIMIT {}", n),
            _ => String::new(),
        };
        
        let sql = format!(
            r#"
            SELECT id, content_type, plain_text, rich_text, image_path, preview, hash, created_at, is_pinned, image_thumbnail 
            FROM clipboard_history 
            WHERE plain_text LIKE ?1 OR preview LIKE ?1
            ORDER BY is_pinned DESC, created_at DESC
            {}
            "#,
            limit_clause
        );
        
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt.query_map([&search_pattern], |row| {
            Ok(ClipboardItem {
                id: row.get(0)?,
                content_type: ContentType::from_str(row.get::<_, String>(1)?.as_str())
                    .unwrap_or(ContentType::Text),
                plain_text: row.get(2)?,
                rich_text: row.get(3)?,
                image_path: row.get(4)?,
                image_thumbnail: row.get(9)?,
                preview: row.get(5)?,
                hash: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                is_pinned: row.get::<_, i32>(8)? != 0,
            })
        })?.filter_map(|r| r.ok()).collect();
        
        Ok(items)
    })
}

// ============== Settings Operations ==============

/// Save setting item
pub fn save_setting(key: &str, value: &str) -> Result<(), DatabaseError> {
    with_db!(conn => {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    })
}

/// Get setting item
pub fn get_setting(key: &str) -> Result<Option<String>, DatabaseError> {
    with_db!(conn => {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let value = stmt.query_row([key], |row| row.get(0)).optional()?;
        Ok(value)
    })
}
