//! Auto-Tag All Documents Use Case

use crate::features::tags::dto::{AutoTagRequestDto, AutoTagResponseDto};
use crate::features::tags::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for auto-tagging all untagged documents
pub struct AutoTagAllDocumentsUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl AutoTagAllDocumentsUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: AutoTagRequestDto) -> Result<AutoTagResponseDto> {
        let tagged_count = self
            .tag_service
            .auto_tag_all_documents(request.max_documents)
            .await?;

        Ok(AutoTagResponseDto {
            tagged_count,
            status: format!("Successfully tagged {} documents", tagged_count),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tags::mocks::MockTagService;

    #[tokio::test]

    async fn test_auto_tag_all_documents() {
        let mock_service = Arc::new(MockTagService::new());
        let use_case = AutoTagAllDocumentsUseCase::new(mock_service);

        let request = AutoTagRequestDto { max_documents: 50 };

        let response = use_case.execute(request).await.unwrap();

        // Mock service returns 0 for auto-tag
        assert_eq!(response.tagged_count, 0);
        assert!(response.status.contains("0 documents"));
    }
}
