//! Apply Tags Use Case

use crate::application::dtos::tag_dto::{ApplyTagsRequestDto, ApplyTagsResponseDto, TagDto};
use crate::infrastructure::services::traits::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for applying multiple tags to a document
pub struct ApplyTagsUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl ApplyTagsUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: ApplyTagsRequestDto) -> Result<ApplyTagsResponseDto> {
        // Acquire lock for document
        let _guard = self
            .tag_service
            .acquire_lock_with_timeout(&request.document_id)
            .await?;

        // Apply tags
        let tags = self
            .tag_service
            .apply_tags(&request.document_id, request.tag_names)
            .await?;

        // Convert to DTOs
        let tag_dtos: Vec<TagDto> = tags
            .into_iter()
            .map(|tag| TagDto {
                id: tag.id().as_str().to_string(),
                name: tag.name().as_str().to_string(),
                color: Some(tag.color().to_string()),
                description: None,
            })
            .collect();

        Ok(ApplyTagsResponseDto {
            tags: tag_dtos,
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::services::traits::MockTagService;

    #[tokio::test]

    async fn test_apply_tags_success() {
        let mock_service = Arc::new(MockTagService::new());
        let use_case = ApplyTagsUseCase::new(mock_service);

        let request = ApplyTagsRequestDto {
            document_id: "doc-123".to_string(),
            tag_names: vec!["rust".to_string(), "tutorial".to_string()],
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(response.tags.len(), 2);
    }

    #[tokio::test]

    async fn test_apply_tags_merges_with_existing() {
        let mock_service = Arc::new(MockTagService::new());

        // First apply some tags
        mock_service
            .apply_tags("doc-123", vec!["existing".to_string()])
            .await
            .unwrap();

        let use_case = ApplyTagsUseCase::new(mock_service);

        let request = ApplyTagsRequestDto {
            document_id: "doc-123".to_string(),
            tag_names: vec!["rust".to_string(), "tutorial".to_string()],
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.tags.len(), 3); // existing + rust + tutorial
    }
}
