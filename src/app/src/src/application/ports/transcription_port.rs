//! Port for on-device speech-to-text.
//!
//! Audio ingest routes through this port: the extraction adapter asks it for a
//! transcript, the transcript becomes the document's text, and the timestamp
//! windows rendered from its segments become each chunk's `section`.
//!
//! Implementations must be `Send + Sync`, must not load a model until the first
//! `transcribe` call (nothing may block app startup), and must run inference off
//! the async runtime.

use crate::shared::error::AppError;
use async_trait::async_trait;
use std::path::Path;

/// One contiguous run of transcribed speech.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    /// Offset of the first word from the start of the recording.
    pub start_ms: u64,
    /// Offset of the last word from the start of the recording.
    pub end_ms: u64,
    /// The spoken text, trimmed.
    pub text: String,
}

/// The full transcript of one audio file.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    /// Segments in playback order.
    pub segments: Vec<TranscriptSegment>,
    /// BCP-47-ish language tag as reported by the model ("en", "de", …).
    pub language: String,
    /// Decoded audio duration.
    pub duration_ms: u64,
}

/// Port for transcribing an audio file on-device.
#[async_trait]
pub trait TranscriptionPort: Send + Sync {
    /// Transcribe `audio`.
    ///
    /// Errors with [`AppError::ServiceNotAvailable`] when no transcription model
    /// is downloaded, and with [`AppError::InvalidInput`] when the recording is
    /// longer than the supported cap.
    async fn transcribe(&self, audio: &Path) -> Result<Transcript, AppError>;

    /// Whether a transcription model is downloaded and loadable.
    ///
    /// This is a repository query — it never walks the filesystem for state and
    /// never loads the model (Repository Barrier, `CLAUDE.md`).
    async fn is_ready(&self) -> Result<bool, AppError>;

    /// Display name of the model that would be used, if any.
    async fn active_model_name(&self) -> Result<Option<String>, AppError>;
}
