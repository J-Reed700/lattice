//! Get Tags Use Case

use crate::application::dtos::tag_dto::{TagDto, TagWithCountDto};
use crate::infrastructure::services::traits::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for retrieving tags
pub struct GetTagsUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl GetTagsUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Get all tags
    pub async fn execute(&self) -> Result<Vec<TagDto>> {
        let tags = self.tag_service.get_all_tags().await?;

        Ok(tags
            .into_iter()
            .map(|tag| TagDto {
                id: tag.id().as_str().to_string(),
                name: tag.name().as_str().to_string(),
                color: Some(tag.color().to_string()),
                description: None,
            })
            .collect())
    }

    /// Get all tags with document counts
    pub async fn get_with_counts(&self) -> Result<Vec<TagWithCountDto>> {
        // Service returns TagWithCount which is an alias to TagWithCountDto
        self.tag_service.get_all_tags_with_counts().await
    }

    /// Get tags for a specific document
    pub async fn get_for_document(&self, document_id: String) -> Result<Vec<TagDto>> {
        let tags = self.tag_service.get_tags_for_document(&document_id).await?;

        Ok(tags
            .into_iter()
            .map(|tag| TagDto {
                id: tag.id().as_str().to_string(),
                name: tag.name().as_str().to_string(),
                color: Some(tag.color().to_string()),
                description: None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::services::traits::MockTagService;

    #[tokio::test]
    async fn test_get_all_tags() {
        let mock_service = Arc::new(MockTagService::new());

        // Create some tags
        mock_service.create_tag("tag1", None).await.unwrap();
        mock_service
            .create_tag("tag2", Some("#FF0000"))
            .await
            .unwrap();

        let use_case = GetTagsUseCase::new(mock_service);

        let result = use_case.execute().await.unwrap();

        assert_eq!(result.len(), 2);
    }

    #[tokio::test]

    async fn test_get_tags_for_document() {
        let mock_service = Arc::new(MockTagService::new());

        // Apply tags to a document
        mock_service
            .apply_tags("doc-123", vec!["tag1".to_string(), "tag2".to_string()])
            .await
            .unwrap();

        let use_case = GetTagsUseCase::new(mock_service);

        let result = use_case
            .get_for_document("doc-123".to_string())
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
    }
}
