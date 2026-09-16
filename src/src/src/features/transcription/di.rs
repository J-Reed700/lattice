//! Transcription feature dependency injection.
//!
//! Construction is cheap — no file I/O, no model load — so wiring this in does
//! not slow app startup.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::TranscriptionPort;
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::transcription::engine::WhisperTranscriptionService;

/// Transcription wiring: one shared port.
#[derive(Clone)]
pub struct TranscriptionDi {
    /// The on-device speech-to-text port.
    pub port: Arc<dyn TranscriptionPort>,
}

/// Build the transcription slice.
pub fn build(db_pool: SqlitePool) -> TranscriptionDi {
    TranscriptionDi {
        port: Arc::new(WhisperTranscriptionService::new(
            DownloadedModelRepository::new(db_pool),
        )) as Arc<dyn TranscriptionPort>,
    }
}
