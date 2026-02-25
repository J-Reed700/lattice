//! Tag Command Handlers (DDD Architecture)
//!
//! Thin controllers for document tagging operations following Domain-Driven Design.
//! Provides CRUD operations, query commands, tag assignment, and LLM-based tag generation.
//! These commands apply cross-cutting concerns (rate limiting, validation, audit logging)
//! and delegate business logic to dedicated use cases.

use crate::application::dtos::tag_dto::*;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use tauri::State;

// ============================================================================
// Tag CRUD Operations
// ============================================================================

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
/// - **Audit Logging (CWE-778)**: Logs tag creation with tag ID and name
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
/// 4. Logs audit event with tag ID and name
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (tag name 1-100 characters)
/// 3. Execute CreateTagUseCase (uniqueness check, database insert)
/// 4. Log audit event (tag created)
/// 5. Return created tag with ID
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

    // Execute use case
    let use_case = container.create_tag_use_case();
    let response = use_case.execute(request).await?;

    // Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::TagCreated,
        response.tag.id(),
        "tag_name" => response.tag.name()
    )
    .await
    .ok();

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
/// - **Audit Logging (CWE-778)**: Logs tag updates with tag ID
/// - **Uniqueness Enforcement**: New names checked for conflicts
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `UpdateTagUseCase` (validation, uniqueness check, update)
/// 3. Logs audit event
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute UpdateTagUseCase (find tag, check uniqueness if name changed, update)
/// 3. Log audit event (tag updated)
/// 4. Return updated tag
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

    // Execute use case
    let use_case = container.update_tag_use_case();
    let response = use_case.execute(request.clone()).await?;

    // Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::TagUpdated,
        request.id.as_str()
    )
    .await
    .ok();

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
/// - **Audit Logging (CWE-778)**: Logs tag deletions for compliance
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
/// 3. Logs audit event
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute DeleteTagUseCase (find tag, remove from documents, delete tag)
/// 3. Log audit event (tag deleted)
/// 4. Return success
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

    // Execute use case
    let use_case = container.delete_tag_use_case();
    use_case.execute(request.clone()).await?;

    // Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::TagDeleted,
        request.tag_id.as_str()
    )
    .await
    .ok();

    Ok(())
}

// ============================================================================
// Tag Query Operations
// ============================================================================

/// Retrieves all tags in the system
///
/// Returns a complete list of all tags, useful for displaying available tags in UI
/// for filtering or tag selection interfaces.
///
/// # Returns
///
/// * `Ok(Vec<TagDto>)` - List of all tags with metadata
/// * `Err(AppError)` - If query fails (database error)
///
/// # Example
///
/// ```typescript
/// const tags = await invoke<Array<{
///   id: string;
///   name: string;
///   color?: string;
///   description?: string;
/// }>>('get_all_tags_ddd');
///
/// console.log(`Total tags: ${tags.length}`);
/// tags.forEach(tag => console.log(`- ${tag.name}`));
/// ```
///
/// # DDD Architecture
///
/// Simple query command - delegates to `GetTagsUseCase`.
pub async fn get_all_tags_ddd(container: State<'_, Container>) -> Result<Vec<TagDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.execute().await?;

    Ok(tags)
}

/// Retrieves all tags with document counts
///
/// Returns tags with the count of documents each tag is applied to. Useful for
/// displaying tag popularity or filtering empty tags.
///
/// # Returns
///
/// * `Ok(Vec<TagWithCountDto>)` - Tags with document counts
/// * `Err(AppError)` - If query fails
///
/// # Example
///
/// ```typescript
/// const tagsWithCounts = await invoke<Array<{
///   tag: { id: string; name: string };
///   documentCount: number;
/// }>>('get_all_tags_with_counts_ddd');
///
/// tagsWithCounts.forEach(item => {
///   console.log(`${item.tag.name}: ${item.documentCount} documents`);
/// });
/// ```
///
/// # DDD Architecture
///
/// Query command with aggregate - delegates to `GetTagsUseCase.get_with_counts()`.
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn get_all_tags_with_counts_impl(container: &Container) -> Result<Vec<TagWithCountDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.get_with_counts().await?;

    Ok(tags)
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn get_all_tags_with_counts_ddd(
    container: State<'_, Container>,
) -> Result<Vec<TagWithCountDto>> {
    get_all_tags_with_counts_impl(container.inner()).await
}

/// Retrieves all tags applied to a specific document
///
/// Returns the list of tags currently applied to the specified document.
/// Useful for displaying document tags and managing tag assignments.
///
/// # Arguments
///
/// * `document_id` - Document identifier
///
/// # Returns
///
/// * `Ok(Vec<TagDto>)` - List of tags applied to document
/// * `Err(AppError)` - If document not found or query fails
///
/// # Example
///
/// ```typescript
/// const documentTags = await invoke<Array<{
///   id: string;
///   name: string;
///   color?: string;
/// }>>('get_document_tags_ddd', {
///   documentId: 'doc_123'
/// });
///
/// console.log(`Document has ${documentTags.length} tags`);
/// ```
///
/// # DDD Architecture
///
/// Query command - delegates to `GetTagsUseCase.get_for_document()`.
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn get_document_tags_impl(
    container: &Container,
    document_id: String,
) -> Result<Vec<TagDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.get_for_document(document_id).await?;

    Ok(tags)
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn get_document_tags_ddd(
    container: State<'_, Container>,
    document_id: String,
) -> Result<Vec<TagDto>> {
    get_document_tags_impl(container.inner(), document_id).await
}

/// Searches for documents by tag name
///
/// Finds all documents that have the specified tag applied. Supports filtering
/// and pagination for large result sets.
///
/// # Arguments
///
/// * `request` - Search request with tag name and optional filters
///
/// # Returns
///
/// * `Ok(SearchByTagResponseDto)` - Matching documents with metadata
/// * `Err(AppError)` - If tag not found or search fails
///
/// # Example
///
/// ```typescript
/// const results = await invoke<{
///   documents: Array<{
///     id: string;
///     filePath: string;
///     title: string;
///   }>;
///   total: number;
/// }>('search_by_tag_ddd', {
///   request: {
///     tagName: 'Research',
///     limit: 50,
///     offset: 0
///   }
/// });
///
/// console.log(`Found ${results.total} documents with tag "Research"`);
/// ```
///
/// # DDD Architecture
///
/// Query command - delegates to `SearchByTagUseCase`.
pub async fn search_by_tag_ddd(
    container: State<'_, Container>,
    request: SearchByTagRequestDto,
) -> Result<SearchByTagResponseDto> {
    let use_case = container.search_by_tag_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}

// ============================================================================
// Tag Assignment Operations
// ============================================================================

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
/// - **Audit Logging (CWE-778)**: Logs tag application with count
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
/// 4. Logs audit event with tag count
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (each tag name 1-100 characters)
/// 3. Execute ApplyTagsUseCase:
///    - Find or create each tag
///    - Assign all tags to document (idempotent)
///    - Return complete tag list
/// 4. Log audit event (tags_applied with count)
/// 5. Return applied tags
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
    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .general
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Validate tag names
    for name in &request.tag_names {
        if name.is_empty() || name.len() > 100 {
            return Err(AppError::InvalidInput(
                "Each tag name must be 1-100 characters".into(),
            ));
        }
    }

    // Execute use case
    let use_case = container.apply_tags_use_case();
    let response = use_case.execute(request.clone()).await?;

    // Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::Custom("tags_applied".to_string()),
        request.document_id.as_str(),
        "tag_count" => response.tags.len().to_string().as_str()
    )
    .await
    .ok();

    Ok(response)
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
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
/// - **Audit Logging (CWE-778)**: Logs tag removals with document and tag IDs
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
/// 3. Logs audit event
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute RemoveTagFromDocumentUseCase (find and remove assignment)
/// 3. Log audit event (tag_removed)
/// 4. Return success

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn remove_tag_from_document_impl(
    container: &Container,
    request: RemoveTagRequestDto,
) -> Result<()> {
    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .general
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Execute use case
    let use_case = container.remove_tag_from_document_use_case();
    use_case.execute(request.clone()).await?;

    // Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::Custom("tag_removed".to_string()),
        request.document_id.as_str(),
        "tag_id" => request.tag_id.as_str()
    )
    .await
    .ok();

    Ok(())
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn remove_tag_from_document_ddd(
    container: State<'_, Container>,
    request: RemoveTagRequestDto,
) -> Result<()> {
    remove_tag_from_document_impl(container.inner(), request).await
}

// ============================================================================
// LLM-Based Tag Generation
// ============================================================================

/// Generates tags for a document using LLM analysis
///
/// Automatically generates relevant tags for a document by analyzing its content
/// with a Large Language Model. The LLM reads the document text and suggests
/// appropriate categorization tags based on the content's topics, themes, and
/// subject matter. This is useful for bulk tagging, automated workflows, and
/// suggesting tags to users.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Tag generation request DTO with document ID and optional parameters
///
/// # Returns
///
/// * `Ok(GenerateTagsResponseDto)` - Generated tags with confidence scores
/// * `Err(AppError)` - If rate limited, LLM unavailable, or generation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many LLM tag generation requests (strict rate limit)
/// * `AppError::NotFound` - Document ID does not exist
/// * `AppError::Other` - LLM unavailable, generation failed, or document not indexed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface GenerateTagsRequest {
///   documentId: string;
///   maxTags?: number;
///   existingTags?: string[];
/// }
///
/// interface GenerateTagsResponse {
///   tags: Array<{
///     name: string;
///     confidence: number;
///     rationale?: string;
///   }>;
///   modelUsed: string;
/// }
///
/// // Generate tags for a document
/// const response = await invoke<GenerateTagsResponse>('generate_tags_ddd', {
///   request: {
///     documentId: 'doc_123',
///     maxTags: 5
///   }
/// });
///
/// console.log(`Generated ${response.tags.length} tags using ${response.modelUsed}`);
/// response.tags.forEach(tag => {
///   console.log(`- ${tag.name} (confidence: ${tag.confidence})`);
/// });
///
/// // Apply generated tags with user confirmation
/// const confirmedTags = response.tags
///   .filter(tag => tag.confidence > 0.7)
///   .map(tag => tag.name);
///
/// await invoke('apply_tags_ddd', {
///   request: {
///     documentId: 'doc_123',
///     tagNames: confirmedTags
///   }
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Strict rate limiting on LLM calls (expensive operations)
/// - **Audit Logging (CWE-778)**: Logs tag generation with success/failure and tag count
/// - **Resource Protection**: LLM operations are costly - rate limiter prevents abuse
///
/// # LLM Tag Generation Process
///
/// 1. **Content Retrieval**: Fetch document content from index
/// 2. **Context Building**: Prepare prompt with document text and instructions
/// 3. **LLM Analysis**: Send to configured LLM for tag suggestions
/// 4. **Tag Extraction**: Parse LLM response and extract tag names
/// 5. **Confidence Scoring**: Evaluate tag relevance (if LLM provides scores)
/// 6. **Deduplication**: Remove duplicate or overlapping tags
///
/// # Generation Parameters
///
/// - **maxTags**: Limit number of tags generated (default: 5, recommended: 3-10)
/// - **existingTags**: Provide context of already-applied tags to avoid duplicates
/// - **Model**: Uses configured LLM model (see `get_qa_model`)
///
/// # Tag Quality
///
/// Generated tags are:
/// - **Relevant**: Based on actual document content, not guesses
/// - **Specific**: More specific than generic categories when appropriate
/// - **Actionable**: Useful for search and organization
/// - **Confidence-Scored**: Higher confidence = more relevant
///
/// # Rate Limiting Details
///
/// LLM tag generation has **stricter rate limits** than other operations:
/// - Limits: Lower request rate (LLM operations are expensive)
/// - Purpose: Prevent resource exhaustion and cost overruns
/// - Workaround: For bulk tagging, use `auto_tag_all_documents_ddd` (batched + throttled)
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies strict LLM rate limiting
/// 2. Delegates to `GenerateTagsUseCase` (LLM prompting, tag extraction)
/// 3. Logs audit event (success/failure, tag count)
///
/// # Command Flow
///
/// 1. Rate limiting check (strict LLM limits)
/// 2. Execute GenerateTagsUseCase:
///    - Fetch document content
///    - Build LLM prompt
///    - Call LLM API
///    - Parse and validate tags
///    - Score confidence
/// 3. Log audit event (tags_generated, count)
/// 4. Return generated tags with metadata
///
/// # Performance
///
/// - **Typical Response Time**: 2-10 seconds (depends on LLM and document length)
/// - **Cost**: LLM API calls incur costs (varies by model and provider)
/// - **Optimization**: Cache document embeddings, reuse for multiple operations
///
/// # Use Cases
///
/// - **Automated Tagging**: Suggest tags to users during document import
/// - **Bulk Organization**: Tag large document collections automatically
/// - **Tag Suggestion**: Provide tag recommendations in UI
/// - **Content Analysis**: Understand document themes and topics

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn generate_tags_impl(
    container: &Container,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    // SECURITY: Rate limit LLM tag generation to prevent resource exhaustion (CWE-770)
    container
        .security_context()
        .rate_limiters()
        .llm_tags
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    container.get_or_load_llm().await?;

    let use_case = container.generate_tags_use_case();
    let response = use_case.execute(request.clone()).await;

    // Audit logging
    let logger = crate::audit::get_audit_logger();
    match &response {
        Ok(resp) => {
            crate::audit_success!(
                logger,
                crate::audit::AuditAction::Custom("tags_generated".to_string()),
                request.document_id.as_str(),
                "tags_count" => resp.tags.len().to_string().as_str()
            )
            .await
            .ok();
        }
        Err(e) => {
            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::Custom("tags_generated".to_string()),
                request.document_id.as_str(),
                e.to_string().as_str()
            )
            .await
            .ok();
        }
    }

    response
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn generate_tags_ddd(
    container: State<'_, Container>,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    generate_tags_impl(container.inner(), request).await
}

/// Automatically tags multiple untagged documents using LLM analysis
///
/// Bulk operation that generates and applies tags to multiple documents using LLM
/// analysis. Processes documents in batches with throttling to prevent resource
/// exhaustion. Useful for tagging large document collections after initial import
/// or for periodic re-tagging operations.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Auto-tag request DTO with max documents and generation parameters
///
/// # Returns
///
/// * `Ok(AutoTagResponseDto)` - Statistics (tagged count, skipped, errors)
/// * `Err(AppError)` - If rate limited, validation fails, or operation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many bulk tagging requests (strict LLM rate limit)
/// * `AppError::InvalidInput` - Batch size exceeds hard limit (max 100 documents)
/// * `AppError::Other` - LLM unavailable or bulk operation failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface AutoTagRequest {
///   maxDocuments: number;
///   tagsPerDocument?: number;
///   onlyUntagged?: boolean;
/// }
///
/// interface AutoTagResponse {
///   totalProcessed: number;
///   successCount: number;
///   skippedCount: number;
///   errorCount: number;
///   errors?: Array<{ documentId: string; error: string }>;
/// }
///
/// // Auto-tag up to 50 untagged documents
/// const response = await invoke<AutoTagResponse>('auto_tag_all_documents_ddd', {
///   request: {
///     maxDocuments: 50,
///     tagsPerDocument: 5,
///     onlyUntagged: true
///   }
/// });
///
/// console.log(`Tagged ${response.successCount}/${response.totalProcessed} documents`);
/// console.log(`Skipped: ${response.skippedCount}, Errors: ${response.errorCount}`);
///
/// if (response.errors && response.errors.length > 0) {
///   console.error('Failed documents:', response.errors);
/// }
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Strict LLM rate limiting prevents resource exhaustion
/// - **Hard Batch Limit (CWE-770)**: Maximum 100 documents per batch (DoS prevention)
/// - **Input Validation**: Batch size validated before processing
/// - **Audit Logging (CWE-778)**: Logs bulk operations with statistics
///
/// # Hard Limits (DoS Prevention)
///
/// **CRITICAL SECURITY**: Batch size is hard-limited to prevent abuse:
/// - **Max Batch Size**: 100 documents per request
/// - **Rationale**: LLM calls are expensive (time, compute, cost)
/// - **Enforcement**: Request rejected if `maxDocuments > 100`
/// - **Workaround**: Process large collections in multiple batches
///
/// Example of proper batching:
/// ```typescript
/// async function tagAllDocuments(documentIds: string[]) {
///   const BATCH_SIZE = 100;
///   for (let i = 0; i < documentIds.length; i += BATCH_SIZE) {
///     const batch = documentIds.slice(i, i + BATCH_SIZE);
///     await invoke('auto_tag_all_documents_ddd', {
///       request: { maxDocuments: batch.length }
///     });
///     // Wait between batches to respect rate limits
///     await new Promise(resolve => setTimeout(resolve, 60000));
///   }
/// }
/// ```
///
/// # Processing Behavior
///
/// - **Batch Processing**: Documents processed in configurable batches
/// - **Throttling**: Automatic delays between LLM calls to respect rate limits
/// - **Error Handling**: Individual document failures don't stop batch processing
/// - **Progress**: Returns statistics for monitoring
/// - **Filtering**: Can filter to only untagged documents (`onlyUntagged: true`)
///
/// # Performance
///
/// - **Typical Time**: 2-10 seconds per document (LLM generation time)
/// - **100 Documents**: ~5-15 minutes (with throttling)
/// - **Cost**: LLM API costs scale with document count
/// - **Optimization**: Batch processing amortizes overhead
///
/// # Response Statistics
///
/// - **totalProcessed**: Number of documents attempted
/// - **successCount**: Successfully tagged documents
/// - **skippedCount**: Documents skipped (already tagged, no content, etc.)
/// - **errorCount**: Documents that failed to tag
/// - **errors**: Array of {documentId, error} for failed documents
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies strict LLM rate limiting
/// 2. Validates batch size (hard limit enforcement)
/// 3. Delegates to `AutoTagDocumentsUseCase` (batch processing, throttling, error handling)
/// 4. Logs audit event with statistics
///
/// # Command Flow
///
/// 1. Rate limiting check (strict LLM limits)
/// 2. Validate batch size (<= 100 documents)
/// 3. Execute AutoTagDocumentsUseCase:
///    - Find candidate documents (untagged or all)
///    - For each document in batch:
///      - Generate tags via LLM
///      - Apply tags to document
///      - Handle errors gracefully
///      - Throttle between documents
/// 4. Log audit event (bulk operation stats)
/// 5. Return statistics
///
/// # Use Cases
///
/// - **Initial Organization**: Tag all documents after first import
/// - **Re-Tagging**: Update tags after model improvements
/// - **Maintenance**: Periodically tag new untagged documents
/// - **Migration**: Bulk tag documents moved from other systems
///
/// # Best Practices
///
/// - **Small Batches**: Start with 10-20 documents to test
/// - **Monitor Costs**: Track LLM API usage and costs
/// - **User Feedback**: Allow users to review generated tags
/// - **Respect Limits**: Don't exceed 100 document hard limit
/// - **Batch Delays**: Wait 1+ minute between batches for large collections
pub async fn auto_tag_all_documents_ddd(
    container: State<'_, Container>,
    request: AutoTagRequestDto,
) -> Result<AutoTagResponseDto> {
    // SECURITY: Rate limit to prevent DoS attacks (CWE-770)
    container
        .security_context()
        .rate_limiters()
        .llm_tags
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // SECURITY: Enforce hard limit
    const MAX_AUTO_TAG_DOCUMENTS: usize = 100;
    if request.max_documents > MAX_AUTO_TAG_DOCUMENTS {
        return Err(AppError::InvalidInput(format!(
            "Batch size {} exceeds maximum {}. Please tag documents in smaller batches.",
            request.max_documents, MAX_AUTO_TAG_DOCUMENTS
        )));
    }

    container.get_or_load_llm().await?;

    let use_case = container.auto_tag_all_documents_use_case();
    let response = use_case.execute(request).await?;

    // Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::Custom("auto_tag_all".to_string()),
        "batch",
        "tagged_count" => response.tagged_count.to_string().as_str()
    )
    .await
    .ok();

    Ok(response)
}

// ============================================================================
// Legacy Compatibility Wrappers
// ============================================================================

/// Legacy: Apply tags to a document (compatibility wrapper)
pub async fn apply_tags(
    document_id: String,
    tags: Vec<String>,
    container: State<'_, Container>,
) -> Result<Vec<TagDto>> {
    let request = ApplyTagsRequestDto {
        document_id,
        tag_names: tags,
    };

    let response = apply_tags_ddd(container, request).await?;
    Ok(response.tags)
}

/// Legacy: Get document tags (compatibility wrapper)
pub async fn get_document_tags(
    document_id: String,
    container: State<'_, Container>,
) -> Result<Vec<TagDto>> {
    get_document_tags_ddd(container, document_id).await
}

/// Legacy: Generate tags (compatibility wrapper)
pub async fn generate_tags(
    document_id: String,
    container: State<'_, Container>,
) -> Result<Vec<String>> {
    let request = GenerateTagsRequestDto {
        document_id,
        max_tags: 5,
    };

    let response = generate_tags_ddd(container, request).await?;
    Ok(response.tags)
}

/// Legacy: Get all tags (compatibility wrapper)
pub async fn get_all_tags(container: State<'_, Container>) -> Result<Vec<TagDto>> {
    get_all_tags_ddd(container).await
}

/// Legacy: Get all tags with counts (compatibility wrapper)
pub async fn get_all_tags_with_counts(
    container: State<'_, Container>,
) -> Result<Vec<TagWithCountDto>> {
    get_all_tags_with_counts_ddd(container).await
}

/// Legacy: Search by tag (compatibility wrapper)
pub async fn search_by_tag(
    tag_name: String,
    container: State<'_, Container>,
) -> Result<Vec<String>> {
    let request = SearchByTagRequestDto { tag_name };
    let response = search_by_tag_ddd(container, request).await?;
    Ok(response.document_ids)
}

/// Legacy: Create tag (compatibility wrapper)
pub async fn create_tag(
    name: String,
    color: Option<String>,
    container: State<'_, Container>,
) -> Result<TagDto> {
    let request = CreateTagRequestDto {
        name,
        color,
        description: None,
    };

    let response = create_tag_ddd(container, request).await?;
    Ok(response.tag)
}

/// Legacy: Update tag (compatibility wrapper)
pub async fn update_tag(
    tag_id: String,
    name: Option<String>,
    color: Option<String>,
    container: State<'_, Container>,
) -> Result<TagDto> {
    let request = UpdateTagRequestDto {
        id: tag_id,
        name,
        color,
        description: None,
    };

    update_tag_ddd(container, request).await
}

/// Legacy: Delete tag (compatibility wrapper)
pub async fn delete_tag(tag_id: String, container: State<'_, Container>) -> Result<()> {
    let request = DeleteTagRequestDto { tag_id };
    delete_tag_ddd(container, request).await
}

/// Legacy: Remove tag from document (compatibility wrapper)
pub async fn remove_tag_from_document(
    document_id: String,
    tag_id: String,
    container: State<'_, Container>,
) -> Result<()> {
    let request = RemoveTagRequestDto {
        document_id,
        tag_id,
    };

    remove_tag_from_document_ddd(container, request).await
}

/// Legacy: Auto-tag all documents (compatibility wrapper)
pub async fn auto_tag_all_documents(container: State<'_, Container>) -> Result<usize> {
    let request = AutoTagRequestDto { max_documents: 100 };

    let response = auto_tag_all_documents_ddd(container, request).await?;
    Ok(response.tagged_count)
}
