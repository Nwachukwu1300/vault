//! Local model files: where they live (and, from Phase 3, how they get there).

use std::path::PathBuf;

/// `~/Library/Application Support/com.vault.app/models` on macOS.
pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.vault.app")
        .join("models")
}
