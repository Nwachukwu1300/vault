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

## Transcription: whisper-rs + symphonia, no sidecars (Phase 4)

- **Whisper** is linked in through `whisper-rs 0.16` (whisper.cpp, Metal), with the `ggml-base.bin` model from the models dir. This replaces the rework doc's "bundle whisper-cli and ffmpeg as sidecars": there are no extra binaries to ship or sign, and no fetch script. Building needs `cmake`, which GitHub's macOS runners already have.
- **Decoding** is done by `symphonia 0.6` (wav, mp3, AAC in m4a/mp4/mov), with a mono downmix and `rubato 5` FFT resampling to 16 kHz. This replaces ffmpeg. Known limit: HE-AAC isn't supported, so those files are recorded as failed and the queue moves on.
- **Bounded memory:** audio is resampled to 16 kHz in 30-second blocks while decoding, transcription stops at 2 hours of audio, and sample rates outside 8 to 192 kHz (or changing mid-stream) are rejected. Before this, a long or malformed file could use gigabytes of RAM.
- **Whisper is loaded per file, not cached.** A context kept alive until exit makes ggml's Metal teardown abort (`GGML_ASSERT` in `ggml-metal-device.m`), which would crash the app on quit. Loading takes well under a second, so it's small next to transcription.
- **Verified:** speech made with macOS `say` was transcribed word for word from both WAV (44.1 kHz) and AAC/m4a, with segment timestamps.

## Extraction and chunking (Phase 5)

- **PDF text:** `pdf-extract 0.12`, one segment per page so results can say "page 4". A page with no text layer marks the file `needs_ocr` (OCR comes later). It runs on a blocking thread, so a parser panic on a malformed PDF fails that file instead of crashing the app. PDFs over 200 MB are skipped, because pdf-extract loads the whole file into memory.
- **Chunks:** about 450 tokens, measured with the embedding model's own tokenizer, with 15% overlap. Text is split at paragraphs first, then sentences, then words, so a chunk never cuts a sentence unless the sentence alone is too long. Each chunk keeps its first page, and the timestamps from its first to its last segment.
- **Until Phase 6:** the indexer embeds only the first chunk per file, because the old one-row-per-file table can't hold more.
- **Build:** dependencies are compiled without debug info (`[profile.dev.package."*"] debug = false`). With it, the debug cache reached 25 GB, and linking stalled for 20+ minutes on a USB drive.
