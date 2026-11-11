//! Transcription command implementations.
//!
//! # Commands (3 total)
//!
//! - `transcribe_file` — explicit re-run of the on-device transcriber
//! - `transcribe_speech` — what was said in a recording made in the app
//! - `get_transcription_status` — whether a transcription model is downloaded
//!
//! Normal ingest transcribes audio automatically through the content-extraction
//! adapter; `transcribe_file` exists for a deliberate re-run.

use std::path::PathBuf;

use crate::features::transcription::dto::{
    TranscribedSpeechDto, TranscriptDto, TranscriptionStatusDto,
};
use crate::features::transcription::use_cases::{
    GetTranscriptionStatusUseCase, TranscribeFileUseCase, TranscribeSpeechUseCase,
};
use crate::interfaces::di::Container;
use lattice_core::domain::value_objects::FileFormat;
use lattice_core::shared::error::{AppError, Result};

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

    if !FileFormat::is_audio_path(&validated_path) {
        return Err(AppError::InvalidInput(
            "Only audio files can be transcribed.".to_string(),
        ));
    }

    TranscribeFileUseCase::new(container.transcription_port())
        .execute(&validated_path)
        .await
}

/// Hears a recording made in the app: `pcm` is mono 16-bit samples at 16 kHz, base64.
pub async fn transcribe_speech_impl(
    container: &Container,
    pcm: &str,
) -> Result<TranscribedSpeechDto> {
    TranscribeSpeechUseCase::new(container.transcription_port())
        .execute(pcm)
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
