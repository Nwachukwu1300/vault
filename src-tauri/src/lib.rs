mod database;
mod embedding;
mod indexer;
mod transcription;

use database::{get_database_path, VectorDatabase};
use indexer::{Indexer, IndexingProgress};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;
use walkdir::WalkDir;

/// Bump whenever the on-disk index layout changes; older indexes are wiped and rebuilt.
const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub folders: Vec<String>,
    pub onboarding_complete: bool,
    /// Missing in configs written before versioning existed, which reads as 0.
    #[serde(default)]
    pub schema_version: u32,
}

/// Results from scanning folders for supported files
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanResult {
    pub documents: u32,
    pub images: u32,
    pub audio: u32,
    pub videos: u32,
    pub total: u32,
    pub estimated_time: String,
    pub estimated_cost: String,
}

/// Application state for managing indexer and database
pub struct AppState {
    database: Arc<RwLock<Option<Arc<VectorDatabase>>>>,
    indexer: Arc<RwLock<Option<Indexer>>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            onboarding_complete: false,
            schema_version: SCHEMA_VERSION,
        }
    }
}

fn get_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.vault.app");

    fs::create_dir_all(&config_dir).ok();
    config_dir.join("config.json")
}

fn get_keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.vault.app", "gemini-api-key")
        .map_err(|e| format!("Failed to access keyring: {}", e))
}

#[tauri::command]
async fn validate_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let client = reqwest::Client::new();

    // Make a lightweight API call to validate the key
    // Using the models list endpoint as a simple validation check
    let response = client
        .get("https://generativelanguage.googleapis.com/v1beta/models")
        .query(&[("key", &api_key)])
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().is_success() {
        Ok(true)
    } else if response.status() == 400 || response.status() == 401 || response.status() == 403 {
        Err("Invalid API key. Please check your key and try again.".to_string())
    } else {
        let status = response.status();
        Err(format!("API error ({}): Please try again.", status))
    }
}

#[tauri::command]
async fn save_api_key(api_key: String) -> Result<(), String> {
    let entry = get_keyring_entry()?;
    entry
        .set_password(&api_key)
        .map_err(|e| format!("Failed to save API key securely: {}", e))
}

#[tauri::command]
async fn get_api_key() -> Result<Option<String>, String> {
    let entry = get_keyring_entry()?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to retrieve API key: {}", e)),
    }
}

/// Delete the API key from secure storage
#[tauri::command]
async fn delete_api_key() -> Result<(), String> {
    let entry = get_keyring_entry()?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
        Err(e) => Err(format!("Failed to delete API key: {}", e)),
    }
}

/// Reset the app - clear config, API key, and database
#[tauri::command]
async fn reset_app(state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Delete API key
    let _ = delete_api_key().await;

    // Delete config file
    let config_path = get_config_path();
    if config_path.exists() {
        fs::remove_file(&config_path)
            .map_err(|e| format!("Failed to delete config: {}", e))?;
    }

    // Clear database reference
    *state.database.write().await = None;

    // Delete database directory
    let db_path = get_database_path();
    if db_path.exists() {
        fs::remove_dir_all(&db_path)
            .map_err(|e| format!("Failed to delete database: {}", e))?;
    }

    Ok(())
}

/// Clear just the index database (keep API key and config)
#[tauri::command]
async fn clear_index(state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Clear database reference
    *state.database.write().await = None;

    // Delete database directory
    let db_path = get_database_path();
    if db_path.exists() {
        fs::remove_dir_all(&db_path)
            .map_err(|e| format!("Failed to delete database: {}", e))?;
    }

    // Re-initialize the database
    let db = VectorDatabase::open(db_path).await?;
    *state.database.write().await = Some(Arc::new(db));

    Ok(())
}

#[tauri::command]
async fn save_config(folders: Vec<String>) -> Result<(), String> {
    write_config(&AppConfig {
        folders,
        onboarding_complete: true,
        schema_version: SCHEMA_VERSION,
    })
}

fn write_config(config: &AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(get_config_path(), json)
        .map_err(|e| format!("Failed to write config file: {}", e))
}

#[tauri::command]
async fn load_config() -> Result<Option<AppConfig>, String> {
    let config_path = get_config_path();

    if !config_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;

    let config: AppConfig = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config file: {}", e))?;

    Ok(Some(config))
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    app.dialog()
        .file()
        .pick_folder(move |folder_path| {
            let _ = tx.send(folder_path);
        });

    match rx.recv() {
        Ok(Some(path)) => Ok(Some(path.to_string())),
        Ok(None) => Ok(None),
        Err(_) => Err("Failed to receive folder selection".to_string()),
    }
}

/// Directories to skip during scanning
const SKIP_DIRECTORIES: &[&str] = &[
    ".venv", "venv", "node_modules", ".git", ".svn", "__pycache__",
    ".cache", ".npm", ".cargo", "target", "build", "dist", ".next",
    ".nuxt", "vendor", "Pods", ".gradle", ".idea", ".vscode", "Library", ".Trash",
];

/// Check if a directory should be skipped during scanning
fn should_skip_dir(path: &std::path::Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        // Skip hidden directories (starting with .)
        if name.starts_with('.') && name != "." && name != ".." {
            return true;
        }
        // Skip known directories that shouldn't be indexed
        if SKIP_DIRECTORIES.contains(&name) {
            return true;
        }
    }
    false
}

/// Categorize a file based on its extension
fn categorize_file(path: &std::path::Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_lowercase();

    match extension.as_str() {
        // Images
        "jpg" | "jpeg" | "png" | "webp" | "heic" => Some("image"),
        // Videos
        "mp4" | "mov" => Some("video"),
        // Audio
        "mp3" | "wav" | "m4a" | "aac" => Some("audio"),
        // Documents
        "pdf" | "txt" | "md" | "docx" => Some("document"),
        // Skip everything else
        _ => None,
    }
}

/// Format seconds into human-readable time string
fn format_time(total_seconds: f64) -> String {
    let hours = (total_seconds / 3600.0).floor() as u32;
    let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u32;

    if hours > 0 {
        if minutes > 0 {
            format!("{}h {}min", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        format!("{}min", minutes)
    } else {
        "< 1min".to_string()
    }
}

/// Scan folders and return file counts, estimated time, and estimated cost
#[tauri::command]
async fn scan_folders(folders: Vec<String>) -> Result<ScanResult, String> {
    let mut documents: u32 = 0;
    let mut images: u32 = 0;
    let mut audio: u32 = 0;
    let mut videos: u32 = 0;

    for folder in folders {
        let folder_path = PathBuf::from(&folder);

        if !folder_path.exists() {
            continue;
        }

        for entry in WalkDir::new(&folder_path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !should_skip_dir(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Categorize the file
            if let Some(category) = categorize_file(path) {
                match category {
                    "document" => documents += 1,
                    "image" => images += 1,
                    "audio" => audio += 1,
                    "video" => videos += 1,
                    _ => {}
                }
            }
        }
    }

    let total = documents + images + audio + videos;

    // Calculate estimated time based on conservative rates:
    // - Documents: 0.3s per file
    // - Images: 0.5s per file
    // - Audio: 2s per file (includes Whisper transcription)
    // - Videos: 2s per file (includes audio extraction + Whisper transcription)
    let estimated_seconds =
        (documents as f64 * 0.3) +
        (images as f64 * 0.5) +
        (audio as f64 * 2.0) +
        (videos as f64 * 2.0);

    let estimated_time = format_time(estimated_seconds);

    // Calculate estimated API cost based on Gemini Embedding 2 pricing
    // -----------------------------------------------------------------
    // PRICING ASSUMPTIONS (update these when pricing changes):
    // - Text embeddings (documents, audio transcripts, video transcripts): ~$0.00001 per 1K characters
    // - Image embeddings: ~$0.0001 per image
    //
    // Conservative estimates per file:
    // - Documents: $0.0001 (avg ~10K chars)
    // - Images: $0.0001 per image
    // - Audio: $0.0002 (transcription tends to be longer)
    // - Videos: $0.0002 (transcription tends to be longer)
    // -----------------------------------------------------------------
    const COST_PER_DOCUMENT: f64 = 0.0001;
    const COST_PER_IMAGE: f64 = 0.0001;
    const COST_PER_AUDIO: f64 = 0.0002;
    const COST_PER_VIDEO: f64 = 0.0002;

    let estimated_cost_value =
        (documents as f64 * COST_PER_DOCUMENT) +
        (images as f64 * COST_PER_IMAGE) +
        (audio as f64 * COST_PER_AUDIO) +
        (videos as f64 * COST_PER_VIDEO);

    // Format cost as dollars with ~ prefix to indicate estimate
    let estimated_cost = if estimated_cost_value < 0.01 {
        "< $0.01".to_string()
    } else {
        format!("~${:.2}", estimated_cost_value)
    };

    Ok(ScanResult {
        documents,
        images,
        audio,
        videos,
        total,
        estimated_time,
        estimated_cost,
    })
}

/// Initialize the database
#[tauri::command]
async fn init_database(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db_path = get_database_path();

    // An index built with an older schema can't be read; wipe it so the
    // frontend sees 0 indexed files and offers a rebuild via the estimate screen.
    if let Some(mut config) = load_config().await? {
        if config.schema_version < SCHEMA_VERSION {
            if db_path.exists() {
                fs::remove_dir_all(&db_path)
                    .map_err(|e| format!("Failed to clear old index: {}", e))?;
            }
            config.schema_version = SCHEMA_VERSION;
            write_config(&config)?;
        }
    }

    let db = VectorDatabase::open(db_path).await?;
    *state.database.write().await = Some(Arc::new(db));
    Ok(())
}

/// Start indexing files
#[tauri::command]
async fn start_indexing(
    folders: Vec<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Get the API key
    let api_key = get_api_key().await?.ok_or("No API key found")?;

    // Ensure database is initialized
    let db = {
        let db_guard = state.database.read().await;
        db_guard.clone().ok_or("Database not initialized")?
    };

    // Create and configure the indexer
    let mut indexer = Indexer::new(db, api_key);
    indexer.scan_and_queue(&folders).await?;

    // Start indexing and get the progress receiver
    let mut progress_rx = indexer.start();

    // Store the indexer
    *state.indexer.write().await = Some(indexer);

    // Spawn a task to emit progress events to the frontend
    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            // Emit progress to frontend
            let _ = app_handle.emit("indexing-progress", &progress);

            // If indexing is complete, send notification
            if !progress.is_indexing && progress.done_files() > 0 {
                let _ = app_handle.emit("indexing-complete", ());
            }
        }
    });

    Ok(())
}

/// Pause indexing
#[tauri::command]
async fn pause_indexing(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let indexer_guard = state.indexer.read().await;
    if let Some(indexer) = indexer_guard.as_ref() {
        indexer.pause().await
    } else {
        Err("No indexing in progress".to_string())
    }
}

/// Resume indexing
#[tauri::command]
async fn resume_indexing(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let indexer_guard = state.indexer.read().await;
    if let Some(indexer) = indexer_guard.as_ref() {
        indexer.resume().await
    } else {
        Err("No indexing in progress".to_string())
    }
}

/// Stop indexing
#[tauri::command]
async fn stop_indexing(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let indexer_guard = state.indexer.read().await;
    if let Some(indexer) = indexer_guard.as_ref() {
        indexer.stop().await
    } else {
        Err("No indexing in progress".to_string())
    }
}

/// Get current indexing progress
#[tauri::command]
async fn get_indexing_progress(state: tauri::State<'_, AppState>) -> Result<IndexingProgress, String> {
    let indexer_guard = state.indexer.read().await;
    if let Some(indexer) = indexer_guard.as_ref() {
        Ok(indexer.get_progress().await)
    } else {
        Ok(IndexingProgress::default())
    }
}

/// Get count of indexed files
#[tauri::command]
async fn get_indexed_count(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let db_guard = state.database.read().await;
    if let Some(db) = db_guard.as_ref() {
        db.count().await
    } else {
        Ok(0)
    }
}

/// Check if a file is indexed
#[tauri::command]
async fn is_file_indexed(
    file_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let db_guard = state.database.read().await;
    if let Some(db) = db_guard.as_ref() {
        db.exists(&file_path).await
    } else {
        Ok(false)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState {
            database: Arc::new(RwLock::new(None)),
            indexer: Arc::new(RwLock::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            validate_api_key,
            save_api_key,
            get_api_key,
            delete_api_key,
            reset_app,
            clear_index,
            save_config,
            load_config,
            pick_folder,
            scan_folders,
            init_database,
            start_indexing,
            pause_indexing,
            resume_indexing,
            stop_indexing,
            get_indexing_progress,
            get_indexed_count,
            is_file_indexed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_without_schema_version_needs_rebuild() {
        let old: AppConfig =
            serde_json::from_str(r#"{"folders":["/tmp"],"onboarding_complete":true}"#).unwrap();
        assert_eq!(old.schema_version, 0);
        assert!(old.schema_version < SCHEMA_VERSION);
    }
}
