//! Create Tag Use Case

use crate::features::tags::dto::{CreateTagRequestDto, CreateTagResponseDto, TagDto};
use crate::features::tags::TagServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for creating a new tag
pub struct CreateTagUseCase {
    tag_service: Arc<dyn TagServiceTrait>,
}

impl CreateTagUseCase {
    /// Create a new use case instance
    pub fn new(tag_service: Arc<dyn TagServiceTrait>) -> Self {
        Self { tag_service }
    }

    /// Execute the use case
    pub async fn execute(&self, request: CreateTagRequestDto) -> Result<CreateTagResponseDto> {
        let tag = self
            .tag_service
            .create_tag(&request.name, request.color.as_deref())
            .await?;

        let tag_dto = TagDto {
            id: tag.id().as_str().to_string(),
            name: tag.name().as_str().to_string(),
            color: Some(tag.color().to_string()),
            description: request.description,
        };

        Ok(CreateTagResponseDto {
            tag: tag_dto,
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tags::mocks::MockTagService;

    #[tokio::test]

    async fn test_create_tag_success() {
        let mock_service = Arc::new(MockTagService::new());
        let use_case = CreateTagUseCase::new(mock_service);

        let request = CreateTagRequestDto {
            name: "Important".to_string(),
            color: Some("#FF0000".to_string()),
            description: Some("Important documents".to_string()),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(response.tag.name, "important"); // normalized
        assert_eq!(response.tag.color, Some("#FF0000".to_string()));
    }
}
