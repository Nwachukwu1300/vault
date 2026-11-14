//! Gemini Embedding 2 API integration
//!
//! This module handles all interactions with the Google Gemini Embedding 2 API.
//! It supports embedding text, images, and documents.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// Gemini API endpoint for embeddings
/// Using gemini-embedding-2 multimodal model (text, images, PDFs, audio, video)
/// Supports output dimensions: 256, 512, 768, 1024, 2048, 3072 (default: 3072)
const GEMINI_EMBED_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-2:embedContent";

/// Vector dimension for gemini-embedding-2
/// Using 3072 (the default and maximum for best quality)
pub const VECTOR_DIMENSION: usize = 3072;

/// Task type for embedding - use RETRIEVAL_DOCUMENT for files, RETRIEVAL_QUERY for search
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskType {
    RetrievalDocument,
    RetrievalQuery,
}

/// Request body for Gemini embedding API
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EmbedRequest {
    model: String,
    content: EmbedContent,
    task_type: TaskType,
}

#[derive(Debug, Serialize)]
struct EmbedContent {
    parts: Vec<EmbedPart>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum EmbedPart {
    Text { text: String },
    InlineData { inline_data: InlineData },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

/// Response from Gemini embedding API
#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: Option<EmbeddingValues>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

/// Client for interacting with Gemini Embedding API
pub struct EmbeddingClient {
    client: Client,
    api_key: String,
}

impl EmbeddingClient {
    /// Create a new embedding client with the given API key
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    /// Embed text content (for documents, transcripts)
    pub async fn embed_text(&self, text: &str, task_type: TaskType) -> Result<Vec<f32>, String> {
        let request = EmbedRequest {
            model: "models/gemini-embedding-2".to_string(),
            content: EmbedContent {
                parts: vec![EmbedPart::Text {
                    text: text.to_string(),
                }],
            },
            task_type,
        };

        self.send_request(&request).await
    }

    /// Embed an image file
    pub async fn embed_image(&self, path: &Path, task_type: TaskType) -> Result<Vec<f32>, String> {
        let data = fs::read(path)
            .await
            .map_err(|e| format!("Failed to read image file: {}", e))?;

        let mime_type = get_mime_type(path)?;
        let base64_data = BASE64.encode(&data);

        let request = EmbedRequest {
            model: "models/gemini-embedding-2".to_string(),
            content: EmbedContent {
                parts: vec![EmbedPart::InlineData {
                    inline_data: InlineData {
                        mime_type,
                        data: base64_data,
                    },
                }],
            },
            task_type,
        };

        self.send_request(&request).await
    }

    /// Embed a document file (PDF, TXT, MD, DOCX)
    pub async fn embed_document(
        &self,
        path: &Path,
        task_type: TaskType,
    ) -> Result<Vec<f32>, String> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        match extension.as_str() {
            "txt" | "md" => {
                // Read text files directly
                let content = fs::read_to_string(path)
                    .await
                    .map_err(|e| format!("Failed to read text file: {}", e))?;
                self.embed_text(&content, task_type).await
            }
            "pdf" => {
                // For PDF, send as binary with mime type (supported by gemini-embedding-2)
                let data = fs::read(path)
                    .await
                    .map_err(|e| format!("Failed to read PDF file: {}", e))?;

                let base64_data = BASE64.encode(&data);

                let request = EmbedRequest {
                    model: "models/gemini-embedding-2".to_string(),
                    content: EmbedContent {
                        parts: vec![EmbedPart::InlineData {
                            inline_data: InlineData {
                                mime_type: "application/pdf".to_string(),
                                data: base64_data,
                            },
                        }],
                    },
                    task_type,
                };

                self.send_request(&request).await
            }
            "docx" => {
                // DOCX is not directly supported by the embedding API
                // Extract text content and embed as text
                let content = extract_docx_text(path).await?;
                self.embed_text(&content, task_type).await
            }
            _ => Err(format!("Unsupported document type: {}", extension)),
        }
    }

    /// Send request to Gemini API
    async fn send_request(&self, request: &EmbedRequest) -> Result<Vec<f32>, String> {
        let response = self
            .client
            .post(GEMINI_EMBED_URL)
            .query(&[("key", &self.api_key)])
            .json(request)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let status = response.status();
        let body: EmbedResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = body.error {
            return Err(format!("API error ({}): {}", status, error.message));
        }

        body.embedding
            .map(|e| e.values)
            .ok_or_else(|| "No embedding returned from API".to_string())
    }
}

/// Extract text from a DOCX file
/// DOCX is a ZIP archive containing XML files
async fn extract_docx_text(path: &Path) -> Result<String, String> {
    use std::io::Read;

    let data = fs::read(path)
        .await
        .map_err(|e| format!("Failed to read DOCX file: {}", e))?;

    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("Failed to open DOCX as ZIP: {}", e))?;

    // The main document content is in word/document.xml
    let mut document_xml = archive
        .by_name("word/document.xml")
        .map_err(|e| format!("Failed to find document.xml in DOCX: {}", e))?;

    let mut xml_content = String::new();
    document_xml
        .read_to_string(&mut xml_content)
        .map_err(|e| format!("Failed to read document.xml: {}", e))?;

    // Extract text from XML by removing tags (simple approach)
    // This extracts content between <w:t> tags which contain the actual text
    let mut text = String::new();
    let mut in_text_tag = false;
    let mut chars = xml_content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            // Check if this is a text tag
            let mut tag = String::new();
            while let Some(&next) = chars.peek() {
                if next == '>' {
                    chars.next();
                    break;
                }
                tag.push(chars.next().unwrap());
            }

            if tag.starts_with("w:t") && !tag.starts_with("w:t ") || tag.starts_with("w:t>") || tag.starts_with("w:t ") {
                in_text_tag = true;
            } else if tag == "/w:t" {
                in_text_tag = false;
            } else if tag == "w:p" || tag.starts_with("w:p ") || tag.starts_with("w:p>") {
                // Paragraph break
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
            }
        } else if in_text_tag {
            text.push(c);
        }
    }

    Ok(text.trim().to_string())
}

/// Get MIME type for a file based on extension
fn get_mime_type(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "jpg" | "jpeg" => Ok("image/jpeg".to_string()),
        "png" => Ok("image/png".to_string()),
        "webp" => Ok("image/webp".to_string()),
        "heic" => Ok("image/heic".to_string()),
        "pdf" => Ok("application/pdf".to_string()),
        "txt" => Ok("text/plain".to_string()),
        "md" => Ok("text/markdown".to_string()),
        _ => Err(format!("Unknown file type: {}", extension)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_types() {
        assert_eq!(
            get_mime_type(Path::new("test.jpg")).unwrap(),
            "image/jpeg"
        );
        assert_eq!(get_mime_type(Path::new("test.png")).unwrap(), "image/png");
        assert_eq!(
            get_mime_type(Path::new("test.pdf")).unwrap(),
            "application/pdf"
        );
    }
}
