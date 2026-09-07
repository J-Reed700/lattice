//! Content extraction adapter implementing ContentExtractionPort.
//!
//! This is the one place the ingest path branches for audio. Putting the branch
//! here (rather than in `ContentExtractor`, which is a pure, DI-free struct
//! constructed in five places) keeps the audio path in one file and leaves every
//! existing call site's behaviour unchanged.

use crate::application::ports::content_extraction_port::{
    ContentExtractionPort, ExtractedContentData,
};
use crate::application::ports::TranscriptionPort;
use crate::features::transcription::engine::{
    group_segments_into_windows, render_transcript, TRANSCRIPT_WINDOW_SECS,
};
use crate::infrastructure::indexing::extraction::ContentExtractor;
use crate::infrastructure::services::file_type_detector::{
    FileCategory, FileTypeDetector,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, instrument};

/// Shown when audio is ingested with no transcription model downloaded.
///
/// Must begin with this exact phrase — the indexing UI keys off it
/// (GROUND-RULES §4.16).
const NEEDS_MODEL: &str =
    "Needs a transcription model — download Whisper Tiny in Settings → AI → Models.";

/// Adapter wrapping ContentExtractor as ContentExtractionPort.
pub struct ContentExtractionAdapter {
    extractor: ContentExtractor,
    transcription: Option<Arc<dyn TranscriptionPort>>,
}

impl ContentExtractionAdapter {
    pub fn new() -> Self {
        Self {
            extractor: ContentExtractor::new(),
            transcription: None,
        }
    }

    /// Enables audio ingest. Without a port, audio files are reported
    /// unsupported exactly as they were before transcription existed.
    pub fn with_transcription(transcription: Arc<dyn TranscriptionPort>) -> Self {
        Self {
            extractor: ContentExtractor::new(),
            transcription: Some(transcription),
        }
    }

    async fn extract_audio(&self, path: &Path) -> Result<ExtractedContentData> {
        let Some(port) = self.transcription.as_ref() else {
            return Err(AppError::ContentExtraction {
                path: path.display().to_string(),
                reason: NEEDS_MODEL.into(),
            });
        };

        if !port.is_ready().await.unwrap_or(false) {
            return Err(AppError::ContentExtraction {
                path: path.display().to_string(),
                reason: NEEDS_MODEL.into(),
            });
        }

        let start = Instant::now();
        let transcript = port.transcribe(path).await?;
        let windows = group_segments_into_windows(&transcript.segments, TRANSCRIPT_WINDOW_SECS);
        let text = render_transcript(&windows);

        if text.trim().is_empty() {
            return Err(AppError::ContentExtraction {
                path: path.display().to_string(),
                reason: "No speech was found in this recording.".into(),
            });
        }

        let mime_type = FileTypeDetector::detect(path)?.mime_type;

        info!(
            duration_ms = start.elapsed().as_millis(),
            audio_ms = transcript.duration_ms,
            segments = transcript.segments.len(),
            language = %transcript.language,
            "Transcription completed"
        );

        Ok(ExtractedContentData {
            word_count: text.split_whitespace().count(),
            char_count: text.chars().count(),
            text,
            mime_type,
            page_count: None,
        })
    }
}

/// Whether this path is an audio file the transcription path should handle.
fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| FileTypeDetector::get_category(&ext.to_ascii_lowercase()) == FileCategory::Audio)
        .unwrap_or(false)
}

impl Default for ContentExtractionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentExtractionPort for ContentExtractionAdapter {
    #[instrument(skip(self), fields(file_path = %path.display()))]
    async fn extract_content(&self, path: &Path) -> Result<ExtractedContentData> {
        if is_audio(path) {
            return self.extract_audio(path).await;
        }

        let start = Instant::now();

        let extracted = self.extractor.extract_from_file(path).await?;

        let duration = start.elapsed();
        let file_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown");

        info!(
            duration_ms = duration.as_millis(),
            file_type = file_extension,
            mime_type = %extracted.mime_type,
            word_count = extracted.metadata.word_count,
            char_count = extracted.metadata.char_count,
            page_count = ?extracted.metadata.page_count,
            "Content extraction completed"
        );

        Ok(ExtractedContentData {
            text: extracted.text,
            mime_type: extracted.mime_type,
            page_count: extracted.metadata.page_count,
            word_count: extracted.metadata.word_count,
            char_count: extracted.metadata.char_count,
        })
    }

    fn is_supported(&self, path: &Path) -> bool {
        (self.transcription.is_some() && is_audio(path)) || self.extractor.is_supported(path)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn audio_without_a_port_is_unsupported_and_says_so() {
        let adapter = ContentExtractionAdapter::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice-memo.mp3");
        std::fs::write(&path, b"not really audio").unwrap();

        let error = adapter.extract_content(&path).await.unwrap_err();

        match error {
            AppError::ContentExtraction { reason, .. } => {
                assert!(
                    reason.starts_with("Needs a transcription model"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected ContentExtraction, got {other:?}"),
        }

        // `is_supported` stays true: `mime::supported_extensions()` lists the
        // audio extensions so `BatchFileImportService` (which calls
        // `ContentExtractor::is_supported` directly) does not fail the whole
        // batch fast. The honest "no model" answer is given at extraction time,
        // per file, which is what leaves the file unindexed and retried.
        assert!(adapter.is_supported(&path));
    }

    #[tokio::test]
    async fn non_audio_still_routes_to_the_extractor() {
        let adapter = ContentExtractionAdapter::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, b"# Heading\n\nSome body text.\n").unwrap();

        let extracted = adapter.extract_content(&path).await.unwrap();

        assert!(extracted.text.contains("Some body text"));
        assert!(adapter.is_supported(&path));
    }
}
