use std::sync::Arc;

use crate::features::file::dto::UpdateFileMetadataRequestDto;
use lattice_core::application::ports::DocumentRepositoryPort;
use lattice_core::application::ports::TagServiceTrait;
use lattice_core::shared::error::{AppError, Result};

pub struct UpdateFileMetadataUseCase {
    document_repository: Arc<dyn DocumentRepositoryPort>,
    tag_service: Arc<dyn TagServiceTrait>,
}

impl UpdateFileMetadataUseCase {
    pub fn new(
        document_repository: Arc<dyn DocumentRepositoryPort>,
        tag_service: Arc<dyn TagServiceTrait>,
    ) -> Self {
        Self {
            document_repository,
            tag_service,
        }
    }

    pub async fn execute(&self, request: UpdateFileMetadataRequestDto) -> Result<()> {
        let exists = self
            .document_repository
            .document_exists(&request.document_id)
            .await?;

        if !exists {
            return Err(AppError::NotFound(format!(
                "Document not found: {}",
                request.document_id
            )));
        }

        let Some(tags) = request.tags else {
            return Ok(());
        };

        let _guard = self
            .tag_service
            .acquire_lock_with_timeout(&request.document_id)
            .await?;

        self.tag_service
            .replace_tags(&request.document_id, tags)
            .await?;

        Ok(())
    }
}
