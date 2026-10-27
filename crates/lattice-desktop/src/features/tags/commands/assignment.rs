//! Applying tags to a document and removing them.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result};

/// Applies multiple tags to a document with automatic tag creation
///
/// Assigns multiple tags to a document in a single operation. If any tag names don't
/// exist yet, they are automatically created. This is the primary method for bulk
/// tagging documents and supports the common workflow of applying multiple categories
/// without needing to pre-create tags.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Tag names invalid (empty or > 100 characters)
/// * `AppError::NotFound` - Document ID does not exist
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Security
///
/// - **Input Validation**: Each tag name checked (1-100 characters)
///
/// # Tag Creation Behavior
///
/// - **Create-If-Not-Exists**: Missing tags are automatically created
/// - **Idempotent**: Applying same tag twice has no effect (no duplicate assignments)
/// - **Atomic**: All tags applied or none (transactional)
/// - **Bulk Operation**: More efficient than creating tags individually
///
/// # Tag Validation
///
/// Each tag name must:
/// - Be 1-100 characters long
/// - Not be empty or whitespace-only
/// - Follow same rules as `create_tag_ddd`
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

/// Removes a tag assignment from a document
///
/// Unassigns a specific tag from a document. The tag itself is not deleted,
/// only the association between the tag and document is removed. This operation
/// is idempotent - removing a tag that isn't assigned has no effect.
///
/// # Errors
///
/// * `AppError::NotFound` - Document or tag ID does not exist
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Behavior
///
/// - **Idempotent**: Removing unassigned tag has no effect (no error)
/// - **Tag Preservation**: Tag itself is not deleted, only the assignment
/// - **Document Preservation**: Document is not affected, only loses tag association
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
