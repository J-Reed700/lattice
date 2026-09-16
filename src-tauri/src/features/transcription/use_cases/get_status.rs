//! Report whether transcription is available.

use std::sync::Arc;

use crate::application::ports::TranscriptionPort;
use crate::features::transcription::dto::TranscriptionStatusDto;
use crate::shared::error::Result;

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

        Ok(TranscriptionStatusDto {
            model_ready,
            model_name,
        })
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
    use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
    use crate::features::transcription::engine::WhisperTranscriptionService;

    async fn empty_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn reports_not_ready_without_a_model() {
        let pool = empty_pool().await;
        let port = Arc::new(WhisperTranscriptionService::new(
            DownloadedModelRepository::new(pool),
        )) as Arc<dyn TranscriptionPort>;

        let status = GetTranscriptionStatusUseCase::new(port)
            .execute()
            .await
            .unwrap();

        assert!(!status.model_ready);
        assert_eq!(status.model_name, None);
    }
}
