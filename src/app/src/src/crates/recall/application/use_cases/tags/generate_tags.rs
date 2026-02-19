//! Generate Tags Use Case

use crate::application::dtos::tag_dto::{GenerateTagsRequestDto, GenerateTagsResponseDto};
use crate::infrastructure::services::traits::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for generating tags using LLM
pub struct GenerateTagsUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl GenerateTagsUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(
        &self,
        request: GenerateTagsRequestDto,
    ) -> Result<GenerateTagsResponseDto> {
        // Generate tags using LLM
        let tags = self
            .tag_service
            .generate_tags(&request.document_id, request.max_tags)
            .await?;

        Ok(GenerateTagsResponseDto {
            tags,
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::services::traits::MockTagService;

    #[tokio::test]

    async fn test_generate_tags_success() {
        let mock_service = Arc::new(MockTagService::new());
        let use_case = GenerateTagsUseCase::new(mock_service);

        let request = GenerateTagsRequestDto {
            document_id: "doc-123".to_string(),
            max_tags: 5,
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        // Mock returns default tags
        assert!(!response.tags.is_empty());
    }
}
