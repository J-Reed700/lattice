//! Creating, updating and deleting tags.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

/// Creates a new tag with name, color, and description
///
/// Creates a new tag that can be applied to documents for organization and categorization.
/// Tags support optional visual customization (color) and descriptive metadata. Tag names
/// must be unique and are validated for length and format.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Tag creation request DTO with name, optional color, and description
///
/// # Returns
///
/// * `Ok(CreateTagResponseDto)` - Created tag with ID and status message
/// * `Err(AppError)` - If rate limited, validation fails, or tag creation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many tag creation requests
/// * `AppError::InvalidInput` - Tag name invalid (empty, too long, or > 100 characters)
/// * `AppError::Conflict` - Tag name already exists (duplicate tag names not allowed)
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface CreateTagRequest {
///   name: string;
///   color?: string;
///   description?: string;
/// }
///
/// interface CreateTagResponse {
///   tag: {
///     id: string;
///     name: string;
///     color?: string;
///     description?: string;
///     createdAt: string;
///   };
///   status: string;
/// }
///
/// // Create a simple tag
/// const response = await invoke<CreateTagResponse>('create_tag_ddd', {
///   request: {
///     name: 'Research'
///   }
/// });
///
/// console.log(`Created tag: ${response.tag.name} (${response.tag.id})`);
///
/// // Create tag with color and description
/// const coloredTag = await invoke<CreateTagResponse>('create_tag_ddd', {
///   request: {
///     name: 'Important',
///     color: '#FF5733',
///     description: 'High-priority documents requiring immediate attention'
///   }
/// });
///
/// console.log(`Tag created with color: ${coloredTag.tag.color}`);
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents tag spam and resource exhaustion
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
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Validates tag name length
/// 3. Delegates to `CreateTagUseCase` (business logic + uniqueness check)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (tag name 1-100 characters)
/// 3. Execute CreateTagUseCase (uniqueness check, database insert)
/// 4. Return created tag with ID
///
/// # Use Cases
///
/// - **Organization**: Categorize documents by topic, project, or priority
/// - **Search**: Find documents by tag (see `search_by_tag_ddd`)
/// - **Filtering**: Filter document lists by tag
/// - **Color Coding**: Visual organization with colored tags
pub async fn create_tag_ddd(
    container: State<'_, Container>,
    request: CreateTagRequestDto,
) -> Result<CreateTagResponseDto> {
    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .general
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

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
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Update request DTO with tag ID and optional new values
///
/// # Returns
///
/// * `Ok(TagDto)` - Updated tag with new values
/// * `Err(AppError)` - If rate limited, tag not found, or update fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many update requests
/// * `AppError::NotFound` - Tag ID does not exist
/// * `AppError::Conflict` - New name conflicts with existing tag
/// * `AppError::Other` - Use case execution failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface UpdateTagRequest {
///   id: string;
///   name?: string;
///   color?: string;
///   description?: string;
/// }
///
/// interface Tag {
///   id: string;
///   name: string;
///   color?: string;
///   description?: string;
/// }
///
/// // Update tag name only
/// const updated = await invoke<Tag>('update_tag_ddd', {
///   request: {
///     id: 'tag_123',
///     name: 'Critical Research'
///   }
/// });
///
/// // Update color and description
/// const colorUpdate = await invoke<Tag>('update_tag_ddd', {
///   request: {
///     id: 'tag_123',
///     color: '#00FF00',
///     description: 'Updated priority level'
///   }
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents excessive update operations
/// - **Uniqueness Enforcement**: New names checked for conflicts
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `UpdateTagUseCase` (validation, uniqueness check, update)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute UpdateTagUseCase (find tag, check uniqueness if name changed, update)
/// 3. Return updated tag
pub async fn update_tag_ddd(
    container: State<'_, Container>,
    request: UpdateTagRequestDto,
) -> Result<TagDto> {
    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .general
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let use_case = container.update_tag_use_case();
    let response = use_case.execute(request.clone()).await?;

    Ok(response)
}

/// Deletes a tag and removes all document associations
///
/// Permanently removes a tag from the system. This operation also removes the tag
/// from all documents it was applied to. Cannot be undone.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Delete request DTO with tag ID
///
/// # Returns
///
/// * `Ok(())` - Tag successfully deleted
/// * `Err(AppError)` - If rate limited, tag not found, or deletion fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many deletion requests
/// * `AppError::NotFound` - Tag ID does not exist
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface DeleteTagRequest {
///   tagId: string;
/// }
///
/// // Delete a tag
/// await invoke<void>('delete_tag_ddd', {
///   request: {
///     tagId: 'tag_123'
///   }
/// });
///
/// console.log('Tag deleted successfully');
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents mass deletion abuse
///
/// # Side Effects
///
/// - **Cascade Deletion**: Tag is removed from all documents
/// - **Irreversible**: Tag cannot be recovered after deletion
/// - **Search Impact**: Documents will no longer be findable by this tag
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `DeleteTagUseCase` (cascade deletion logic)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute DeleteTagUseCase (find tag, remove from documents, delete tag)
/// 3. Return success
pub async fn delete_tag_ddd(
    container: State<'_, Container>,
    request: DeleteTagRequestDto,
) -> Result<()> {
    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .general
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let use_case = container.delete_tag_use_case();
    use_case.execute(request.clone()).await?;

    Ok(())
}
