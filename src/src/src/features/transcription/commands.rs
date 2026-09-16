//! Transcription command implementations.
//!
//! # Commands (2 total)
//!
//! - `transcribe_file` — explicit re-run of the on-device transcriber
//! - `get_transcription_status` — whether a transcription model is downloaded
//!
//! Normal ingest transcribes audio automatically through the content-extraction
//! adapter; `transcribe_file` exists for a deliberate re-run.

use std::path::PathBuf;

use crate::features::transcription::dto::{TranscriptDto, TranscriptionStatusDto};
use crate::features::transcription::use_cases::{
    GetTranscriptionStatusUseCase, TranscribeFileUseCase,
};
use crate::infrastructure::services::file_type_detector::{FileCategory, FileTypeDetector};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

/// Transcribes an audio file on-device and returns timestamped segments.
///
/// # Security
///
/// - **Path Validation (CWE-22)**: the path is confined to the configured
///   file-access roots before anything is opened.
pub async fn transcribe_file_impl(container: &Container, path: &str) -> Result<TranscriptDto> {
    let validated_path: PathBuf = container
        .file_access_config()
        .validate_path(path)
        .map_err(|e| AppError::InvalidInput(format!("Invalid path: {}", e)))?;

    let is_audio = validated_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| FileTypeDetector::get_category(&ext.to_ascii_lowercase()) == FileCategory::Audio)
        .unwrap_or(false);

    if !is_audio {
        return Err(AppError::InvalidInput(
            "Only audio files can be transcribed.".to_string(),
        ));
    }

    TranscribeFileUseCase::new(container.transcription_port())
        .execute(&validated_path)
        .await
}

/// Reports whether a transcription model is downloaded.
pub async fn get_transcription_status_impl(
    container: &Container,
) -> Result<TranscriptionStatusDto> {
    GetTranscriptionStatusUseCase::new(container.transcription_port())
        .execute()
        .await
}
