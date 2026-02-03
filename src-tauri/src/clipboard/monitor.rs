//! EveryPaste - Clipboard monitoring module
//! 
//! Responsible for monitoring system clipboard changes and capturing new content

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;

use arboard::Clipboard;
use parking_lot::Mutex;
use blake3::Hasher;

use super::models::ContentType;

/// Clipboard content snapshot
#[derive(Debug, Clone)]
pub struct ClipboardSnapshot {
    /// Content type
    pub content_type: ContentType,
    /// Plain text content
    pub plain_text: Option<String>,
    /// Rich text HTML
    pub rich_text: Option<String>,
    /// Image data (PNG format)
    pub image_data: Option<Vec<u8>>,
    /// Content hash
    pub hash: String,
}

/// Clipboard monitor
pub struct ClipboardMonitor {
    /// Whether running
    running: Arc<AtomicBool>,
    /// Polling interval (milliseconds)
    poll_interval_ms: u64,
    /// Hash of last content
    last_hash: Arc<Mutex<String>>,
    /// Whether paused (used when the app writes to clipboard)
    paused: Arc<AtomicBool>,
}

impl ClipboardMonitor {
    /// Create a new monitor
    pub fn new(poll_interval_ms: u64) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            poll_interval_ms,
            last_hash: Arc::new(Mutex::new(String::new())),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start monitoring
    /// 
    /// callback: Callback function called when new content is detected
    pub fn start<F>(&self, callback: F)
    where
        F: Fn(ClipboardSnapshot) + Send + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            log::warn!("Clipboard monitor is already running");
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let last_hash = Arc::clone(&self.last_hash);
        let paused = Arc::clone(&self.paused);
        let interval = self.poll_interval_ms;

        thread::spawn(move || {
            log::info!("Clipboard monitor started with {}ms interval", interval);

            while running.load(Ordering::SeqCst) {
                // If paused, skip this detection
                if paused.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(interval));
                    continue;
                }

                // Create new Clipboard instance each loop to ensure getting latest data
                let mut clipboard = match Clipboard::new() {
                    Ok(cb) => cb,
                    Err(e) => {
                        log::error!("Failed to create clipboard instance: {}", e);
                        thread::sleep(Duration::from_millis(interval));
                        continue;
                    }
                };

                // Try to read clipboard content
                if let Some(snapshot) = Self::read_clipboard(&mut clipboard) {
                    let mut last = last_hash.lock();
                    
                    if snapshot.hash != *last {
                        log::debug!("[Monitor] New content detected: {:?}", snapshot.content_type);
                        *last = snapshot.hash.clone();
                        drop(last);
                        callback(snapshot);
                    }
                }

                thread::sleep(Duration::from_millis(interval));
            }

            log::info!("Clipboard monitor stopped");
        });
    }

    /// Stop monitoring
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Pause monitoring
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume monitoring
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Read clipboard content
    fn read_clipboard(clipboard: &mut Clipboard) -> Option<ClipboardSnapshot> {
        // 1. First check direct image data in clipboard (arboard)
        match clipboard.get_image() {
            Ok(image) => {
                log::debug!("[Clipboard] Detected direct image: {}x{}", image.width, image.height);
                // Calculate hash based on raw RGBA data (ensure consistency)
                let hash = Self::compute_hash(&image.bytes);
                
                let image_data = Self::rgba_to_png(&image);
                if image_data.is_empty() {
                    log::error!("[Clipboard] Failed to convert image to PNG");
                } else {
                    log::debug!("[Clipboard] Converted to PNG: {} bytes", image_data.len());
                    return Some(ClipboardSnapshot {
                        content_type: ContentType::Image,
                        plain_text: None,
                        rich_text: None,
                        image_data: Some(image_data),
                        hash,
                    });
                }
            },
            Err(e) => {
                // arboard failed, try using clipboard-win to read DIB format
                log::debug!("[Clipboard] arboard failed: {}, trying DIB format...", e);
                
                if let Some((image_data, hash)) = Self::read_dib_image() {
                    return Some(ClipboardSnapshot {
                        content_type: ContentType::Image,
                        plain_text: None,
                        rich_text: None,
                        image_data: Some(image_data),
                        hash,
                    });
                }
            }
        }

        // 2. Check for image files (new)
        log::debug!("[Clipboard] Checking for image files...");
        if let Some(image_data) = Self::read_image_files() {
            log::debug!("[Clipboard] Got image from file: {} bytes", image_data.len());
            let hash = Self::compute_hash(&image_data);
            return Some(ClipboardSnapshot {
                content_type: ContentType::Image,
                plain_text: None,
                rich_text: None,
                image_data: Some(image_data),
                hash,
            });
        }

        // 3. Check text
        if let Ok(text) = clipboard.get_text() {
            if !text.is_empty() {
                let hash = Self::compute_hash(text.as_bytes());
                
                return Some(ClipboardSnapshot {
                    content_type: ContentType::Text,
                    plain_text: Some(text),
                    rich_text: None,
                    image_data: None,
                    hash,
                });
            }
        }

        None
    }

    /// Try to read images from clipboard file list
    fn read_image_files() -> Option<Vec<u8>> {
        use std::path::Path;

        // Try to get file list (fully qualified path)
        let files: Vec<String> = match clipboard_win::get_clipboard::<Vec<String>, _>(clipboard_win::formats::FileList) {
            Ok(f) => {
                log::info!("[Clipboard] FileList detected: {} files", f.len());
                f
            },
            Err(e) => {
                log::debug!("[Clipboard] No FileList in clipboard: {}", e);
                return None;
            }
        };

        if files.is_empty() {
            log::debug!("[Clipboard] FileList is empty");
            return None;
        }

        // Iterate to find the first valid image file
        for file_path in &files {
            log::debug!("[Clipboard] Checking file: {}", file_path);
            let path = Path::new(file_path);
            if !path.exists() {
                log::debug!("[Clipboard] File does not exist: {}", file_path);
                continue;
            }
            if !path.is_file() {
                log::debug!("[Clipboard] Not a file: {}", file_path);
                continue;
            }

            // Check extension
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            log::debug!("[Clipboard] File extension: {}", ext);
            match ext.as_str() {
                "png" | "jpg" | "jpeg" | "bmp" | "webp" | "ico" | "gif" => {
                    // Read and convert image to standard PNG data
                    match image::open(&path) {
                        Ok(img) => {
                            let mut png_data = Vec::new();
                            let mut cursor = std::io::Cursor::new(&mut png_data);
                            if img.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                log::info!("[Clipboard] Read image from file: {:?} ({} bytes)", path, png_data.len());
                                return Some(png_data);
                            } else {
                                log::error!("[Clipboard] Failed to write PNG for: {:?}", path);
                            }
                        },
                        Err(e) => {
                            log::error!("[Clipboard] Failed to open image file {:?}: {}", path, e);
                        }
                    }
                },
                _ => {
                    log::debug!("[Clipboard] Skipping non-image extension: {}", ext);
                    continue;
                }
            }
        }

        log::debug!("[Clipboard] No valid image files found in FileList");
        None
    }

    /// Try to read DIB format image from clipboard (for supporting third-party screenshot tools like PixPin)
    fn read_dib_image() -> Option<(Vec<u8>, String)> {
        use clipboard_win::{formats, get_clipboard};
        
        // Try to read CF_BITMAP format
        let bitmap_data: Vec<u8> = match get_clipboard::<Vec<u8>, _>(formats::Bitmap) {
            Ok(data) => {
                log::info!("[Clipboard] Got DIB/Bitmap data: {} bytes", data.len());
                data
            },
            Err(e) => {
                log::debug!("[Clipboard] No DIB/Bitmap data: {}", e);
                return None;
            }
        };

        if bitmap_data.is_empty() {
            return None;
        }

        // Calculate hash (based on raw data)
        let hash = Self::compute_hash(&bitmap_data);
        log::info!("[Clipboard] DIB hash: {}", &hash[..8]);

        // Try to decode BMP data and convert to PNG
        match image::load_from_memory(&bitmap_data) {
            Ok(img) => {
                let mut png_data = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut png_data);
                if img.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                    log::info!("[Clipboard] Converted DIB to PNG: {} bytes", png_data.len());
                    return Some((png_data, hash));
                } else {
                    log::error!("[Clipboard] Failed to convert DIB to PNG");
                }
            },
            Err(e) => {
                log::debug!("[Clipboard] Failed to decode DIB data: {}", e);
            }
        }

        None
    }

    /// Convert RGBA image data to PNG
    fn rgba_to_png(image: &arboard::ImageData) -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.to_vec(),
        ).unwrap_or_else(|| ImageBuffer::new(1, 1));

        let mut png_data = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_data);
        if let Err(e) = img.write_to(&mut cursor, image::ImageFormat::Png) {
            log::error!("Failed to write PNG data: {}", e);
        }
        
        png_data
    }

    /// Compute content hash
    fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Hasher::new();
        hasher.update(data);
        hasher.finalize().to_hex().to_string()
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new(150) // Default 150ms polling interval
    }
}
