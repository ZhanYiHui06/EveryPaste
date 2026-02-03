//! EveryPaste - Clipboard content data models
//! 
//! Defines data structures for clipboard history records

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Clipboard content type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// Plain text
    Text,
    /// Rich text (HTML format)
    RichText,
    /// Image
    Image,
}

impl ContentType {
    /// Convert from string to ContentType
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ContentType::Text),
            "rich_text" => Some(ContentType::RichText),
            "image" => Some(ContentType::Image),
            _ => None,
        }
    }

    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::RichText => "rich_text",
            ContentType::Image => "image",
        }
    }
}

/// Clipboard history record item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    /// Unique identifier
    pub id: i64,
    /// Content type
    pub content_type: ContentType,
    /// Plain text content (both text and rich text have this field)
    pub plain_text: Option<String>,
    /// Rich text HTML content
    pub rich_text: Option<String>,
    /// Image path (relative to data directory)
    pub image_path: Option<String>,
    /// Image Base64 thumbnail (for frontend preview)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_thumbnail: Option<String>,
    /// Preview text (for list display)
    pub preview: String,
    /// Content hash (for deduplication)
    pub hash: String,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Whether pinned
    pub is_pinned: bool,
}

impl ClipboardItem {
    /// Create new text type record
    pub fn new_text(id: i64, text: String, hash: String) -> Self {
        let preview = Self::generate_preview(&text, 100);
        Self {
            id,
            content_type: ContentType::Text,
            plain_text: Some(text),
            rich_text: None,
            image_path: None,
            image_thumbnail: None,
            preview,
            hash,
            created_at: Utc::now(),
            is_pinned: false,
        }
    }

    /// Create new rich text type record
    pub fn new_rich_text(id: i64, plain: String, html: String, hash: String) -> Self {
        let preview = Self::generate_preview(&plain, 100);
        Self {
            id,
            content_type: ContentType::RichText,
            plain_text: Some(plain),
            rich_text: Some(html),
            image_path: None,
            image_thumbnail: None,
            preview,
            hash,
            created_at: Utc::now(),
            is_pinned: false,
        }
    }

    /// Create new image type record
    pub fn new_image(id: i64, image_path: String, thumbnail: Option<String>, hash: String) -> Self {
        Self {
            id,
            content_type: ContentType::Image,
            plain_text: None,
            rich_text: None,
            image_path: Some(image_path),
            image_thumbnail: thumbnail,
            preview: "[Image]".to_string(),
            hash,
            created_at: Utc::now(),
            is_pinned: false,
        }
    }

    /// Generate preview text
    fn generate_preview(text: &str, max_len: usize) -> String {
        let text = text.trim();
        if text.chars().count() <= max_len {
            text.to_string()
        } else {
            let truncated: String = text.chars().take(max_len).collect();
            format!("{}...", truncated)
        }
    }
}

/// Simplified record for frontend display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItemView {
    pub id: i64,
    pub content_type: ContentType,
    pub preview: String,
    pub image_thumbnail: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_pinned: bool,
}

impl From<ClipboardItem> for ClipboardItemView {
    fn from(item: ClipboardItem) -> Self {
        Self {
            id: item.id,
            content_type: item.content_type,
            preview: item.preview,
            image_thumbnail: item.image_thumbnail,
            created_at: item.created_at,
            is_pinned: item.is_pinned,
        }
    }
}
