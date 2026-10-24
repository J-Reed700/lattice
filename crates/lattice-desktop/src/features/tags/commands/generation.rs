//! LLM tag suggestions for one document and auto-tagging the library.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result};

pub async fn generate_tags_impl(
    container: &Container,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    container.default_chat_llm().await?;

    let use_case = container.generate_tags_use_case();
    use_case.execute(request).await
}

pub async fn generate_tags_ddd(
    container: State<'_, Container>,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    generate_tags_impl(container.inner(), request).await
}

pub async fn auto_tag_all_documents_ddd(
    container: State<'_, Container>,
    request: AutoTagRequestDto,
) -> Result<AutoTagResponseDto> {
    const MAX_AUTO_TAG_DOCUMENTS: usize = 100;
    if request.max_documents > MAX_AUTO_TAG_DOCUMENTS {
        return Err(AppError::InvalidInput(format!(
            "Batch size {} exceeds maximum {}. Please tag documents in smaller batches.",
            request.max_documents, MAX_AUTO_TAG_DOCUMENTS
        )));
    }

    container.default_chat_llm().await?;

    let use_case = container.auto_tag_all_documents_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}
