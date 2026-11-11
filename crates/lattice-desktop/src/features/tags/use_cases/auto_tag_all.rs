//! Auto-Tag All Documents Use Case

use crate::features::tags::dto::{AutoTagRequestDto, AutoTagResponseDto};
use crate::features::tags::tag_generation::TagGenerationService;
use lattice_core::shared::error::Result;
use std::sync::Arc;

/// Use case for auto-tagging all untagged documents
pub struct AutoTagAllDocumentsUseCase {
    generation: Arc<TagGenerationService>,
}

impl AutoTagAllDocumentsUseCase {
    /// Create a new use case instance
    pub fn new(generation: Arc<TagGenerationService>) -> Self {
        Self { generation }
    }

    /// Execute the use case
    pub async fn execute(&self, request: AutoTagRequestDto) -> Result<AutoTagResponseDto> {
        let tagged_count = self
            .generation
            .auto_tag_all_documents(request.max_documents)
            .await?;

        Ok(AutoTagResponseDto {
            tagged_count,
            status: format!("Successfully tagged {} documents", tagged_count),
        })
    }
}
