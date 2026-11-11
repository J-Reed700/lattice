//! Generate Tags Use Case

use crate::features::tags::dto::{GenerateTagsRequestDto, GenerateTagsResponseDto};
use crate::features::tags::tag_generation::TagGenerationService;
use lattice_core::shared::error::Result;
use std::sync::Arc;

/// Use case for generating tags using LLM
pub struct GenerateTagsUseCase {
    generation: Arc<TagGenerationService>,
}

impl GenerateTagsUseCase {
    /// Create a new use case instance
    pub fn new(generation: Arc<TagGenerationService>) -> Self {
        Self { generation }
    }

    /// Execute the use case
    pub async fn execute(
        &self,
        request: GenerateTagsRequestDto,
    ) -> Result<GenerateTagsResponseDto> {
        let tags = self
            .generation
            .generate_tags(&request.document_id, request.max_tags)
            .await?;

        Ok(GenerateTagsResponseDto {
            tags,
            status: "success".to_string(),
        })
    }
}
