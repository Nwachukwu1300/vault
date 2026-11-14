//! Audio/Video transcription module using Whisper.cpp
//!
//! This module handles transcription of audio and video files using a local
//! Whisper model. Videos are first converted to audio, then transcribed.

use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;
use uuid::Uuid;

/// Get the path to the whisper model file
fn get_model_path() -> Result<PathBuf, String> {
    // Check common locations for whisper models
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    let possible_paths = [
        // User's .cache directory (common download location)
        home.join(".cache/whisper/ggml-base.bin"),
        home.join(".cache/whisper/ggml-base.en.bin"),
        // Application data directory
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.vault.app/models/ggml-base.bin"),
        // Homebrew locations
        PathBuf::from("/opt/homebrew/share/whisper-cpp/models/ggml-base.bin"),
        PathBuf::from("/usr/local/share/whisper-cpp/models/ggml-base.bin"),
        // Current directory
        PathBuf::from("models/ggml-base.bin"),
    ];

    for path in possible_paths {
        if path.exists() {
            return Ok(path);
        }
    }

    Err(
        "Whisper model not found. Please download a model:\n\
        curl -L -o ~/.cache/whisper/ggml-base.bin \\\n\
          https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin\n\n\
        Or visit: https://huggingface.co/ggerganov/whisper.cpp/tree/main".to_string()
    )
}

/// Transcribe an audio file using Whisper.cpp
///
/// For the MVP, we use the whisper-cli command-line tool which should be installed
/// on the system. The tool handles chunking internally for files longer than 80s.
pub async fn transcribe_audio(audio_path: &Path) -> Result<String, String> {
    // Check if whisper is available
    let whisper_path = find_whisper_binary()?;

    // Find the model file
    let model_path = get_model_path()?;

    // Create a temporary output file path
    let temp_dir = std::env::temp_dir();
    let output_id = Uuid::new_v4();
    let output_base = temp_dir.join(format!("vault_whisper_{}", output_id));

    // Run whisper-cli on the audio file
    // whisper-cli uses different flags than the Python whisper
    let output = Command::new(&whisper_path)
        .args([
            "-m", model_path.to_str().unwrap(),
            "-f", audio_path.to_str().unwrap(),
            "-of", output_base.to_str().unwrap(),
            "-otxt",  // Output as text file
            "-np",    // No progress output
        ])
        .output()
        .map_err(|e| format!("Failed to run Whisper: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("Whisper transcription failed:\nstderr: {}\nstdout: {}", stderr, stdout));
    }

    // whisper-cli outputs to output_base.txt
    let output_txt = PathBuf::from(format!("{}.txt", output_base.display()));

    // Read the transcript
    let transcript = fs::read_to_string(&output_txt)
        .await
        .map_err(|e| format!("Failed to read transcript from {}: {}", output_txt.display(), e))?;

    // Clean up temp file
    let _ = fs::remove_file(&output_txt).await;

    Ok(transcript.trim().to_string())
}

/// Extract audio from a video file and transcribe it
pub async fn transcribe_video(video_path: &Path) -> Result<String, String> {
    // First, extract audio from video using ffmpeg
    let audio_path = extract_audio_from_video(video_path).await?;

    // Then transcribe the extracted audio
    let transcript = transcribe_audio(&audio_path).await;

    // Clean up the temporary audio file
    let _ = fs::remove_file(&audio_path).await;

    transcript
}

/// Extract audio track from video file using ffmpeg
async fn extract_audio_from_video(video_path: &Path) -> Result<PathBuf, String> {
    let ffmpeg_path = find_ffmpeg_binary()?;

    // Create a temporary audio file
    let temp_dir = std::env::temp_dir();
    let audio_id = Uuid::new_v4();
    let audio_path = temp_dir.join(format!("vault_audio_{}.wav", audio_id));

    // Extract audio using ffmpeg
    // -vn: no video
    // -acodec pcm_s16le: 16-bit PCM audio (WAV format)
    // -ar 16000: 16kHz sample rate (optimal for Whisper)
    // -ac 1: mono audio
    let output = Command::new(&ffmpeg_path)
        .args([
            "-i", video_path.to_str().unwrap(),
            "-vn",
            "-acodec", "pcm_s16le",
            "-ar", "16000",
            "-ac", "1",
            "-y", // Overwrite output file
            audio_path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Audio extraction failed: {}", stderr));
    }

    Ok(audio_path)
}

/// Find the Whisper binary on the system
fn find_whisper_binary() -> Result<String, String> {
    // Check common locations for whisper-cli (the actual binary name from whisper-cpp)
    let possible_paths = [
        "whisper-cli",                       // In PATH (Homebrew whisper-cpp)
        "/opt/homebrew/bin/whisper-cli",    // Homebrew on Apple Silicon
        "/usr/local/bin/whisper-cli",       // Homebrew on Intel Mac
        "whisper",                           // Alternative name
        "/usr/local/bin/whisper",
        "/opt/homebrew/bin/whisper",
    ];

    for path in possible_paths {
        if Command::new(path).arg("--help").output().is_ok() {
            return Ok(path.to_string());
        }
    }

    // If whisper isn't found, provide installation instructions
    Err(
        "Whisper not found. Please install whisper.cpp:\n\
        brew install whisper-cpp\n\
        Or download from: https://github.com/ggerganov/whisper.cpp".to_string()
    )
}

/// Find ffmpeg binary on the system
fn find_ffmpeg_binary() -> Result<String, String> {
    let possible_paths = [
        "ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/homebrew/bin/ffmpeg",
    ];

    for path in possible_paths {
        if Command::new(path).arg("-version").output().is_ok() {
            return Ok(path.to_string());
        }
    }

    Err(
        "ffmpeg not found. Please install ffmpeg:\n\
        brew install ffmpeg\n\
        Or download from: https://ffmpeg.org/download.html".to_string()
    )
}

/// Check if transcription dependencies are available
pub fn check_dependencies() -> Result<(), String> {
    find_whisper_binary()?;
    find_ffmpeg_binary()?;
    get_model_path()?;
    Ok(())
}
