//! Delete Tag Use Case

use crate::features::tags::dto::DeleteTagRequestDto;
use crate::infrastructure::services::traits::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for deleting a tag
pub struct DeleteTagUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl DeleteTagUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: DeleteTagRequestDto) -> Result<()> {
        self.tag_service.delete_tag(&request.tag_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::services::traits::MockTagService;

    #[tokio::test]
    async fn test_delete_tag_success() {
        let mock_service = Arc::new(MockTagService::new());

        // First create a tag
        let created_tag = mock_service.create_tag("test_tag", None).await.unwrap();

        let use_case = DeleteTagUseCase::new(mock_service.clone());

        let request = DeleteTagRequestDto {
            tag_id: created_tag.id().to_string(),
        };

        use_case.execute(request).await.unwrap();

        // Verify tag is deleted
        let tags = mock_service.get_all_tags().await.unwrap();
        assert!(tags.is_empty());
    }
}
