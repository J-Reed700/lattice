//! Remove Tag from Document Use Case

use crate::features::tags::dto::RemoveTagRequestDto;
use crate::features::tags::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for removing a tag from a document
pub struct RemoveTagFromDocumentUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl RemoveTagFromDocumentUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: RemoveTagRequestDto) -> Result<()> {
        // Acquire lock for document
        let _guard = self
            .tag_service
            .acquire_lock_with_timeout(&request.document_id)
            .await?;

        self.tag_service
            .remove_tag_from_document(&request.document_id, &request.tag_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tags::mocks::MockTagService;

    #[tokio::test]

    async fn test_remove_tag_from_document() {
        let mock_service = Arc::new(MockTagService::new());

        // First apply tags
        let tags = mock_service
            .apply_tags("doc-123", vec!["tag1".to_string(), "tag2".to_string()])
            .await
            .unwrap();

        let use_case = RemoveTagFromDocumentUseCase::new(mock_service.clone());

        let request = RemoveTagRequestDto {
            document_id: "doc-123".to_string(),
            tag_id: tags[0].id().to_string(),
        };

        use_case.execute(request).await.unwrap();

        // Verify only one tag remains
        let remaining_tags = mock_service.get_tags_for_document("doc-123").await.unwrap();
        assert_eq!(remaining_tags.len(), 1);
    }
}
