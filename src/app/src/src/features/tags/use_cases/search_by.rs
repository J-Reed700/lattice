//! Search by Tag Use Case

use crate::features::tags::dto::{SearchByTagRequestDto, SearchByTagResponseDto};
use crate::features::tags::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for searching documents by tag
pub struct SearchByTagUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl SearchByTagUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: SearchByTagRequestDto) -> Result<SearchByTagResponseDto> {
        let document_ids = self
            .tag_service
            .search_documents_by_tag(&request.tag_name)
            .await?;

        let total = document_ids.len();

        Ok(SearchByTagResponseDto {
            document_ids,
            total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tags::mocks::MockTagService;

    #[tokio::test]

    async fn test_search_by_tag() {
        let mock_service = Arc::new(MockTagService::new());

        // Apply same tag to multiple documents
        mock_service
            .apply_tags("doc-1", vec!["rust".to_string()])
            .await
            .unwrap();
        mock_service
            .apply_tags("doc-2", vec!["rust".to_string()])
            .await
            .unwrap();
        mock_service
            .apply_tags("doc-3", vec!["python".to_string()])
            .await
            .unwrap();

        let use_case = SearchByTagUseCase::new(mock_service);

        let request = SearchByTagRequestDto {
            tag_name: "rust".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.total, 2);
        assert!(response.document_ids.contains(&"doc-1".to_string()));
        assert!(response.document_ids.contains(&"doc-2".to_string()));
    }
}
