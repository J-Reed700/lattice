//! Update Tag Use Case

use crate::features::tags::dto::{TagDto, UpdateTagRequestDto};
use crate::features::tags::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for updating an existing tag
pub struct UpdateTagUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl UpdateTagUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: UpdateTagRequestDto) -> Result<TagDto> {
        // Update the tag
        let tag = self
            .tag_service
            .update_tag(
                &request.id,
                request.name.as_deref(),
                request.color.as_deref(),
            )
            .await?;

        // Convert to DTO
        Ok(TagDto {
            id: tag.id().as_str().to_string(),
            name: tag.name().as_str().to_string(),
            color: Some(tag.color().to_string()),
            description: request.description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tags::mocks::MockTagService;

    #[tokio::test]

    async fn test_update_tag_success() {
        let mock_service = Arc::new(MockTagService::new());

        // First create a tag
        let created_tag = mock_service.create_tag("old_name", None).await.unwrap();

        let use_case = UpdateTagUseCase::new(mock_service);

        let request = UpdateTagRequestDto {
            id: created_tag.id().to_string(),
            name: Some("New Name".to_string()),
            color: Some("#00FF00".to_string()),
            description: None,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.name, "new name"); // normalized
        assert_eq!(result.color, Some("#00FF00".to_string()));
    }
}
