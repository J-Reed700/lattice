use std::sync::Arc;

use crate::application::ports::DocumentRepositoryPort;
use crate::features::file::dto::UpdateFileMetadataRequestDto;
use crate::features::tags::TagServiceTrait;
use crate::shared::error::{AppError, Result};

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

        let existing_tags = self
            .tag_service
            .get_tags_for_document(&request.document_id)
            .await?;

        for tag in existing_tags {
            self.tag_service
                .remove_tag_from_document(&request.document_id, tag.id().as_str())
                .await?;
        }

        if tags.is_empty() {
            return Ok(());
        }

        self.tag_service
            .apply_tags(&request.document_id, tags)
            .await?;

        Ok(())
    }
}
