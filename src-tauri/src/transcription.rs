//! Local speech-to-text with no external tools.
//!
//! Audio (and the audio track of mp4/mov) is decoded with symphonia,
//! downmixed to mono, resampled to 16 kHz with rubato, then transcribed by
//! whisper.cpp linked in through `whisper-rs` (Metal on Apple Silicon).

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use serde::Serialize;
use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const WHISPER_MODEL: &str = "whisper/ggml-base.bin";
const WHISPER_RATE: u32 = 16_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// Loaded per file and dropped after: a context kept alive until process
/// exit makes ggml's Metal teardown abort (GGML_ASSERT in ggml-metal-device.m).
fn load_whisper() -> Result<WhisperContext, String> {
    whisper_rs::install_logging_hooks(); // keep whisper.cpp's chatter off stderr
    let path = crate::models::models_dir().join(WHISPER_MODEL);
    WhisperContext::new_with_params(&path, WhisperContextParameters::default())
        .map_err(|e| format!("Failed to load Whisper model: {e}"))
}

/// Transcribe an audio or video file. A file with no audio track (or only
/// silence) yields no segments rather than an error.
pub async fn transcribe(path: &Path) -> Result<Vec<TranscriptSegment>, String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let Some(samples) = decode_mono_16k(&path)? else {
            return Ok(Vec::new());
        };
        run_whisper(&samples)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn run_whisper(samples: &[f32]) -> Result<Vec<TranscriptSegment>, String> {
    let ctx = load_whisper()?;
    let mut state = ctx.create_state().map_err(|e| format!("Whisper state error: {e}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("auto"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state
        .full(params, samples)
        .map_err(|e| format!("Transcription failed: {e}"))?;

    let mut segments = Vec::new();
    for seg in state.as_iter() {
        let text = seg.to_str_lossy().map_err(|e| e.to_string())?.trim().to_string();
        if !text.is_empty() {
            // whisper timestamps are in centiseconds
            segments.push(TranscriptSegment {
                start_ms: seg.start_timestamp() * 10,
                end_ms: seg.end_timestamp() * 10,
                text,
            });
        }
    }
    Ok(segments)
}

/// Decode the default audio track to 16 kHz mono f32. `None` if the file has
/// no audio track.
fn decode_mono_16k(path: &Path) -> Result<Option<Vec<f32>>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open media: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("Unsupported media format: {e}"))?;

    let Some(track) = format.default_track(TrackType::Audio) else {
        return Ok(None);
    };
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("Audio track has no codec parameters")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| format!("Unsupported audio codec (HE-AAC is not supported): {e}"))?;

    let mut mono = Vec::new();
    let mut rate = 0;
    let mut buf: Vec<f32> = Vec::new();
    while let Some(packet) = format.next_packet().map_err(|e| format!("Failed to read media: {e}"))? {
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                rate = audio.spec().rate();
                let channels = audio.spec().channels().count().max(1);
                buf.resize(audio.samples_interleaved(), 0.0);
                audio.copy_to_slice_interleaved(&mut buf);
                mono.extend(downmix(&buf, channels));
            }
            // A corrupt packet loses a few ms of audio; keep going.
            Err(SymphoniaError::DecodeError(_)) => {}
            Err(e) => return Err(format!("Audio decode failed: {e}")),
        }
    }
    if mono.is_empty() {
        return Ok(None);
    }
    resample(&mono, rate).map(Some)
}

fn downmix(interleaved: &[f32], channels: usize) -> impl Iterator<Item = f32> + '_ {
    interleaved
        .chunks_exact(channels)
        .map(move |frame| frame.iter().sum::<f32>() / channels as f32)
}

fn resample(mono: &[f32], rate: u32) -> Result<Vec<f32>, String> {
    if rate == WHISPER_RATE {
        return Ok(mono.to_vec());
    }
    let mut resampler = Fft::<f32>::new(rate as usize, WHISPER_RATE as usize, 1024, 1, FixedSync::Input)
        .map_err(|e| format!("Resampler error: {e}"))?;
    let input = InterleavedSlice::new(mono, 1, mono.len()).map_err(|e| e.to_string())?;
    let output = resampler
        .process_all(&input, mono.len(), None)
        .map_err(|e| format!("Resample failed: {e}"))?;
    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let stereo = [1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
        assert_eq!(downmix(&stereo, 2).collect::<Vec<_>>(), vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn resample_48k_to_16k_keeps_duration() {
        let one_second: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = resample(&one_second, 48_000).unwrap();
        assert!((out.len() as i64 - 16_000).abs() < 50, "{}", out.len());
    }

    /// Needs the Whisper model. Generates speech with macOS `say` as WAV
    /// (44.1 kHz) and AAC in an m4a container, so both resampling and the
    /// AAC/mp4 path are exercised.
    #[tokio::test]
    #[ignore]
    async fn transcribes_wav_and_m4a() {
        let dir = tempfile::tempdir().unwrap();
        let phrase = "The quarterly budget meeting moved to Thursday.";
        for (name, fmt) in [("speech.wav", "LEI16@44100"), ("speech.m4a", "aac")] {
            let out = dir.path().join(name);
            let ok = std::process::Command::new("say")
                .args(["-o"])
                .arg(&out)
                .args(["--data-format", fmt, phrase])
                .status()
                .unwrap()
                .success();
            assert!(ok, "say failed for {name}");

            let start = std::time::Instant::now();
            let segs = transcribe(&out).await.unwrap();
            let text = segs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
            println!("{name} ({:?}): {text:?} {segs:?}", start.elapsed());
            let lower = text.to_lowercase();
            assert!(lower.contains("budget") && lower.contains("thursday"), "{text}");
            assert!(segs[0].end_ms > segs[0].start_ms);
        }
    }
}
