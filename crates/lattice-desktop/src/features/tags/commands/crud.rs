//! Creating, updating and deleting tags.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result};

/// Creates a new tag with name, color, and description
///
/// Creates a new tag that can be applied to documents for organization and categorization.
/// Tags support optional visual customization (color) and descriptive metadata. Tag names
/// must be unique and are validated for length and format.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Tag name invalid (empty, too long, or > 100 characters)
/// * `AppError::Conflict` - Tag name already exists (duplicate tag names not allowed)
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Security
///
/// - **Input Validation**: Tag name length checked (1-100 characters)
/// - **Uniqueness Enforcement**: Duplicate tag names rejected
///
/// # Tag Naming Rules
///
/// - **Length**: 1-100 characters
/// - **Required**: Name cannot be empty
/// - **Unique**: Tag names must be unique across the system
/// - **Case-Sensitive**: "Research" and "research" are different tags
///
/// # Color Format
///
/// Colors should be hex color codes (e.g., "#FF5733", "#00FF00").
/// - **Format**: `#RRGGBB` (6-digit hex)
/// - **Optional**: Color is not required
/// - **UI Display**: Used for tag badges and labels
pub async fn create_tag_ddd(
    container: State<'_, Container>,
    request: CreateTagRequestDto,
) -> Result<CreateTagResponseDto> {
    // Input validation
    if request.name.is_empty() || request.name.len() > 100 {
        return Err(AppError::InvalidInput(
            "Tag name must be 1-100 characters".into(),
        ));
    }

    let use_case = container.create_tag_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}

/// Updates an existing tag's name, color, or description
///
/// Modifies an existing tag's metadata. All fields are optional - only provided fields
/// will be updated. Tag name uniqueness is enforced if name is changed.
///
/// # Errors
///
/// * `AppError::NotFound` - Tag ID does not exist
/// * `AppError::Conflict` - New name conflicts with existing tag
/// * `AppError::Other` - Use case execution failed
///
/// # Security
///
/// - **Uniqueness Enforcement**: New names checked for conflicts
pub async fn update_tag_ddd(
    container: State<'_, Container>,
    request: UpdateTagRequestDto,
) -> Result<TagDto> {
    let use_case = container.update_tag_use_case();
    let response = use_case.execute(request.clone()).await?;

    Ok(response)
}

/// Permanently deletes a tag and removes it from every document.
///
/// # Errors
///
/// * `AppError::NotFound` - Tag ID does not exist
/// * `AppError::Other` - Use case execution failed (database error)
pub async fn delete_tag_ddd(
    container: State<'_, Container>,
    request: DeleteTagRequestDto,
) -> Result<()> {
    let use_case = container.delete_tag_use_case();
    use_case.execute(request.clone()).await?;

    Ok(())
}
