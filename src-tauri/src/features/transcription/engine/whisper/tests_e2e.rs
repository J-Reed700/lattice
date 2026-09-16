//! End-to-end proof that the whisper decode loop transcribes real audio.
//!
//! Ignored by default: it needs a downloaded whisper model, so CI never runs it.
//! Point it at a model directory and an audio file and run it by hand:
//!
//! ```text
//! cd src-tauri
//! LATTICE_WHISPER_E2E_DIR=/path/to/{model.gguf,config.json,tokenizer.json,melfilters.bytes} \
//! LATTICE_WHISPER_E2E_AUDIO=/path/to/speech.wav \
//!   cargo test features::transcription -- --ignored --nocapture
//! ```
//!
//! The engine is constructed directly from the model directory, bypassing
//! `DownloadedModelRepository`, so no database and no download are involved.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::PathBuf;
use std::time::Instant;

use super::*;
use crate::features::transcription::engine::transcript::{
    format_timestamp, group_segments_into_windows, render_transcript, TRANSCRIPT_WINDOW_SECS,
};

fn env_path(key: &str) -> PathBuf {
    let raw =
        std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set for the whisper e2e test"));
    PathBuf::from(raw)
}

/// Load the model straight from a directory, no repository lookup.
fn load_from_dir(dir: PathBuf) -> LoadedWhisper {
    let resolved = ResolvedModel {
        model_id: "e2e-whisper".to_string(),
        name: "Whisper (e2e)".to_string(),
        dir,
    };
    load_whisper(&resolved).expect("whisper model failed to load")
}

/// Real speech in, real transcript out.
///
/// Asserts the words actually come back, that segment timings are sane and
/// monotonic, and that the reported duration matches the file.
#[test]
#[ignore = "needs a downloaded whisper model; set LATTICE_WHISPER_E2E_DIR + LATTICE_WHISPER_E2E_AUDIO"]
fn transcribes_a_real_recording() {
    let model_dir = env_path("LATTICE_WHISPER_E2E_DIR");
    let audio_path = env_path("LATTICE_WHISPER_E2E_AUDIO");

    let load_started = Instant::now();
    let mut loaded = load_from_dir(model_dir);
    let load_ms = load_started.elapsed().as_millis();
    println!("model loaded in {load_ms} ms on {:?}", loaded.device);

    let decode_started = Instant::now();
    let decoded = decode_to_mono_16k(&audio_path, MAX_AUDIO_SECS).expect("audio failed to decode");
    let decode_ms = decode_started.elapsed().as_millis();
    println!(
        "decoded {} samples ({} ms of audio, source {} Hz / {} ch) in {decode_ms} ms",
        decoded.samples.len(),
        decoded.duration_ms,
        decoded.source_sample_rate,
        decoded.channels
    );

    let transcribe_started = Instant::now();
    let (segments, language) =
        transcribe_pcm(&mut loaded, &decoded.samples).expect("transcription failed");
    let transcribe_ms = transcribe_started.elapsed().as_millis();

    let joined = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    println!("language: {language}");
    println!("transcribed in {transcribe_ms} ms");
    println!("segments: {}", segments.len());
    for segment in &segments {
        println!(
            "  [{} – {}] {}",
            format_timestamp(segment.start_ms),
            format_timestamp(segment.end_ms),
            segment.text
        );
    }
    println!("--- transcript ---\n{joined}\n---");

    let windows = group_segments_into_windows(&segments, TRANSCRIPT_WINDOW_SECS);
    let rendered = render_transcript(&windows);
    println!("--- rendered document text ---\n{rendered}\n---");

    // The words actually came back.
    let lowered = joined.to_lowercase();
    assert!(
        lowered.contains("brown fox"),
        "transcript is missing 'brown fox': {joined}"
    );
    assert!(
        lowered.contains("lazy dog"),
        "transcript is missing 'lazy dog': {joined}"
    );

    // Timings are sane and monotonic.
    assert!(!segments.is_empty(), "no segments were produced");
    let mut previous_start = 0u64;
    for segment in &segments {
        assert!(
            segment.start_ms < segment.end_ms,
            "segment start {} is not before end {}",
            segment.start_ms,
            segment.end_ms
        );
        assert!(
            segment.start_ms >= previous_start,
            "segment starts went backwards: {} after {previous_start}",
            segment.start_ms
        );
        previous_start = segment.start_ms;
    }

    // The transcript covers the file: the last segment ends within 20 % of the
    // real duration.
    let real_ms = decoded.duration_ms as f64;
    let covered_ms = segments
        .last()
        .map(|segment| segment.end_ms as f64)
        .unwrap_or(0.0);
    let drift = (covered_ms - real_ms).abs() / real_ms;
    println!(
        "coverage: {covered_ms} ms of {real_ms} ms ({:.1} % drift)",
        drift * 100.0
    );
    assert!(
        drift <= 0.20,
        "transcript covers {covered_ms} ms but the file is {real_ms} ms ({:.1} % drift)",
        drift * 100.0
    );
}

/// A [`TranscriptionPort`] that loads straight from a directory.
///
/// The production port resolves the model through `DownloadedModelRepository`;
/// this one skips that so the ingest path can be exercised without a database
/// row or a download.
struct DirectWhisperPort {
    dir: PathBuf,
}

#[async_trait::async_trait]
impl TranscriptionPort for DirectWhisperPort {
    async fn transcribe(&self, audio: &std::path::Path) -> Result<Transcript, AppError> {
        let mut loaded = load_from_dir(self.dir.clone());
        let decoded = decode_to_mono_16k(audio, MAX_AUDIO_SECS)?;
        let (segments, language) = transcribe_pcm(&mut loaded, &decoded.samples)?;
        Ok(Transcript {
            segments,
            language,
            duration_ms: decoded.duration_ms,
        })
    }

    async fn is_ready(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    async fn active_model_name(&self) -> Result<Option<String>, AppError> {
        Ok(Some("Whisper (e2e)".to_string()))
    }
}

/// The ingest path end to end: adapter → transcript text with `[m:ss–m:ss]`
/// markers → `extract_section` turning a chunk of that text back into a
/// "m:ss–m:ss" range for `Chunk.section`.
#[test]
#[ignore = "needs a downloaded whisper model; set LATTICE_WHISPER_E2E_DIR + LATTICE_WHISPER_E2E_AUDIO"]
fn ingest_path_produces_timestamped_sections() {
    use crate::application::ports::content_extraction_port::ContentExtractionPort;
    use crate::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;
    use crate::infrastructure::services::metadata_extraction::MetadataExtractor;

    let model_dir = env_path("LATTICE_WHISPER_E2E_DIR");
    let audio_path = env_path("LATTICE_WHISPER_E2E_AUDIO");

    let port: Arc<dyn TranscriptionPort> = Arc::new(DirectWhisperPort { dir: model_dir });
    let adapter = ContentExtractionAdapter::with_transcription(port);

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let extracted = runtime
        .block_on(adapter.extract_content(&audio_path))
        .expect("audio extraction failed");

    println!("mime: {}", extracted.mime_type);
    println!("words: {}", extracted.word_count);
    println!("--- extracted text ---\n{}\n---", extracted.text);

    assert!(
        extracted.mime_type.starts_with("audio/"),
        "expected an audio mime type, got {}",
        extracted.mime_type
    );

    // The document text carries the window markers used by citation labels
    // and the AudioViewer seek both read.
    let marker =
        lazy_regex::regex!(r"\[\d{1,2}:\d{2}(?::\d{2})?\u{2013}\d{1,2}:\d{2}(?::\d{2})?\]");
    assert!(
        marker.is_match(&extracted.text),
        "no [m:ss–m:ss] marker in the extracted text: {}",
        extracted.text
    );
    assert!(
        extracted.text.to_lowercase().contains("brown fox"),
        "extracted text is missing the speech: {}",
        extracted.text
    );

    // A chunk of that text yields a "m:ss–m:ss" section.
    let section = MetadataExtractor::new()
        .extract_section(&extracted.text)
        .expect("extract_section returned None for transcript text");
    println!("section: {section}");

    let span = lazy_regex::regex!(r"^\d{1,2}:\d{2}(?::\d{2})?\u{2013}\d{1,2}:\d{2}(?::\d{2})?$");
    assert!(
        span.is_match(&section),
        "section {section:?} is not a m:ss–m:ss range"
    );
}
