//! Applying tags to a document and removing them.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result};

/// Applies multiple tags to a document, creating missing tags on the fly.
pub async fn apply_tags_impl(
    container: &Container,
    request: ApplyTagsRequestDto,
) -> Result<ApplyTagsResponseDto> {
    for name in &request.tag_names {
        if name.is_empty() || name.len() > 100 {
            return Err(AppError::InvalidInput(
                "Each tag name must be 1-100 characters".into(),
            ));
        }
    }

    let use_case = container.apply_tags_use_case();
    let response = use_case.execute(request.clone()).await?;

    Ok(response)
}

pub async fn apply_tags_ddd(
    container: State<'_, Container>,
    request: ApplyTagsRequestDto,
) -> Result<ApplyTagsResponseDto> {
    apply_tags_impl(container.inner(), request).await
}

/// Removes a tag assignment from a document idempotently without deleting the tag itself.
pub async fn remove_tag_from_document_impl(
    container: &Container,
    request: RemoveTagRequestDto,
) -> Result<()> {
    let use_case = container.remove_tag_from_document_use_case();
    use_case.execute(request.clone()).await?;

    Ok(())
}

pub async fn remove_tag_from_document_ddd(
    container: State<'_, Container>,
    request: RemoveTagRequestDto,
) -> Result<()> {
    remove_tag_from_document_impl(container.inner(), request).await
}
