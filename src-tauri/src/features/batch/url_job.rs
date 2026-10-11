//! A URL import's items: each page is fetched and ingested as one document.
use super::file_job::EmbeddingLoader;
use super::items::BatchItem;
use super::worker::{ItemImporter, ItemResult};
use crate::features::web::dto::IngestWebUrlRequestDto;
use crate::features::web::use_cases::IngestWebUrlUseCase;
use crate::shared::{
    error::{AppError, Result},
    runtime::jobs::JobContext,
};
use async_trait::async_trait;
use std::sync::Arc;

pub(crate) struct UrlImporter {
    pub ingest: Arc<IngestWebUrlUseCase>,
    pub load_embedding: EmbeddingLoader,
}

#[async_trait]
impl ItemImporter for UrlImporter {
    async fn prepare(&self, _context: &JobContext) -> Result<()> {
        (self.load_embedding)().await.map_err(|error| {
            AppError::ServiceNotAvailable(format!(
                "Could not load the active embedding model: {error}"
            ))
        })?;
        Ok(())
    }

    async fn import(&self, context: &JobContext, item: &BatchItem) -> Result<ItemResult> {
        // A page fetch may take long; cancelling drops it at once.
        let ingested = context
            .until_cancelled(self.ingest.execute(IngestWebUrlRequestDto {
                url: item.target.clone(),
            }))
            .await;
        Ok(match ingested {
            None => ItemResult::Stopped,
            Some(Ok(response)) => ItemResult::Imported {
                document_id: response.document_id,
            },
            Some(Err(error)) => ItemResult::Failed {
                error: error.to_string(),
                document_id: None,
            },
        })
    }

    fn noun(&self) -> &'static str {
        "URL"
    }
}
