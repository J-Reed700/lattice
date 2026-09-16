//! Transcription feature dependency injection.
//!
//! Construction is cheap — no file I/O, no model load — so wiring this in does
//! not slow app startup.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::TranscriptionPort;
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::transcription::engine::WhisperTranscriptionService;
use crate::interfaces::di::Container;

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

/// Transcription's registrar surface on `Container`.
impl Container {
    /// On-device speech-to-text. Shared with the content-extraction adapter so
    /// audio ingest and `transcribe_file` run through the same engine.
    pub fn transcription_port(&self) -> Arc<dyn TranscriptionPort> {
        Arc::clone(self.indexing.transcription_port())
    }
}
