# Decisions

Short records of choices made during the local first rework. Newest at the bottom.

## Embedding runtime: ONNX Runtime via `ort` (Phase 2)

- **Model:** `google/embeddinggemma-2` (Apache 2.0). Text, images, video and audio share one 768-dim space, so a text query can match a photo directly.
- **Export:** `onnx-community/embeddinggemma-2-ONNX`. It ships separate graphs: `model` (text, which also takes the media features), `vision_encoder` and `audio_encoder`.
- **Runtime:** `ort = 2.0.0-rc.15` (ONNX Runtime 1.31), CPU only. `candle` and a `llama.cpp` sidecar weren't needed because a working official ONNX export exists.
- **No CoreML:** measured on an M-series Mac in a release build, the CoreML provider took 10.5 s for the first image and 119 s for the second (it recompiles for each input shape). CPU took 6 s and 5 s at budget 280. CoreML also rejects the text model's empty media inputs. `ort` turns on ONNX Runtime's auto device policy by default, which quietly picks CoreML on macOS, so sessions set `AutoDevicePolicy::PreferCPU` explicitly.
- **Vision token budget 140** (the model allows 70 to 1120, default 280). Cats example scores on CPU:

  | Budget | 2nd image | cats / speech / turtle |
  |---|---|---|
  | 280 | 5.1 s | 0.744 / 0.460 / 0.497 |
  | 140 | 2.1 s | 0.742 / 0.481 / 0.519 |
  | 70 | 1.0 s | 0.754 / 0.479 / 0.520 |

  140 keeps a clear margin and is the model's own default for video frames. It's a single constant (`MAX_SOFT_TOKENS`) if photo quality needs more.
- **Verified:** the text reference (Mars doc) scores 0.853 here against 0.854 on the card.
- **Quantization:** q8 (`*_quantized.onnx`). Text is 314 MB and vision 195 MB, with 0.9997 worst-case similarity to fp32 per the export's card. q4 loses more (0.975). fp16 isn't used because the base model card warns it produces NaNs.
- **Prompts:** these come from the model card. Queries use `task: search result | query: {q}`. Documents use `title: {file name} | text: {chunk}`. Images take no prefix.
- **Image preprocessing:** reimplemented in Rust from transformers.js `Gemma4ImageProcessor`:
  - aspect-preserving resize to sides that are multiples of 48 px, within the patch budget (budget x 9 patches)
  - rescale to 0..1, with no mean/std normalization
  - cut into 16x16 patches in HWC order, with (col, row) position ids padded with -1
  - the text model then gets `<|image>` + N x `<|image|>` + `<image|>`, where N = patches / 9
  - resizing uses Catmull-Rom (bicubic); the reference uses PIL bicubic, so scores can differ slightly
- **HEIC:** the `image` crate can't decode it, so HEIC files are converted with macOS's built-in `/usr/bin/sips` first. This only works on macOS.
- **Audio encoder:** not used. Audio and video go through Whisper, and the transcript is embedded as text. That keeps timestamps for snippets, avoids a 340 MB download, and gets past the audio encoder's short input window.
