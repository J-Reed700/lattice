//! Report whether transcription is available.

use std::sync::Arc;

use crate::features::transcription::dto::{TranscriptionStatusDto, VoiceModelDto};
use lattice_core::application::ports::TranscriptionPort;
use lattice_core::domain::models::curated::{
    get_curated_transcription_models, VOICE_TRANSCRIPTION_MODEL_ID,
};
use lattice_core::shared::error::{AppError, Result};

/// Answers "can Lattice transcribe audio right now?".
///
/// The answer comes from the port, which asks the downloaded-model repository —
/// the SSOT for model presence. No filesystem walk, no model load.
pub struct GetTranscriptionStatusUseCase {
    transcription: Arc<dyn TranscriptionPort>,
}

impl GetTranscriptionStatusUseCase {
    /// Build the use case.
    pub fn new(transcription: Arc<dyn TranscriptionPort>) -> Self {
        Self { transcription }
    }

    /// Report the current transcription status.
    pub async fn execute(&self) -> Result<TranscriptionStatusDto> {
        let model_ready = self.transcription.is_ready().await?;
        let model_name = if model_ready {
            self.transcription.active_model_name().await?
        } else {
            None
        };

        let voice_model = get_curated_transcription_models()
            .into_iter()
            .find(|model| model.id == VOICE_TRANSCRIPTION_MODEL_ID)
            .ok_or_else(|| {
                AppError::InternalError(
                    "The voice transcription model is not in the catalog.".into(),
                )
            })?;

        Ok(TranscriptionStatusDto {
            model_ready,
            model_name,
            voice_model: VoiceModelDto {
                model_id: voice_model.id,
                model_name: voice_model.name,
                download_bytes: voice_model.total_size_bytes as f64,
                license: voice_model.license,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::transcription::engine::WhisperTranscriptionService;
    use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
    use lattice_core::shared::persistence::Database;

    async fn empty_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn reports_not_ready_without_a_model() {
        let pool = empty_pool().await;
        let port = Arc::new(WhisperTranscriptionService::new(
            Arc::new(DownloadedModelRepository::new(Database::new(pool))),
            Arc::default(),
        )) as Arc<dyn TranscriptionPort>;

        let status = GetTranscriptionStatusUseCase::new(port)
            .execute()
            .await
            .unwrap();

        assert!(!status.model_ready);
        assert_eq!(status.model_name, None);
    }

    #[tokio::test]
    async fn offers_the_small_model_for_a_voice_conversation() {
        let pool = empty_pool().await;
        let port = Arc::new(WhisperTranscriptionService::new(
            Arc::new(DownloadedModelRepository::new(Database::new(pool))),
            Arc::default(),
        )) as Arc<dyn TranscriptionPort>;

        let status = GetTranscriptionStatusUseCase::new(port)
            .execute()
            .await
            .unwrap();

        assert_eq!(status.voice_model.model_id, VOICE_TRANSCRIPTION_MODEL_ID);
        assert_eq!(status.voice_model.model_name, "Whisper Tiny");
        assert!(status.voice_model.download_bytes > 0.0);
        assert_eq!(status.voice_model.license, "MIT");
    }
}
