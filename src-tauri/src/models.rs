//! Local model files: where they live and how they get there.
//!
//! Weights are not bundled in the installer. On first launch they are
//! downloaded into the models dir with resume, SHA256 verification and retry.

use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// `~/Library/Application Support/com.vault.app/models` on macOS.
pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.vault.app")
        .join("models")
}

pub struct ModelFile {
    /// Path relative to the models dir.
    pub path: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

// Repo revisions are pinned in the URLs so the checksums stay valid.
pub const MANIFEST: &[ModelFile] = &[
    ModelFile {
        path: "embeddinggemma-2/tokenizer.json",
        url: "https://huggingface.co/onnx-community/embeddinggemma-2-ONNX/resolve/daa72c51243991dfcaf9f9137d2c573d8f7790c0/tokenizer.json",
        size: 32_170_510,
        sha256: "4d777ef5bdc1aa36227abdfb77c3e49e7b9c892d16e1b6bda41c393504828be4",
    },
    ModelFile {
        path: "embeddinggemma-2/onnx/model_quantized.onnx",
        url: "https://huggingface.co/onnx-community/embeddinggemma-2-ONNX/resolve/daa72c51243991dfcaf9f9137d2c573d8f7790c0/onnx/model_quantized.onnx",
        size: 495_165,
        sha256: "d06edd601f851c633a2519304cbeb8dc6170d7ceb61b436625c17fb9b6e74953",
    },
    ModelFile {
        path: "embeddinggemma-2/onnx/model_quantized.onnx_data",
        url: "https://huggingface.co/onnx-community/embeddinggemma-2-ONNX/resolve/daa72c51243991dfcaf9f9137d2c573d8f7790c0/onnx/model_quantized.onnx_data",
        size: 313_724_928,
        sha256: "278a7ff1248c3618e4bd11a607fc54f7bdc7778854230f3956d3f86bd9db4f3b",
    },
    ModelFile {
        path: "embeddinggemma-2/onnx/vision_encoder_quantized.onnx",
        url: "https://huggingface.co/onnx-community/embeddinggemma-2-ONNX/resolve/daa72c51243991dfcaf9f9137d2c573d8f7790c0/onnx/vision_encoder_quantized.onnx",
        size: 162_495,
        sha256: "bb0de2df53a2448a32dc7908a187c168c8afd514d4d6f674f7f46024875fa4e3",
    },
    ModelFile {
        path: "embeddinggemma-2/onnx/vision_encoder_quantized.onnx_data",
        url: "https://huggingface.co/onnx-community/embeddinggemma-2-ONNX/resolve/daa72c51243991dfcaf9f9137d2c573d8f7790c0/onnx/vision_encoder_quantized.onnx_data",
        size: 195_228_672,
        sha256: "3dabd69c0a36e9a8771ad82030dde74daa5a0e02b7047a5d3f3382b1137bab89",
    },
    ModelFile {
        path: "whisper/ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin",
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
];

const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub ready: bool,
    pub total_bytes: u64,
    pub missing_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub file: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// A file counts as present when it exists at the expected size. Hashes are
/// checked once, at download time; re-hashing ~690 MB every launch is too slow.
fn is_present(dir: &Path, f: &ModelFile) -> bool {
    std::fs::metadata(dir.join(f.path)).is_ok_and(|m| m.len() == f.size)
}

pub fn status(dir: &Path, files: &[ModelFile]) -> ModelStatus {
    let missing_bytes = files.iter().filter(|f| !is_present(dir, f)).map(|f| f.size).sum();
    ModelStatus {
        ready: missing_bytes == 0,
        total_bytes: files.iter().map(|f| f.size).sum(),
        missing_bytes,
    }
}

/// Download every missing file. Progress counts bytes across all files.
pub async fn download_all(
    dir: &Path,
    files: &[ModelFile],
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let total: u64 = files.iter().map(|f| f.size).sum();
    let mut done: u64 = files.iter().filter(|f| is_present(dir, f)).map(|f| f.size).sum();

    for f in files.iter().filter(|f| !is_present(dir, f)) {
        let dest = dir.join(f.path);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = download_one(&client, f, &dest, cancel, |n| {
                on_progress(DownloadProgress {
                    file: f.path.to_string(),
                    downloaded_bytes: done + n,
                    total_bytes: total,
                })
            })
            .await;
            match result {
                Ok(()) => break,
                Err(e) if cancel.load(Ordering::Relaxed) || attempt >= ATTEMPTS => {
                    return Err(format!("{}: {e}", f.path));
                }
                Err(_) => tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await,
            }
        }
        done += f.size;
    }
    Ok(())
}

/// Stream into `<dest>.part`, resuming with a Range request if a partial file
/// exists, then verify SHA256 and rename into place.
async fn download_one(
    client: &reqwest::Client,
    f: &ModelFile,
    dest: &Path,
    cancel: &AtomicBool,
    mut on_bytes: impl FnMut(u64),
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    if have > f.size {
        tokio::fs::remove_file(&part).await.map_err(|e| e.to_string())?;
        have = 0;
    }

    if have < f.size {
        let mut req = client.get(f.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }
        // A server that ignores Range sends the whole file with 200.
        let append = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !append {
            have = 0;
        }
        let mut out = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&part)
            .await
            .map_err(|e| e.to_string())?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                out.flush().await.map_err(|e| e.to_string())?;
                return Err("cancelled".into());
            }
            let chunk = chunk.map_err(|e| e.to_string())?;
            out.write_all(&chunk).await.map_err(|e| e.to_string())?;
            have += chunk.len() as u64;
            on_bytes(have);
        }
        out.flush().await.map_err(|e| e.to_string())?;
    }

    let hash = sha256_file(&part).await?;
    if hash != f.sha256 {
        // Corrupt or wrong content: start over on the next attempt.
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!("checksum mismatch (got {hash})"));
    }
    tokio::fs::rename(&part, dest).await.map_err(|e| e.to_string())
}

async fn sha256_file(path: &Path) -> Result<String, String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    /// Minimal HTTP server for one file that honours `Range: bytes=N-`.
    async fn serve(body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let start = req
                    .lines()
                    .find_map(|l| l.strip_prefix("range: bytes="))
                    .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok());
                let (status, slice) = match start {
                    Some(s) => ("206 Partial Content", &body[s..]),
                    None => ("200 OK", body),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    slice.len()
                );
                sock.write_all(head.as_bytes()).await.unwrap();
                sock.write_all(slice).await.unwrap();
            }
        });
        format!("http://{addr}/f")
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    const BODY: &[u8] = b"hello vault model bytes";
    // sha256 of BODY
    fn body_sha() -> &'static str {
        leak(format!("{:x}", Sha256::digest(BODY)))
    }

    #[tokio::test]
    async fn downloads_resumes_and_verifies() {
        let url = leak(serve(BODY).await);
        let dir = tempfile::tempdir().unwrap();
        let files = [ModelFile { path: "a/f.bin", url, size: BODY.len() as u64, sha256: body_sha() }];
        assert!(!status(dir.path(), &files).ready);

        // Pre-seed a partial download; only the rest should be fetched.
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/f.bin.part"), &BODY[..6]).unwrap();
        let mut last = 0;
        download_all(dir.path(), &files, &AtomicBool::new(false), |p| last = p.downloaded_bytes)
            .await
            .unwrap();

        assert_eq!(std::fs::read(dir.path().join("a/f.bin")).unwrap(), BODY);
        assert_eq!(last, BODY.len() as u64);
        assert!(!dir.path().join("a/f.bin.part").exists());
        assert!(status(dir.path(), &files).ready);
    }

    #[tokio::test]
    async fn bad_checksum_fails_and_leaves_nothing() {
        let url = leak(serve(BODY).await);
        let dir = tempfile::tempdir().unwrap();
        let files = [ModelFile { path: "f.bin", url, size: BODY.len() as u64, sha256: "00" }];
        // One attempt only, so the test doesn't sleep through retries.
        let err = download_one(
            &reqwest::Client::new(),
            &files[0],
            &dir.path().join("f.bin"),
            &AtomicBool::new(false),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(!dir.path().join("f.bin").exists());
        assert!(!dir.path().join("f.bin.part").exists());
    }
}

#[cfg(test)]
mod real {
    use super::*;

    /// Downloads whatever is missing from the real manifest into the real models dir.
    #[tokio::test]
    #[ignore]
    async fn real_manifest_downloads() {
        let dir = models_dir();
        println!("before: {:?}", status(&dir, MANIFEST));
        download_all(&dir, MANIFEST, &AtomicBool::new(false), |_| {}).await.unwrap();
        let after = status(&dir, MANIFEST);
        println!("after: {after:?}");
        assert!(after.ready);
    }
}
