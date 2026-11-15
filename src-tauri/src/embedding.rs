//! Local EmbeddingGemma 2 inference (ONNX Runtime via `ort`).
//!
//! Text, images and transcripts all land in one shared 768-dim space, so a
//! text query can be compared directly with any stored vector. The vision
//! preprocessing mirrors transformers.js `Gemma4ImageProcessor`.

use image::imageops::FilterType;
use ort::session::builder::AutoDevicePolicy;
use ort::session::Session;
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokenizers::Tokenizer;

pub const VECTOR_DIMENSION: usize = 768;

/// Width of the soft-token features the vision encoder hands to the text model.
const FEATURE_DIM: usize = 512;
const PATCH_SIZE: usize = 16;
const POOLING: usize = 3;
/// Vision token budget (model supports 70..1120, default 280). 140 is ~2.5x
/// faster than 280 on CPU and still ranks the model-card example correctly.
const MAX_SOFT_TOKENS: usize = 140;
const MAX_PATCHES: usize = MAX_SOFT_TOKENS * POOLING * POOLING;
const PATCH_DIM: usize = PATCH_SIZE * PATCH_SIZE * 3;
/// The model's context window.
const MAX_TOKENS: usize = 8192;

pub const TEXT_MODEL: &str = "embeddinggemma-2/onnx/model_quantized.onnx";
pub const VISION_MODEL: &str = "embeddinggemma-2/onnx/vision_encoder_quantized.onnx";
pub const TOKENIZER: &str = "embeddinggemma-2/tokenizer.json";

/// Prefix from the model card for search queries.
pub fn query_prompt(query: &str) -> String {
    format!("task: search result | query: {query}")
}

/// Prefix from the model card for stored documents; `title: none` when there is no title.
pub fn document_prompt(title: Option<&str>, text: &str) -> String {
    format!("title: {} | text: {text}", title.unwrap_or("none"))
}

pub struct Embedder {
    text: Mutex<Session>,
    vision: Mutex<Session>,
    tokenizer: Tokenizer,
    pad_id: i64,
    // Kept alive for the sessions' lifetime.
    _env: ort::environment::Environment,
}

impl Embedder {
    /// Load the models from `models_dir`.
    ///
    /// Everything runs on CPU. The CoreML provider was measured slower (it
    /// recompiles for each new input shape: 119 s for a second image vs 2 s
    /// on CPU) and rejects the text model's empty media inputs.
    pub fn load(models_dir: &Path) -> Result<Self, String> {
        let env = ort::init()
            .with_name("vault")
            .build()
            .map_err(|e| format!("Failed to start ONNX Runtime: {e}"))?;

        let session = |file: &str| -> Result<Session, String> {
            Session::builder(&env)
                // ort's default auto-device policy would pick CoreML on macOS.
                .and_then(|b| Ok(b.with_auto_device(AutoDevicePolicy::PreferCPU)?))
                .and_then(|mut b| b.commit_from_file(models_dir.join(file)))
                .map_err(|e| format!("Failed to load {file}: {e}"))
        };
        let text = session(TEXT_MODEL)?;
        let vision = session(VISION_MODEL)?;

        let tokenizer = Tokenizer::from_file(models_dir.join(TOKENIZER))
            .map_err(|e| format!("Failed to load tokenizer: {e}"))?;
        let pad_id = tokenizer.token_to_id("<pad>").unwrap_or(0) as i64;

        Ok(Self {
            text: Mutex::new(text),
            vision: Mutex::new(vision),
            tokenizer,
            pad_id,
            _env: env,
        })
    }

    /// Token count without special tokens, for sizing chunks.
    pub fn count_tokens(&self, text: &str) -> usize {
        self.tokenizer.encode(text, false).map(|e| e.len()).unwrap_or(0)
    }

    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        Ok(self.embed_texts(&[query_prompt(query)])?.remove(0))
    }

    /// Embed document chunks, each as (title, text), in one batch.
    pub fn embed_documents(&self, docs: &[(Option<&str>, &str)]) -> Result<Vec<Vec<f32>>, String> {
        let prompts: Vec<String> = docs.iter().map(|(t, x)| document_prompt(*t, x)).collect();
        self.embed_texts(&prompts)
    }

    fn embed_texts(&self, prompts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if prompts.is_empty() {
            return Ok(Vec::new());
        }
        let encodings = self
            .tokenizer
            .encode_batch(prompts.to_vec(), true)
            .map_err(|e| format!("Tokenizer error: {e}"))?;
        let ids: Vec<Vec<i64>> = encodings
            .iter()
            .map(|e| e.get_ids().iter().take(MAX_TOKENS).map(|&i| i as i64).collect())
            .collect();
        self.run_text(&ids, empty_features()?)
    }

    /// Embed an image file. HEIC is converted with macOS `sips` first since
    /// the `image` crate can't decode it.
    pub fn embed_image(&self, path: &Path) -> Result<Vec<f32>, String> {
        let img = load_rgb(path)?;
        let (patches, positions, soft_tokens) = patchify(&img)?;

        let features = {
            let mut vision = self.vision.lock().map_err(|_| "Vision model lock poisoned")?;
            let outputs = vision
                .run(ort::inputs![
                    "pixel_values" => Tensor::from_array(([1, MAX_PATCHES, PATCH_DIM], patches)).map_err(ort_err)?,
                    "pixel_position_ids" => Tensor::from_array(([1, MAX_PATCHES, 2], positions)).map_err(ort_err)?,
                ])
                .map_err(ort_err)?;
            let (shape, data) = outputs["image_features"]
                .try_extract_tensor::<f32>()
                .map_err(ort_err)?;
            if shape[0] as usize != soft_tokens {
                return Err(format!("Vision encoder returned {} tokens, expected {soft_tokens}", shape[0]));
            }
            data.to_vec()
        };

        // Same layout the processor builds: <bos><|image> <|image|>×N <image|><eos>
        let prompt = format!("<|image>{}<image|>", "<|image|>".repeat(soft_tokens));
        let enc = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| format!("Tokenizer error: {e}"))?;
        let ids = vec![enc.get_ids().iter().map(|&i| i as i64).collect()];
        let features = Tensor::from_array(([soft_tokens, FEATURE_DIM], features)).map_err(ort_err)?;
        Ok(self.run_text(&ids, features)?.remove(0))
    }

    fn run_text(&self, ids: &[Vec<i64>], image_features: Tensor<f32>) -> Result<Vec<Vec<f32>>, String> {
        let batch = ids.len();
        let seq = ids.iter().map(Vec::len).max().unwrap_or(0);
        let mut input_ids = Vec::with_capacity(batch * seq);
        let mut mask = Vec::with_capacity(batch * seq);
        for row in ids {
            input_ids.extend_from_slice(row);
            mask.extend(std::iter::repeat_n(1i64, row.len()));
            input_ids.extend(std::iter::repeat_n(self.pad_id, seq - row.len()));
            mask.extend(std::iter::repeat_n(0i64, seq - row.len()));
        }

        let mut text = self.text.lock().map_err(|_| "Text model lock poisoned")?;
        let outputs = text
            .run(ort::inputs![
                "input_ids" => Tensor::from_array(([batch, seq], input_ids)).map_err(ort_err)?,
                "attention_mask" => Tensor::from_array(([batch, seq], mask)).map_err(ort_err)?,
                "image_features" => image_features,
                "video_features" => empty_features()?,
                "audio_features" => empty_features()?,
            ])
            .map_err(ort_err)?;
        let (_, data) = outputs["sentence_embedding"]
            .try_extract_tensor::<f32>()
            .map_err(ort_err)?;
        Ok(data.chunks(VECTOR_DIMENSION).map(normalize).collect())
    }
}

fn ort_err(e: ort::Error) -> String {
    format!("Inference error: {e}")
}

fn empty_features() -> Result<Tensor<f32>, String> {
    Tensor::from_array(([0usize, FEATURE_DIM], Vec::<f32>::new())).map_err(ort_err)
}

/// The model already L2-normalizes; doing it again is cheap and guarantees
/// dot product == cosine similarity.
fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / norm).collect()
}

fn load_rgb(path: &Path) -> Result<image::RgbImage, String> {
    let is_heic = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("heic"));
    if !is_heic {
        return image::open(path)
            .map(|i| i.to_rgb8())
            .map_err(|e| format!("Failed to decode image: {e}"));
    }
    let tmp: PathBuf = std::env::temp_dir().join(format!("vault_heic_{}.jpg", uuid::Uuid::new_v4()));
    let out = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg"])
        .arg(path)
        .arg("--out")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("Failed to run sips: {e}"))?;
    let result = if out.status.success() {
        image::open(&tmp)
            .map(|i| i.to_rgb8())
            .map_err(|e| format!("Failed to decode converted HEIC: {e}"))
    } else {
        Err(format!("sips failed: {}", String::from_utf8_lossy(&out.stderr)))
    };
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Aspect-preserving target size with sides in multiples of 48 px
/// (`get_aspect_ratio_preserving_size` in transformers.js).
fn target_size(h: usize, w: usize) -> (usize, usize) {
    let target_px = (MAX_PATCHES * PATCH_SIZE * PATCH_SIZE) as f64;
    let factor = (target_px / (h * w) as f64).sqrt();
    let side = POOLING * PATCH_SIZE;
    let mut th = ((factor * h as f64) / side as f64).floor() as usize * side;
    let mut tw = ((factor * w as f64) / side as f64).floor() as usize * side;
    let max_side = (MAX_PATCHES / (POOLING * POOLING)) * side;
    if th == 0 {
        th = side;
        tw = ((w / h) * side).min(max_side);
    } else if tw == 0 {
        tw = side;
        th = ((h / w) * side).min(max_side);
    }
    (th, tw)
}

/// Resize, rescale to 0..1, cut into 16x16 patches (HWC order inside each
/// patch), and pad to MAX_PATCHES. Positions are (col, row), -1 for padding.
fn patchify(img: &image::RgbImage) -> Result<(Vec<f32>, Vec<i64>, usize), String> {
    let (h, w) = (img.height() as usize, img.width() as usize);
    if h == 0 || w == 0 {
        return Err("Image has no pixels".into());
    }
    let (th, tw) = target_size(h, w);
    let resized = if (th, tw) == (h, w) {
        img.clone()
    } else {
        image::imageops::resize(img, tw as u32, th as u32, FilterType::CatmullRom)
    };

    let (ph, pw) = (th / PATCH_SIZE, tw / PATCH_SIZE);
    let mut patches = vec![0f32; MAX_PATCHES * PATCH_DIM];
    let mut positions = vec![-1i64; MAX_PATCHES * 2];
    let raw = resized.as_raw();
    let mut out = 0;
    for r in 0..ph {
        for c in 0..pw {
            let idx = r * pw + c;
            positions[idx * 2] = c as i64;
            positions[idx * 2 + 1] = r as i64;
            for dy in 0..PATCH_SIZE {
                let row = ((r * PATCH_SIZE + dy) * tw + c * PATCH_SIZE) * 3;
                for v in &raw[row..row + PATCH_SIZE * 3] {
                    patches[out] = *v as f32 / 255.0;
                    out += 1;
                }
            }
        }
    }
    Ok((patches, positions, (ph * pw) / (POOLING * POOLING)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_match_model_card() {
        assert_eq!(query_prompt("cats"), "task: search result | query: cats");
        assert_eq!(document_prompt(None, "hi"), "title: none | text: hi");
        assert_eq!(document_prompt(Some("a.md"), "hi"), "title: a.md | text: hi");
    }

    #[test]
    fn target_size_stays_within_budget() {
        for (h, w) in [(480, 640), (4032, 3024), (100, 5000), (10, 10)] {
            let (th, tw) = target_size(h, w);
            assert_eq!(th % 48, 0);
            assert_eq!(tw % 48, 0);
            assert!((th / 16) * (tw / 16) <= MAX_PATCHES, "{h}x{w} -> {th}x{tw}");
        }
    }

    #[test]
    fn patchify_layout() {
        let img = image::RgbImage::from_fn(48, 96, |x, y| image::Rgb([x as u8, y as u8, 7]));
        let (patches, pos, soft) = patchify(&img).unwrap();
        // Soft tokens are real (unpadded) patches / 9.
        let n = pos.chunks(2).filter(|p| p[0] >= 0).count();
        assert_eq!(soft, n / 9);
        // First pixel of patch 1 (col 1, row 0) is x=16, y=0 in the resized image.
        assert_eq!(&pos[2..4], &[1, 0]);
        assert!(patches[PATCH_DIM..PATCH_DIM + 3].iter().all(|v| (0.0..=1.0).contains(v)));
    }

    fn models() -> PathBuf {
        crate::models::models_dir()
    }

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    /// Needs downloaded models. Reference scores from the model card (q4,
    /// budget 280): Mars doc 0.854; cats image vs queries 0.740 / 0.462 / 0.508.
    /// Set VAULT_TEST_IMAGE to the card's cats.jpg to check images too.
    #[test]
    #[ignore]
    fn matches_model_card_reference() {
        let e = Embedder::load(&models()).unwrap();
        let q = e.embed_query("Which planet is known as the Red Planet?").unwrap();
        let docs = e
            .embed_documents(&[
                (None, "Venus is often called Earth's twin because of its similar size and proximity."),
                (None, "Mars, known for its reddish appearance, is often referred to as the Red Planet."),
            ])
            .unwrap();
        let (venus, mars) = (dot(&q, &docs[0]), dot(&q, &docs[1]));
        println!("mars {mars:.3} venus {venus:.3}");
        assert_eq!(q.len(), VECTOR_DIMENSION);
        assert!(mars > venus && (mars - 0.854).abs() < 0.05);

        if let Ok(img) = std::env::var("VAULT_TEST_IMAGE") {
            let start = std::time::Instant::now();
            let v = e.embed_image(Path::new(&img)).unwrap();
            println!("image embed took {:?}", start.elapsed());
            let s: Vec<f32> = [
                "cats sleeping on a couch",
                "a president's speech about serving your country",
                "a turtle swimming in the ocean",
            ]
            .iter()
            .map(|t| dot(&v, &e.embed_query(t).unwrap()))
            .collect();
            println!("image scores {s:.3?}");
            assert!(s[0] > s[1] && s[0] > s[2]);
            assert!((s[0] - 0.740).abs() < 0.05);
        }
    }
}
