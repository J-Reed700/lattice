//! Transcribe one audio file on demand.

use std::path::Path;
use std::sync::Arc;

use crate::application::ports::TranscriptionPort;
use crate::features::transcription::dto::TranscriptDto;
use crate::features::transcription::engine::{
    group_segments_into_windows, render_transcript, TRANSCRIPT_WINDOW_SECS,
};
use crate::shared::error::{AppError, Result};

/// Explicit re-transcription of a single file.
///
/// Normal ingest transcribes automatically through the content-extraction
/// adapter; this is the "run it again" path.
pub struct TranscribeFileUseCase {
    transcription: Arc<dyn TranscriptionPort>,
}

impl TranscribeFileUseCase {
    /// Build the use case.
    pub fn new(transcription: Arc<dyn TranscriptionPort>) -> Self {
        Self { transcription }
    }

    /// Transcribe `path` and render its document text.
    pub async fn execute(&self, path: &Path) -> Result<TranscriptDto> {
        if !self.transcription.is_ready().await? {
            return Err(AppError::ServiceNotAvailable(
                "No transcription model is downloaded.".to_string(),
            ));
        }

        let transcript = self.transcription.transcribe(path).await?;
        let windows = group_segments_into_windows(&transcript.segments, TRANSCRIPT_WINDOW_SECS);
        let text = render_transcript(&windows);

        Ok(TranscriptDto::from_transcript(
            path.display().to_string(),
            &transcript,
            text,
        ))
    }
}
