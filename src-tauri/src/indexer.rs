//! Indexing queue and pipeline
//!
//! This module manages the file indexing queue with priority-based processing.
//! Files are processed in order: Documents first, then Images, then Audio/Video.

use crate::database::{FileRecord, VectorDatabase};
use crate::embedding::{EmbeddingClient, TaskType};
use crate::transcription::{transcribe_audio, transcribe_video};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use walkdir::WalkDir;

// ============================================================================
// RATE LIMITING CONFIGURATION
// ============================================================================
// Adjust these values based on your Gemini API quota.
// Default: 60 requests per minute (1 request per second)
// ============================================================================
const REQUESTS_PER_MINUTE: u32 = 60;
const MIN_REQUEST_INTERVAL_MS: u64 = (60_000 / REQUESTS_PER_MINUTE as u64) + 50; // Add 50ms buffer

/// Directories to skip during indexing
const SKIP_DIRECTORIES: &[&str] = &[
    ".venv",
    "venv",
    "node_modules",
    ".git",
    ".svn",
    "__pycache__",
    ".cache",
    ".npm",
    ".cargo",
    "target",
    "build",
    "dist",
    ".next",
    ".nuxt",
    "vendor",
    "Pods",
    ".gradle",
    ".idea",
    ".vscode",
    "Library",
    ".Trash",
];

/// Priority levels for processing (lower = higher priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Document = 0,
    Image = 1,
    Audio = 2,
    Video = 3,
}

/// File category for tracking progress
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileCategory {
    Documents,
    Images,
    Audio,
    Videos,
}

impl FileCategory {
    fn from_priority(priority: Priority) -> Self {
        match priority {
            Priority::Document => FileCategory::Documents,
            Priority::Image => FileCategory::Images,
            Priority::Audio => FileCategory::Audio,
            Priority::Video => FileCategory::Videos,
        }
    }
}

/// A file queued for indexing
#[derive(Debug, Clone)]
pub struct QueuedFile {
    pub path: PathBuf,
    pub priority: Priority,
    pub file_type: String, // Extension like "pdf", "jpg", etc.
}

/// Current indexing progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingProgress {
    pub is_indexing: bool,
    pub is_paused: bool,
    pub current_category: Option<FileCategory>,
    pub documents_total: u32,
    pub documents_done: u32,
    pub images_total: u32,
    pub images_done: u32,
    pub audio_total: u32,
    pub audio_done: u32,
    pub videos_total: u32,
    pub videos_done: u32,
    pub current_file: Option<String>,
    pub last_error: Option<String>,
}

impl Default for IndexingProgress {
    fn default() -> Self {
        Self {
            is_indexing: false,
            is_paused: false,
            current_category: None,
            documents_total: 0,
            documents_done: 0,
            images_total: 0,
            images_done: 0,
            audio_total: 0,
            audio_done: 0,
            videos_total: 0,
            videos_done: 0,
            current_file: None,
            last_error: None,
        }
    }
}

impl IndexingProgress {
    pub fn total_files(&self) -> u32 {
        self.documents_total + self.images_total + self.audio_total + self.videos_total
    }

    pub fn done_files(&self) -> u32 {
        self.documents_done + self.images_done + self.audio_done + self.videos_done
    }
}

/// Commands sent to the indexer
pub enum IndexerCommand {
    Pause,
    Resume,
    Stop,
}

/// The main indexer that manages the embedding pipeline
pub struct Indexer {
    database: Arc<VectorDatabase>,
    api_key: String,
    progress: Arc<RwLock<IndexingProgress>>,
    command_tx: Option<mpsc::Sender<IndexerCommand>>,
    queue: Arc<RwLock<VecDeque<QueuedFile>>>,
}

impl Indexer {
    /// Create a new indexer
    pub fn new(database: Arc<VectorDatabase>, api_key: String) -> Self {
        Self {
            database,
            api_key,
            progress: Arc::new(RwLock::new(IndexingProgress::default())),
            command_tx: None,
            queue: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Get the current progress
    pub async fn get_progress(&self) -> IndexingProgress {
        self.progress.read().await.clone()
    }

    /// Scan folders and build the queue
    pub async fn scan_and_queue(&self, folders: &[String]) -> Result<(), String> {
        let mut documents = Vec::new();
        let mut images = Vec::new();
        let mut audio = Vec::new();
        let mut videos = Vec::new();

        for folder in folders {
            let folder_path = PathBuf::from(folder);
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
                if path.is_dir() {
                    continue;
                }

                if let Some((priority, file_type)) = categorize_file(path) {
                    let queued = QueuedFile {
                        path: path.to_path_buf(),
                        priority,
                        file_type,
                    };

                    match priority {
                        Priority::Document => documents.push(queued),
                        Priority::Image => images.push(queued),
                        Priority::Audio => audio.push(queued),
                        Priority::Video => videos.push(queued),
                    }
                }
            }
        }

        // Update progress totals
        {
            let mut progress = self.progress.write().await;
            progress.documents_total = documents.len() as u32;
            progress.images_total = images.len() as u32;
            progress.audio_total = audio.len() as u32;
            progress.videos_total = videos.len() as u32;
        }

        // Build queue in priority order
        {
            let mut queue = self.queue.write().await;
            queue.clear();
            queue.extend(documents);
            queue.extend(images);
            queue.extend(audio);
            queue.extend(videos);
        }

        Ok(())
    }

    /// Start the indexing process
    pub fn start(&mut self) -> mpsc::Receiver<IndexingProgress> {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<IndexerCommand>(10);
        let (progress_tx, progress_rx) = mpsc::channel::<IndexingProgress>(100);

        self.command_tx = Some(cmd_tx);

        let database = self.database.clone();
        let api_key = self.api_key.clone();
        let progress = self.progress.clone();
        let queue = self.queue.clone();

        tokio::spawn(async move {
            let client = EmbeddingClient::new(api_key);
            let mut paused = false;

            // Set indexing started
            {
                let mut p = progress.write().await;
                p.is_indexing = true;
                p.is_paused = false;
            }

            loop {
                // Check for commands (non-blocking)
                match cmd_rx.try_recv() {
                    Ok(IndexerCommand::Pause) => {
                        paused = true;
                        let mut p = progress.write().await;
                        p.is_paused = true;
                        let progress_clone = p.clone();
                        drop(p);
                        let _ = progress_tx.send(progress_clone).await;
                    }
                    Ok(IndexerCommand::Resume) => {
                        paused = false;
                        let mut p = progress.write().await;
                        p.is_paused = false;
                        let progress_clone = p.clone();
                        drop(p);
                        let _ = progress_tx.send(progress_clone).await;
                    }
                    Ok(IndexerCommand::Stop) => {
                        let mut p = progress.write().await;
                        p.is_indexing = false;
                        let progress_clone = p.clone();
                        drop(p);
                        let _ = progress_tx.send(progress_clone).await;
                        break;
                    }
                    Err(_) => {}
                }

                if paused {
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }

                // Get next file from queue
                let next_file = {
                    let mut q = queue.write().await;
                    q.pop_front()
                };

                match next_file {
                    Some(file) => {
                        // Update current file in progress
                        {
                            let mut p = progress.write().await;
                            p.current_category = Some(FileCategory::from_priority(file.priority));
                            p.current_file = file.path.file_name()
                                .and_then(|n| n.to_str())
                                .map(|s| s.to_string());
                        }

                        // Process the file
                        println!("Indexing: {}", file.path.display());
                        let result = process_file(&file, &client, &database).await;
                        if result.is_ok() {
                            println!("  ✓ Success");
                        }

                        // Update progress
                        let progress_clone = {
                            let mut p = progress.write().await;
                            match file.priority {
                                Priority::Document => p.documents_done += 1,
                                Priority::Image => p.images_done += 1,
                                Priority::Audio => p.audio_done += 1,
                                Priority::Video => p.videos_done += 1,
                            }

                            if let Err(e) = result {
                                eprintln!("Failed to index {}: {}", file.path.display(), e);
                                p.last_error = Some(format!("{}: {}", file.path.display(), e));
                            }

                            p.clone()
                        };

                        let _ = progress_tx.send(progress_clone).await;

                        // Rate limiting - wait between API calls
                        sleep(Duration::from_millis(MIN_REQUEST_INTERVAL_MS)).await;
                    }
                    None => {
                        // Queue is empty, indexing complete
                        let progress_clone = {
                            let mut p = progress.write().await;
                            p.is_indexing = false;
                            p.current_category = None;
                            p.current_file = None;
                            p.clone()
                        };
                        let _ = progress_tx.send(progress_clone).await;
                        break;
                    }
                }
            }
        });

        progress_rx
    }

    /// Pause indexing
    pub async fn pause(&self) -> Result<(), String> {
        if let Some(tx) = &self.command_tx {
            tx.send(IndexerCommand::Pause)
                .await
                .map_err(|_| "Failed to send pause command".to_string())
        } else {
            Err("Indexer not running".to_string())
        }
    }

    /// Resume indexing
    pub async fn resume(&self) -> Result<(), String> {
        if let Some(tx) = &self.command_tx {
            tx.send(IndexerCommand::Resume)
                .await
                .map_err(|_| "Failed to send resume command".to_string())
        } else {
            Err("Indexer not running".to_string())
        }
    }

    /// Stop indexing
    pub async fn stop(&self) -> Result<(), String> {
        if let Some(tx) = &self.command_tx {
            tx.send(IndexerCommand::Stop)
                .await
                .map_err(|_| "Failed to send stop command".to_string())
        } else {
            Err("Indexer not running".to_string())
        }
    }
}

/// Process a single file through the appropriate pipeline
async fn process_file(
    file: &QueuedFile,
    client: &EmbeddingClient,
    database: &VectorDatabase,
) -> Result<(), String> {
    let path = &file.path;

    // Check if file still exists
    if !path.exists() {
        return Err("File no longer exists".to_string());
    }

    // Get file metadata
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;

    let last_modified = metadata
        .modified()
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64)
        .unwrap_or_else(|_| Utc::now().timestamp_millis());

    // Check if file is already indexed with same modification time
    if let Ok(Some(existing_modified)) = database.get_last_modified(path.to_str().unwrap()).await {
        if existing_modified == last_modified {
            return Ok(()); // Already up to date
        }
    }

    // Process based on file type
    let (vector, transcript) = match file.priority {
        Priority::Document => {
            let vector = client.embed_document(path, TaskType::RetrievalDocument).await?;
            (vector, String::new())
        }
        Priority::Image => {
            let vector = client.embed_image(path, TaskType::RetrievalDocument).await?;
            (vector, String::new())
        }
        Priority::Audio => {
            let transcript = transcribe_audio(path).await?;
            let vector = client.embed_text(&transcript, TaskType::RetrievalDocument).await?;
            (vector, transcript)
        }
        Priority::Video => {
            let transcript = transcribe_video(path).await?;
            let vector = client.embed_text(&transcript, TaskType::RetrievalDocument).await?;
            (vector, transcript)
        }
    };

    // Create the record
    let record = FileRecord {
        file_path: path.to_str().unwrap().to_string(),
        file_name: path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        file_type: file.file_type.clone(),
        last_modified,
        transcript,
        vector,
    };

    // Insert or update in database
    if database.exists(path.to_str().unwrap()).await? {
        database.update(record).await
    } else {
        database.insert(record).await
    }
}

/// Categorize a file and return its priority and type
fn categorize_file(path: &Path) -> Option<(Priority, String)> {
    let extension = path.extension()?.to_str()?.to_lowercase();

    match extension.as_str() {
        // Documents
        "pdf" | "txt" | "md" | "docx" => Some((Priority::Document, extension)),
        // Images
        "jpg" | "jpeg" | "png" | "webp" | "heic" => Some((Priority::Image, extension)),
        // Audio
        "mp3" | "wav" | "m4a" | "aac" => Some((Priority::Audio, extension)),
        // Video
        "mp4" | "mov" => Some((Priority::Video, extension)),
        // Skip everything else
        _ => None,
    }
}

/// Check if a directory should be skipped during indexing
fn should_skip_dir(path: &Path) -> bool {
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
