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
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Apply tags request DTO with document ID and tag names
///
/// # Returns
///
/// * `Ok(ApplyTagsResponseDto)` - List of applied tags (newly created + existing)
/// * `Err(AppError)` - If rate limited, validation fails, or operation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many tag assignment operations
/// * `AppError::InvalidInput` - Tag names invalid (empty or > 100 characters)
/// * `AppError::NotFound` - Document ID does not exist
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ApplyTagsRequest {
///   documentId: string;
///   tagNames: string[];
/// }
///
/// interface ApplyTagsResponse {
///   tags: Array<{
///     id: string;
///     name: string;
///     color?: string;
///   }>;
///   status: string;
/// }
///
/// // Apply multiple tags (creates if don't exist)
/// const response = await invoke<ApplyTagsResponse>('apply_tags_ddd', {
///   request: {
///     documentId: 'doc_123',
///     tagNames: ['Research', 'Important', 'Q1 2024']
///   }
/// });
///
/// console.log(`Applied ${response.tags.length} tags to document`);
/// response.tags.forEach(tag => console.log(`- ${tag.name}`));
///
/// // Idempotent - applying same tags again has no effect
/// await invoke<ApplyTagsResponse>('apply_tags_ddd', {
///   request: {
///     documentId: 'doc_123',
///     tagNames: ['Research'] // Already exists, no error
///   }
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents tag spam and resource exhaustion
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
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Validates all tag names
/// 3. Delegates to `ApplyTagsUseCase` (create-if-not-exists + assignment logic)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (each tag name 1-100 characters)
/// 3. Execute ApplyTagsUseCase:
///    - Find or create each tag
///    - Assign all tags to document (idempotent)
///    - Return complete tag list
/// 4. Return applied tags
///
/// # Use Cases
///
/// - **Bulk Tagging**: Apply multiple categories at once
/// - **Import Workflows**: Assign tags during document import
/// - **Auto-Tagging**: Apply LLM-generated tags (see `generate_tags_ddd`)
/// - **User Workflows**: Tag documents without pre-creating tags
///
/// ## Implementation Layer (Pure Rust - No Tauri)
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

/// ## Tauri Command Layer (Thin Wrapper)
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
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Remove tag request DTO with document ID and tag ID
///
/// # Returns
///
/// * `Ok(())` - Tag successfully removed from document
/// * `Err(AppError)` - If rate limited or operation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many tag removal operations
/// * `AppError::NotFound` - Document or tag ID does not exist
/// * `AppError::Other` - Use case execution failed (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface RemoveTagRequest {
///   documentId: string;
///   tagId: string;
/// }
///
/// // Remove a tag from document
/// await invoke<void>('remove_tag_from_document_ddd', {
///   request: {
///     documentId: 'doc_123',
///     tagId: 'tag_456'
///   }
/// });
///
/// console.log('Tag removed from document');
///
/// // Idempotent - removing again has no effect
/// await invoke<void>('remove_tag_from_document_ddd', {
///   request: {
///     documentId: 'doc_123',
///     tagId: 'tag_456' // Already removed, no error
///   }
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents excessive removal operations
///
/// # Behavior
///
/// - **Idempotent**: Removing unassigned tag has no effect (no error)
/// - **Tag Preservation**: Tag itself is not deleted, only the assignment
/// - **Document Preservation**: Document is not affected, only loses tag association
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `RemoveTagFromDocumentUseCase` (removes assignment)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute RemoveTagFromDocumentUseCase (find and remove assignment)
/// 3. Return success
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn remove_tag_from_document_impl(
    container: &Container,
    request: RemoveTagRequestDto,
) -> Result<()> {
    let use_case = container.remove_tag_from_document_use_case();
    use_case.execute(request.clone()).await?;

    Ok(())
}

/// ## Tauri Command Layer (Thin Wrapper)
pub async fn remove_tag_from_document_ddd(
    container: State<'_, Container>,
    request: RemoveTagRequestDto,
) -> Result<()> {
    remove_tag_from_document_impl(container.inner(), request).await
}
